# PHPX Architecture Overview (M1-M5 complete + M6.1 stub, developer preview)

M0 scope: buildable Rust workspace, config v1, `phpx doctor` / `phpx serve` skeleton — DONE.
M1 scope: Process Runtime prototype over one-shot PHP + BoundedPool — DONE, see `docs/architecture/m1-notes.md`.
M2 scope: Tokio + Hyper HTTP/1.1 gateway, front-controller routing, safe static serving, PHP dispatch via pool, backpressure 503, graceful drain, TLS via Rustls - DONE, see m2-notes.md. HTTP/2 is deferred, not claimed in M2.
M3 scope: Developer preview v0.1 - finalized serve/doctor, runtime/workers status, documented TOML defaults, non-root Docker, systemd unit, PHP matrix, smoke + baseline bench, /healthz + /readyz - DONE, see m3-notes.md. v0.1 is developer preview, not production SLA.
M4 scope: Worker hardening + app compat - min/max pool, queue/drain/restart policy, crash-loop circuit, /metrics, SAPI-compat bootstrap ($_GET/$_COOKIE/$_SERVER), Composer/Laravel/WP/Woo fixtures, compat matrix, soak runbook - DONE, see m4-notes.md.
M5 phase 1 scope: durable queue (memory + sqlite, retries/backoff/dead-letter, lease recovery, `queue status|push|work` real) - DONE, see `docs/operations/queue.md`.
M5 phase 2 scope: timezone-aware cron scheduler, single-pass dispatch-only, history dedup - DONE, see `docs/operations/scheduler.md` + `docs/architecture/m5-notes.md`.
M5 phase 3 scope: bounded cache (memory + filesystem, TTL) + storage (local real, s3 prototype) - DONE, see `docs/operations/cache.md` + `docs/operations/storage.md`.
M6.1 stub scope: embedded capability reporting + single-entry domain + shared contract tests - DONE (native php-embed linking deferred), see docs/operations/embedded.md.

## Subsystems (SPEC 3.1, ADR-0002/0003/0012)

- `phpx-cli` (M5): `serve [--host --port]`, `doctor` (+SAPI/ext audit), `version`, `runtime list|inspect` (M4 fields + M6.1 embedded capabilities), `workers status|reload` (min/max/bounds/policy + drain), `queue status|push|work` (M5 phase 1 real), `scheduler run [--dry-run] [--catchup-window-secs N]` (M5 phase 2 real single-pass), `metrics` (logs + /healthz + /readyz + /metrics).
- `phpx-core`: errors, app identity, structured logging (tracing).
- `phpx-config`: TOML schema v1, validation, `phpx.toml.example` (incl. scheduler keys + history_file).
- `phpx-runtime`: versioned envelope + 4-byte BE framing, 16 MiB cap, PROTOCOL_VERSION=1.
- `phpx-runtime-process`: PHP discovery + `execute_one_shot` (M1 default).
- `phpx-worker`: `BoundedPool` hardened (M4: min/max, drain, restart budget + backoff/jitter + circuit breaker, recycled/snapshot/Prometheus).
- `phpx-server` (M4): Hyper gateway + /healthz + /readyz + /metrics, SAPI-compat dispatch, Ctrl-C/SIGTERM graceful drain, access logs with x-request-id.
- `phpx-queue` (M5 phase 1 real): `QueueDriver` trait, memory + sqlite, bounded retries/backoff/lease/dead-letter.
- `phpx-cache` (M5 phase 3 real): `CacheDriver` trait, memory + filesystem, TTL, bounded eviction.
- `phpx-storage` (M5 phase 3 real): `StorageDriver` trait, local FS real, s3 prototype (config real, transport deferred).
- `phpx-scheduler` (M5 phase 2 real): cron + timezone + history dedup, dispatch-only, never executes PHP.
- Future: `phpx-observability`. See SPEC 14 mapping note. (Embedded stub lives in phpx-runtime-embedded since M6.1.)

## Contracts

- Config schema v1: `schema_version = 1`, bounded workers (1-1024), no unbounded queues.
- Lifecycle: `Starting -> Ready -> Busy -> Ready`, `Ready/Busy -> Draining -> Stopped`, `Busy -> Unhealthy -> Restarting -> Ready`, `Starting -> Failed` terminal incarnation (SPEC 5.1 + ADR-0014 Proposed).
- Tokio: async orchestration only, blocking PHP off executor threads via `spawn_blocking` (ADR-0006). Scheduler never runs PHP on gateway threads (SPEC 7.2: dispatch-only, single pass + systemd timer).
- Backpressure mandatory (SPEC 5.3, ADR-0014 2.2): overload maps to HTTP 503, never unbounded queue. Queue depth max 10,000; scheduler max 100 schedules, 1 catchup job per schedule per tick.
- Static safety (SPEC 6.3): canonicalize + `starts_with` docroot check, reject traversal, never serve `.php` raw.
- TLS (SPEC 6.1, ADR-0007): Rustls; `tls=true` requires cert/key files, else clear error; plain HTTP for tests; HTTP/2 deferred.

## Decisions

Accepted: ADR-0001..0013. Proposed: ADR-0014 (worker lifecycle + memory guardian).
See `docs/ARCHITECTURE_DECISIONS.md` and `docs/adr/README.md`.
