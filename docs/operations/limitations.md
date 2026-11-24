# PHPX operational limitations (v0.1 developer preview)

**v0.1 is a developer preview, not a production SLA.** Do not use for
production traffic. The list below is current as of the P1-B state: what the
code actually does, what it explicitly does not do, and where the operator is
responsible.

## What v0.1 does

- Serves PHP via Process Runtime in two explicit modes (`runtime.process.execution_mode`):
  `one-shot` (fresh `php` per request) and `persistent` (fixed-size supervised
  workers with handshake, per-worker state, restart budgets). Reported by
  CLI/JSON and `phpx_execution_mode_info`.
- Safe static serving (traversal blocked, symlink escape -> 403, `.php` never served raw).
- Backpressure: overload / crash-loop circuit / draining -> `503 + Retry-After: 1`,
  never an unbounded queue. Queue bounds + wait timeouts enforced and metered.
- Crash isolation: PHP exit != 0 -> `500`, timeout -> `504`, gateway stays up;
  supervised workers restart with a new incarnation (reason recorded).
- Resource guardian (persistent mode): OS-RSS soft limit -> graceful recycle,
  hard limit -> protective termination; per-worker RSS/peak/uptime/startup/
  shutdown/health/recovery metrics. Sampling-based: overshoot between samples
  is possible; RSS != PHP heap.
- PHP heap limit: `memory_limit` is passed as `-d memory_limit=...`
  (Zend heap only, both modes) — not an OS RSS cap.
- Graceful drain on Ctrl-C / SIGTERM / `pool.drain()`, bounded by the drain timeout.
- Health: `GET /healthz` (liveness), `GET /readyz` (readiness); Prometheus `/metrics`.
- Structured access logs with `request_id` + `x-request-id` header; sensitive
  headers redacted (`[redacted]`).
- TLS via Rustls when `tls=true` + PEM cert/key; plain HTTP for local/tests.
- HTTP/1.1 only. HTTP/2 deferred.
- Queue + scheduler + cache + storage are real (`queue.md`, `scheduler.md`,
  `cache.md`, `storage.md`). PHPX Redis driver deferred; app-native Redis via
  `REDIS_HOST` works directly from PHP without a PHPX driver.
- Benchmark laboratory (`scripts/bench-lab.sh/.ps1`): throughput, latency
  distribution, cold vs warm, gateway RSS trend — developer-machine numbers,
  never an SLA.
- Real-application smoke: Laravel 12 + WordPress 7.1 boot and serve through
  PHPX (see `compatibility-matrix.md`). Simulations remain for the fixture set.

## What v0.1 does NOT do

- No live reload. `phpx workers reload` validates config; apply by restarting
  the gateway (`systemctl restart phpx`). SIGHUP zero-downtime reload is not supported.
- PHP-set response status/headers are NOT propagated under CLI SAPI:
  `header()` / `http_response_code()` are no-ops, so framework 404 pages arrive
  as HTTP 200 with the rendered body, and `?p=N`-style canonical redirects
  (WordPress pretty permalinks on) arrive as empty 200s. Crash -> 500 and
  timeout -> 504 are gateway-mapped and unaffected. See `sapi-audit.md`.
- Sessions (`$_SESSION` persistence) and raw `php://input` bodies under CLI
  are not emulated. Multipart/form-data populates `$_POST`/`$_FILES`
  (fixture `upload.php` pins the contract); `move_uploaded_file()` stays
  false under CLI — read `tmp_name` or `rename()` it. No session/file-write
  integration tests on real apps yet.
- Persistent workers reuse the PHP process: `$GLOBALS`/statics set by app code
  persist across requests (pinned by test). Use one-shot when strict isolation
  is required. Function redefinition on re-include fatals the worker (restarted,
  counted as crash).
- Embedded runtime stays an experimental stub (`available=false`, process default).
- No multi-tenancy isolation claim. PHP extensions + app code are trusted
  executable code (SPEC 10).
- No WAF, CDN, or full reverse-proxy parity (SPEC 6.2 deferred).
- S3 storage transport is a prototype; Redis queue/cache drivers deferred.
- cgroup CPU/memory auto-detection: none. The gateway stays bounded and
  observable; enforcement is operator + orchestrator responsibility
  (Docker `--cpus/--memory`, systemd `MemoryMax=`/`CPUQuota=`).
- OTel: gateway is OTel-ready (`request_id` + W3C `traceparent` passthrough +
  JSON logs), no OTLP exporter (keeps release auditable).
- No encrypted-at-rest (SQLite file perms + disk encryption are operator
  responsibility); upgrades are stateless restart + snapshot restore
  (`upgrade-rollback.md`, `backup-recovery.md`).

## Security boundaries

- Run as non-root where feasible (Docker `USER phpx`, systemd `User=phpx`, `NoNewPrivileges=true`).
- Bound request bodies (`request_body_limit`), bounded workers (`1-1024`), bounded queue.
- Never expose secrets in logs/diagnostics. Sensitive headers are redacted in logs
  (values not printed); full header values are only passed to PHP env, not log lines.
- Validate static paths against traversal + symlink escape. No `..`, `\`, NUL,
  `%2e/%2f/%5c/%00` pre-FS (400), canonical + `starts_with` docroot check, escape -> 403.

## When to adopt

- Local dev, CI smoke, Docker evaluation, vanilla PHP prototyping: yes.
- Production, PII, payments, multi-tenant hosting: no — Beta/RC gates
  (see `release-gates.md`) are not met yet.
