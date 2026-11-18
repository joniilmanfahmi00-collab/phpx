# PHPX security review + threat model (M7 Pase 1)

**Status:** RC gate, v0.1 developer preview. App code + PHP extensions are trusted (SPEC 10). No multi-tenant isolation claim.

## Trust boundaries

| # | Boundary | Trust | Mitigation (verified) |
|---|----------|-------|-----------------------|
| 1 | Network -> gateway | Untrusted | Bounded body (`request_body_limit`), bounded pool + queue (503 + `Retry-After: 1`), timeouts, TLS via Rustls opsional |
| 2 | URI path -> FS | Untrusted | Pre-FS reject (`..`, `\\`, NUL, `%2e/%2f/%5c/%00` -> 400) + canonical `starts_with` docroot check (escape -> 403). Never serve `.php` raw. |
| 3 | Headers/logs | Untrusted secrets | Access logs only `request_id/method/path/status/duration_ms`. `is_sensitive_header()` + `redacted_header_value()` guarantee: `authorization/cookie/*token*/*secret*/*password*/*api-key*/*auth*` -> `[redacted]`. Full values only go to PHP env, never logs. |
| 4 | PHP worker | Trusted app, untrusted input | One-shot `php <entry>` per request, no cross-request globals. Crash (`exit != 0`) -> 500, gateway stays up. Restart budget + circuit breaker. |
| 5 | Config/files | Operator trusted | `phpx serve --config` validated; TLS PEM paths, docroot/entrypoint existence checked. No secrets in `phpx doctor`/`metrics` output. |
| 6 | Container/host | Operator responsibility | Docker `USER phpx (10001)`, `HEALTHCHECK /healthz`, systemd `User=phpx + NoNewPrivileges + ProtectSystem=strict`. cgroup CPU/memory limits via systemd/Docker (see below). |

## Non-root verification (M7)

- Static: `m7_dockerfile_runs_non_root` asserts `USER phpx` + `HEALTHCHECK` + uid `10001`.
- Runtime: image runs as `phpx`; systemd example runs as `User=phpx`.
- Operator check: `docker run --rm phpx:0.1.0-dev whoami` must print `phpx`, not `root`.

## cgroup / container limits (operator)

M7 does not claim auto-detection of cgroup limits; it stays bounded and observable so your orchestrator can enforce them:

```bash
# Docker: 2 CPU + 1GiB, gateway 503s when saturated instead of OOM spiral
docker run --rm -p 8080:8080 --cpus=2 --memory=1g phpx:0.1.0-dev

# systemd: see packaging/systemd/phpx.service (MemoryMax=, CPUQuota= examples in upgrade-rollback.md)
```

Saturation signal: `/metrics` (`rejected_overload`, `circuit_breaks`, `waiting`) + access logs `status=503`.

## Secret handling rules (M7 gate)

1. Never log header values; only method/path/status/duration + IDs.
2. New code that must display a header must call `redacted_header_value(name, value)`.
3. `traceparent` (W3C) is passthrough for OTel (no PII); content is IDs, safe to log as ID.
4. Report template: impact, affected boundary above, repro (raw socket vs reqwest), fix + regression test name.

## Known non-goals (RC explicit)

- No WAF/CDN parity, no sandbox escape claim for malicious extensions, no encrypted queue at rest (SQLite file perms are operator responsibility, see backup-recovery.md).
