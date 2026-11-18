# M2 implementation notes (HTTP gateway)

Status: implemented, all exit criteria verified on PHP 8.3.30 / Rust 1.99.1 (Windows dev).

## What landed

- `crates/phpx-server`: Tokio + Hyper HTTP/1.1 listener, host/port from config.
- Routing: document-root + front-controller. Direct `*.php` always dispatches
  (never served raw); exact static files served with MIME types; everything
  else falls through to the configured entrypoint.
- Static safety (SPEC 6.3): `..` segments, backslashes, NUL, and encoded
  `%2e`/`%2f`/`%5c` rejected with 400 before touching the FS; candidates are
  canonicalized and required to start with the canonical docroot (symlink
  escape -> 403); `.php` is never served statically.
- PHP dispatch via `phpx-worker::BoundedPool` inside `spawn_blocking`
  (ADR-0006: blocking PHP stays off Tokio executor threads).
- Request IDs (UUID v4) + `x-request-id` response header + structured access
  logs (`request_id, method, path, status, duration_ms`).
- Body/header limits: `request_body_limit` parsed by `parse_size`
  ("16MB", "256M", "1GB", "512KB", plain bytes); oversize -> 413.
- Backpressure: pool overload maps to 503 + `Retry-After: 1`, never an
  unbounded queue (SPEC 5.3).
- Worker crash (exit != 0) maps to 500 with stderr diagnostic; the gateway
  stays up and the next request succeeds.
- Graceful shutdown: Ctrl-C stops accepting; in-flight PHP is bounded by the
  execution timeout (drain).
- TLS via Rustls: `tls=true` requires `tls_cert_file` + `tls_key_file` (PEM),
  otherwise a clear config error; plain HTTP for tests. HTTP/1.1 only in M2;
  HTTP/2 is deferred (SPEC 6.1 target, gated).

## Config additions (schema still v1)

- `[server] tls_cert_file`, `tls_key_file` (required when `tls=true`).
- `parse_size()` helper + `body_limit_bytes()` + `tls_enabled()`.

## Exit criteria evidence (`cargo test -p phpx-server`)

- `serves_front_controller_and_static`: `/` -> 200 vanilla fixture,
  `/style.css` -> 200 + css content-type, `/hello.php` -> 200 dispatched
  without leaking source.
- `blocks_traversal_and_survives_crash`: raw `%2e`/`..%2f` paths -> 400,
  `/crash.php` -> 500 then `/` -> 200.
- `backpressure_returns_503_not_unbounded`: workers=1 + 5s sleep.php x6
  concurrent -> at least one 503 and one 200.
- Unit: traversal reject + docroot join safety.
- Workspace: `cargo fmt --check` clean,
  `cargo clippy --workspace --all-targets -- -D warnings` clean,
  M1 suites still green (m1_execute 5, m1_pool 2).
