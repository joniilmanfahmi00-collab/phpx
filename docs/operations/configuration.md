# PHPX configuration - TOML schema v1, documented defaults (M4 hardening + M5 queue/scheduler/cache/storage)

Primary file: `phpx.toml`. Example: `phpx.toml.example`.
Resolution for `phpx serve` / `runtime` / `workers` / `queue` / `scheduler` commands:
`--config <path>` > `./phpx.toml` > `phpx.toml.example`.

Exit codes: `0` ok, `1` runtime failure, `2` config/usage.
All config errors are actionable and include the file path.

## Minimal developer config

```toml
schema_version = 1

[server]
host = "127.0.0.1"
port = 8080
tls = false
request_body_limit = "16MB"

[application]
root = "."
document_root = "public"
entrypoint = "index.php"

[runtime]
mode = "process"
php_version = "8.4"

[runtime.process]
workers = 4
request_timeout_seconds = 30
```

## Full defaults (schema v1)

| Key | Default | Notes |
|-----|---------|-------|
| `schema_version` | `1` (required) | Only `1` accepted in M3-M5 |
| `server.host` | `127.0.0.1` | Use `0.0.0.0` in Docker/systemd behind firewall |
| `server.port` | `8080` | Override: `phpx serve --port 9000` |
| `server.tls` | `false` | `true` requires `tls_cert_file` + `tls_key_file` (PEM), else exit 2 |
| `server.tls_cert_file` | none | Required when `tls=true` |
| `server.tls_key_file` | none | Required when `tls=true` |
| `server.request_body_limit` | `"16MB"` | `parse_size`: bytes or `K/KB/M/MB/G/GB/B`, e.g. `512KB`, `256M`. Oversize -> `413` |
| `application.root` | `"."` | Resolved against cwd; Docker default `/app/examples/vanilla-php` |
| `application.document_root` | `"public"` | Only files under here are served; traversal blocked |
| `application.entrypoint` | `"index.php"` | Must exist; front-controller fallback |
| `runtime.mode` | `"process"` | Only `process` active; `embedded` is experimental opt-in since M6.1 (stub) |
| `runtime.php_version` | `"8.4"` | Target for `doctor`/`runtime list`; actual from `php --version` |
| `runtime.process.workers` | `4` | Max pool size, bounded `1-1024`, never unbounded. Overload/circuit/drain -> `503 + Retry-After: 1` |
| `runtime.process.execution_mode` | `"one-shot"` | P0-A explicit, P0-B implemented. `"one-shot"` = fresh `php` per request; `"persistent"` = fixed-size supervised workers (handshake-gated, per-worker state/metrics). Reported by CLI/JSON + `phpx_execution_mode_info` |
| `runtime.process.min_workers` | `1` | Min pool size, `1-1024`, must be `<= workers`. Ready-capacity floor for future adaptive (M4: validated, pool honors max) |
| `runtime.process.max_requests` | `1000` | Logical recycle budget per slot (`>=1`); one-shot already isolates per request; counts toward `recycled` in `/metrics` |
| `runtime.process.request_timeout_seconds` | `30` | Bounds PHP execution + graceful drain |
| `runtime.process.startup_timeout_seconds` | `10` | Reserved for supervised workers (M4) |
| `runtime.process.memory_limit` | `"256M"` | P0-C: real PHP heap limit, passed as `-d memory_limit=...` to every PHP child (one-shot + persistent). Bounds the Zend heap ONLY, not OS RSS. `"0"`/empty = php default |
| `runtime.process.memory_soft_limit` | `"0"` | P0-C guardian (persistent only): OS-RSS soft limit, `"0"` = disabled. At/over -> graceful recycle (replacement first, staggered). Must be below hard when both set |
| `runtime.process.memory_hard_limit` | `"0"` | P0-C guardian (persistent only): OS-RSS hard limit, `"0"` = disabled. At/over -> protective termination + respawn with diagnostic reason |
| `runtime.process.memory_sample_interval_seconds` | `5` | `1-60`. Idle-worker RSS sampling period. Request completion is an additional sampling point |
| `runtime.process.queue_bound` | auto (`workers*4`, min 16) | Explicit `1-100000` overrides auto. Waiting beyond bound -> `503` (never unbounded) |
| `runtime.process.queue_wait_timeout_seconds` | `2` | `0-120`. Wait for a worker slot before `503` |
| `runtime.process.shutdown_drain_timeout_seconds` | `10` | `1-120`. SIGTERM/Ctrl-C drain budget; pool stops intake (`draining` -> `503`), waits for in-flight |
| `runtime.process.restart_max_retries` | `5` | `0-100`. Consecutive-failure budget before crash-loop circuit opens (ADR-0014 2.6) |
| `runtime.process.restart_backoff_ms` | `250` | `10-60000`. Base backoff for circuit window |
| `runtime.process.restart_backoff_max_ms` | `30000` | `>= base`, `<=300000`. Backoff cap (exponential + jitter) |
| `runtime.embedded.enabled` | `false` | Experimental opt-in since M6.1 (stub: available=false); process stays default |
| `queue.driver` | `"sqlite"` | `memory` (ephemeral dev) / `sqlite` (durable) / `redis` (shared server). `queue status` real exit 0 (M5 phase 1 + Redis) |
| `queue.database` | `"./data/queue.db"` | Required when `driver=sqlite` |
| `queue.url` | none | Required when `driver=redis`, e.g. `redis://127.0.0.1:6379` (or `PHPX_REDIS_URL` env) |
| `queue.workers` | `2` | `1-16` (bounded) |
| `scheduler.enabled` | `false` | `false` = `scheduler run` exit 0 + note. `true` + schedules = real dispatch (M5 phase 2) |
| `scheduler.history_file` | none (memory) | e.g. `"./data/scheduler.db"` for durable dedup; empty = ephemeral |
| `scheduler.poll_interval_secs` | `30` | `1-3600`. Informational for timer operators (CLI is single-pass) |
| `scheduler.schedules` | `[]` | Max 100, ids unique. See `scheduler.md` for `[[scheduler.schedules]]` keys |
| `cache.driver` | `"memory"` | `memory` (ephemeral) / `filesystem` (durable) / `redis` (shared server). `cache status|get|set|clear` real exit 0 (M5 phase 3 + Redis) |
| `cache.max_memory` | `"128MB"` | `1KB-1GB` via `parse_size`. Bound for memory+filesystem |
| `cache.dir` | none | Required when `driver=filesystem`, e.g. `./data/cache` |
| `cache.url` | none | Required when `driver=redis`, e.g. `redis://127.0.0.1:6379` (or `PHPX_REDIS_URL` env) |
| `cache.max_entries` | `1000` | `1-100000` (bounded) |
| `storage.driver` | `"local"` | `local` (real) / `s3` (prototype: config real, transport deferred) |
| `storage.root` | none | Required when `driver=local`, e.g. `./data/storage` |
| `storage.endpoint` | none | Required when `driver=s3`, must start `http://`/`https://` |
| `storage.bucket` | none | Required when `driver=s3` |
| `storage.region` | `"auto"` | S3 region, default `auto` for prototype |
| `observability.json_logs` | `true` | `phpx serve --json` enables JSON logs |
| `observability.metrics` | `true` | M4 = access logs + `/healthz` + `/readyz` + `/metrics` (Prometheus text) |

