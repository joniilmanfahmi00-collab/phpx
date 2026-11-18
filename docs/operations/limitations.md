# PHPX operational limitations (M3+M5 v0.1 developer preview)

**v0.1 is a developer preview, not a production SLA.** Do not use for production traffic without M4 hardening + your own review.

## What M3 does

- Serves vanilla PHP via Process Runtime (one-shot PHP per request, bounded pool).
- Safe static serving (traversal blocked, symlink escape -> 403, `.php` never served raw).
- Backpressure: overload -> `503 + Retry-After: 1`, never unbounded queue.
- Crash isolation: PHP exit != 0 -> `500`, gateway stays up.
- Graceful drain on Ctrl-C / SIGINT (`systemd stop`), bounded by `request_timeout_seconds`.
- Health: `GET /healthz` (liveness), `GET /readyz` (readiness).
- Structured access logs with `request_id` + `x-request-id` header.
- TLS via Rustls when `tls=true` + PEM cert/key; plain HTTP for local/tests.
- HTTP/1.1 only. HTTP/2 deferred (gated by integration tests, not claimed).

## What M3 does NOT do

- Prometheus `/metrics` landed in M4. See `phpx metrics`.
- Live pool counters landed in M4 (`/metrics`, `workers status` snapshot).
- No live reload without restart. `phpx workers reload` validates config; apply via `restart phpx serve` / `systemctl restart phpx`. SIGHUP zero-downtime reload is M4.
- Queue + scheduler + cache + storage are real since M5 (see `queue.md` + `scheduler.md` + `cache.md` + `storage.md`): `queue status|push|work` exit 0, `scheduler run` dispatch-only exit 0, `cache status|get|set|clear` exit 0, `storage status|put|get|list` exit 0 (s3 transport prototype deferred). PHPX Redis driver deferred; app-native Redis via WordPress/Laravel REDIS_HOST tetap jalan langsung dari PHP tanpa PHPX driver.
- Embedded runtime is M6 full stub (M6.1 plus M6.2 ABI pin plus link probe, narrow bridge, ownership rules, NTS matrix, compat subsets, containment): `runtime.mode=embedded` needs `[runtime.embedded] enabled=true`; capabilities report available=false, single-entry domain, process stays default. Native linking deferred (see embedded.md).
- No multi-tenancy isolation claim. PHP extensions + app code are trusted executable code in v0.1 (SPEC 10).
- No WAF, CDN, or full reverse-proxy parity (SPEC 6.2 deferred).
- Soak runbook landed in M4 (`soak.md` + `scripts/soak.sh`); memory guardian incremental per ADR-0014 Phase B.
- Framework fixtures (Composer/Laravel/WordPress/WooCommerce) landed in M4; M3 only claimed vanilla.

## Security boundaries (M3)

- Run as non-root where feasible (Docker `USER phpx`, systemd `User=phpx`, `NoNewPrivileges=true`).
- Bound request bodies (`request_body_limit`), bounded workers (`1-1024`), bounded queue.
- Never expose secrets in logs/diagnostics. Sensitive headers are redacted in logs (values not printed); full header values are only passed to PHP env, not log lines.
- Validate static paths against traversal + symlink escape. No `..`, `\`, NUL, `%2e/%2f/%5c/%00` pre-FS (400), canonical + `starts_with` docroot check, escape -> 403.
- OS/CPU/memory cgroup limits for production are your responsibility in M3 (see systemd.md).

## When to adopt

- Local dev, CI smoke, Docker evaluation, vanilla PHP prototyping: yes.
- Production, PII, payments, multi-tenant hosting: no, wait for M4+ and your security review.

## M7 RC delta (production candidate, still not a blanket SLA)

- Threat model published (`security.md`); secrets never logged (`redacted_header_value` + `m7_security` soak).
- Saturation bounded by construction (`workers` + `queue_bound` + `queue_wait` -> 503 + `Retry-After: 1`); proven by `m7_resilience` (8 slow sleepers on 2 workers stay 200/503/504, `/metrics` observable, fast endpoint recovers).
- cgroup/CPU/RAM enforcement is operator + orchestrator responsibility (Docker `--cpus/--memory`, systemd `MemoryMax/CPUQuota`); gateway does not auto-detect cgroup quotas in v0.1.
- OTel: gateway is OTel-ready (`request_id` + W3C `traceparent` passthrough + JSON logs), no OTLP exporter in v0.1 (keeps release auditable). Use `/metrics` + logs for now.
- Upgrades are stateless restart + SQLite snapshot restore (`upgrade-rollback.md`, `backup-recovery.md`). No encrypted-at-rest (file perms + disk encryption are operator responsibility).
- Docker runs non-root `phpx:10001` + `HEALTHCHECK` (verified by test); harden at runtime with `--read-only --cap-drop ALL` + `data/` volume.
- SBOM via `scripts/sbom.sh` (syft SPDX preferred, cargo-tree fallback). Release via `scripts/release.sh` + `packaging/README.md`.
