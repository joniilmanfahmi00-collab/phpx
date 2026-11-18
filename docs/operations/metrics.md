# PHPX metrics + observability (M7)

## Endpoints (no auth in v0.1, bind 127.0.0.1 or private network)

- `GET /healthz` -> 200 `ok` (liveness, no PHP).
- `GET /readyz` -> 200 `ready` when docroot + entrypoint exist, else 500 (readiness).
- `GET /metrics` -> Prometheus text (`text/plain; version=0.0.4`).

## Series (from `BoundedPool::prometheus_text()` + `phpx_build_info`)

- `phpx_requests_completed_total`, `phpx_requests_failed_total`, `phpx_timeouts_total`
- `phpx_rejected_overload_total`, `phpx_recycled_total`, `phpx_restarts_total`, `phpx_circuit_breaks_total`
- `phpx_pool_active`, `phpx_pool_waiting` (gauges)
- `phpx_build_info{version="x.y.z"} 1`

## CLI

```bash
phpx metrics                 # human snapshot (pool + endpoints hint)
phpx metrics --json          # machine JSON where supported
curl -fsS http://127.0.0.1:8080/metrics
```

## Logs (JSON when `RUST_LOG=json` or `phpx serve --json`)

- Fields: `request_id`, `method`, `path`, `status`, `duration_ms`, `x-request-id` header echo.
- Secrets never logged (see security.md). `traceparent` incoming is accepted and forwarded to PHP env for OTel correlation; no OTel exporter in v0.1 (gateway is OTel-ready, not OTel-enabled).
- Prometheus scrape example (15s) + Grafana alert hints: alert on `rate(phpx_rejected_overload_total[5m]) > 0` (saturated) and `phpx_circuit_breaks_total` increases (crash loop).

## OTel note (M7 explicit)

v0.1 has no OTel exporter dependency (keeps release small + auditable). OTel-ready means: `request_id` = trace-local ID, W3C `traceparent` passthrough, JSON logs correlatable. Full OTLP exporter is deferred post-RC; use gateway logs + `/metrics` for now.