### `[[scheduler.schedules]]` keys (M5 phase 2)

| Key | Required | Notes |
|-----|----------|-------|
| `id` | yes | Non-empty, unique across schedules |
| `cron` | yes | 5-field `minute hour dom month dow`, names + steps + lists. See `scheduler.md` |
| `timezone` | no (default UTC) | `UTC` / `local` / offset `+07:00` / IANA `Asia/Jakarta` |
| `payload` | yes | Non-empty JSON string, max 256KB, enqueued as queue job |
| `enabled` | no (default true) | `false` = counted as `skipped_disabled`, never dispatched |
| `missed_policy` | no (default `skip`) | `skip` / `dispatch-now` (`catchup` alias). Explicit per SPEC 7.2 |
| `max_retries` | no (default 3) | `0-25`, passed to queue enqueue |

## Validation rules (M4-M5)

- `schema_version != 1` -> error, exit 2.
- `port == 0` -> error.
- `tls=true` without cert+key -> error naming both files.
- `request_body_limit` must parse via `parse_size`, else error with expected format.
- `application.root` / `entrypoint` must be non-empty; docroot must exist and entry file must exist at startup.
- `runtime.mode` must be `process` or `embedded`; `embedded` without `[runtime.embedded] enabled=true` is rejected.
- `workers` must be `1-1024`, `min_workers <= workers`, bounds per key (see table).
- `runtime.process.execution_mode` must be `one-shot` or `persistent` (P0-B: both runnable; persistent = fixed-size supervised pool).
- Enforcement points (P0-C): `memory_limit` = PHP heap (`-d`, Zend only, both modes);
  `memory_soft_limit`/`memory_hard_limit` = OS RSS supervisor policy (persistent
  workers only); soft must be < hard when both set; `memory_sample_interval_seconds` 1-60.
  RSS != PHP heap: allocator behavior, native extensions, shared libraries and
  fragmentation affect observed RSS; sampling overshoot between sample points is
  possible and does not imply a leak (or prove its absence).
- `queue.driver` must be `memory`, `sqlite` or `redis`; `workers` 1-16; sqlite requires `database`; redis requires `url` (or `PHPX_REDIS_URL`).
- `scheduler.poll_interval_secs` 1-3600; max 100 schedules; ids unique.
  Each schedule validated via `ScheduleDef::new` (cron, timezone, payload,
  policy, retries) — config and CLI can never disagree. Bad schedule = exit 2
  with flattened `invalid schedule: ...` message.
- `cache.driver` must be `memory`, `filesystem` or `redis`; filesystem requires `dir`; redis requires `url` (or `PHPX_REDIS_URL`); `max_memory` 1KB-1GB; `max_entries` 1-100000.
- `storage.driver` must be `local` or `s3`; local requires `root`; s3 requires `endpoint` + `bucket` (prototype, transport deferred).

## CLI overrides (M3 finalized, M5 queue/scheduler/cache/storage real)

- `phpx serve --host 0.0.0.0 --port 9000 --config /etc/phpx/phpx.toml`
- `phpx serve --json` prints startup JSON including `health: {liveness: /healthz, readiness: /readyz}`.
- `phpx doctor [--json]`, `phpx runtime list|inspect [--json]`, `phpx workers status|reload [--json]`, `phpx metrics [--json]`.
- `phpx queue status|push|work` real (M5 phase 1, exit 0; `push` refused on memory driver with exit 2).
- `phpx cache status|get|set|delete|clear` real (M5 phase 3, exit 0; bad key/value exit 2).
- `phpx storage status|put|get|delete|list` real (M5 phase 3, exit 0; s3 data ops deferred note, bad path exit 2).
- `phpx scheduler run [--dry-run] [--catchup-window-secs N]` real single-pass dispatch (M5 phase 2, exit 0 + note when disabled; exit 2 on bad cron/TZ or memory driver without `--dry-run`).

See also: `queue.md`, `scheduler.md`, `cache.md`, `storage.md`, `quickstart-linux.md`, `quickstart-docker.md`, `php-version-matrix.md`, `limitations.md`, `systemd.md`.
