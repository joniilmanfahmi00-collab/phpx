# PHPX M4 notes - Worker hardening + app compat

Goal: make the Process Runtime robust for real applications. Status: DONE (developer preview, not production SLA).

## What changed vs M3

- Config (`phpx-config`): added `min_workers`, `queue_bound` (auto `workers*4`/min 16 or explicit),
  `queue_wait_timeout_seconds`, `shutdown_drain_timeout_seconds`, `restart_max_retries`,
  `restart_backoff_ms/max_ms`. Validation: `min<=max`, bounds per key. Old configs keep parsing (serde defaults).
- Pool (`phpx-worker`): `min_workers`, `drain_timeout`, `max_requests_per_slot` (logical `recycled`),
  consecutive-failure tracking + exponential backoff with jitter + crash-loop circuit breaker
  (fast-503 while open), `begin_drain()`/`drain(timeout)`, `snapshot()` (active/waiting/failures),
  `prometheus_text()`. One-shot isolation kept; recycling is logical for future persistent workers.
- Gateway (`phpx-server`): `build_state` wires all M4 fields; `GET /metrics` (Prometheus text +
  `phpx_build_info`); `is_overload` also matches `draining`/`circuit` (503 + `Retry-After: 1`);
  shutdown waits Ctrl-C AND SIGTERM (systemd) then `begin_drain()` + `drain(drain_timeout)`.
  Lifecycle doc: Failed = terminal attempt + diagnostics; Stopped = drained, no new work.
- SAPI compat (`phpx-runtime-process`): `php -d auto_prepend_file=<embedded bootstrap>` per request.
  Env bridge: `PHPX_HEADERS_JSON` + existing `PHPX_*`; bootstrap populates `$_GET` (query),
  `$_COOKIE`, `$_SERVER` (`REQUEST_METHOD/REQUEST_URI/QUERY_STRING/HTTP_*`), `getallheaders()`
  polyfill, `phpx_input()` (reads `php://input`, falls back to `php://stdin`).
- CLI: `workers status` shows min/max/bounds/timeouts/policy + `/metrics` hint + lifecycle note;
  `runtime inspect` shows M4 fields; `metrics` documents `/metrics` keys + curl hint;
  `doctor` adds `php-sapi` audit (only `cli` supported) + 11-extension combo check.
- Fixtures (no network): `composer-autoload` (stub autoloader + `$_GET`/`$_COOKIE`/body),
  `laravel-minimal` sim (routing, `/api/hello` JSON, middleware header), `wordpress-minimal`
  sim (`?p=`/`?s=`, shortcode), `woocommerce-minimal` sim (cart/checkout after WP baseline).
- Tests: `tests/m4_compat.rs` (5 tests: isolation/crash, composer+SAPI, laravel, wp+woo, `/metrics`).
  M1-M3 suites still green. `m3_cli` updated for M4 doctor banner.
- Docs: `operations/compatibility-matrix.md` (fixtures x PHP/ext, unsupported explicit),
  `sapi-audit.md` (cli-only, `php://input` limit, audit runbook), `soak.md` (RSS/restart/latency
  report, adaptive disabled), `configuration.md` (M4 keys). Scripts: `scripts/soak.sh`.

## Exit evidence

- `cargo test -p phpx-config -q`, `-p phpx-worker -q`, `-p phpx-runtime-process -q`,
  `-p phpx-server -q`, `-p phpx-cli -q` all green (incl. 5 new M4 compat tests).
- `phpx doctor` reports `php-sapi cli` + 11 extensions; `phpx workers status` shows
  min/max/bounds/policy; `GET /metrics` exposes 10 pool keys.
- `isolate.php` xN stays `clean` with distinct RIDs; `crash.php` (500) followed by `/` (200).
- Compat matrix published; unsupported SAPI/ext explicit; soak runbook ready.

## Deferred (explicit, must not block M4)

- Soft/hard RSS memory guardian (sampling + overshoot semantics): incremental per ADR-0014 Phase B.
- Adaptive min/max scaling: stays DISABLED (fixed-pool baseline only).
- Queue/scheduler/cache/storage: M5. Embedded runtime: M6. HTTP/2: gated.
- `$_FILES`/multipart, `$_SESSION` persistence, FPM semantics: not emulated (see sapi-audit).

Traceability: SPEC 15 items 5-7 + 11-12 complete in M4 (worker scope); full v0.1 acceptance in SPEC 15.
