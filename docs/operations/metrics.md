# PHPX metrics + observability (M7)

## Endpoints (no auth in v0.1, bind 127.0.0.1 or private network)

- `GET /healthz` -> 200 `ok` (liveness, no PHP).
- `GET /readyz` -> 200 `ready` when docroot + entrypoint exist, else 500 (readiness).
- `GET /metrics` -> Prometheus text (`text/plain; version=0.0.4`).

## Series (from pool `prometheus_text()` + `phpx_build_info`)

- `phpx_execution_mode_info{execution_mode,protocol_version} 1` (P0-A: distinguishes one-shot vs persistent)
- `phpx_pool_completed_total`, `phpx_pool_failed_total`, `phpx_pool_timeouts_total`
- `phpx_pool_rejected_overload_total`, `phpx_pool_recycled_total`, `phpx_pool_restarts_total`, `phpx_pool_circuit_breaks_total`
- `phpx_pool_active`, `phpx_pool_waiting`, `phpx_pool_max_workers`, `phpx_pool_min_workers` (gauges)
- `phpx_worker_state{worker_id}`, `phpx_worker_requests_total{worker_id}`, `phpx_worker_uptime_seconds{worker_id}` (P0-B)
- `phpx_worker_rss_bytes{worker_id}`, `phpx_worker_peak_rss_bytes{worker_id}`, `phpx_worker_healthy{worker_id}` (P0-C guardian; `0` rss = never sampled on this incarnation)
- `phpx_worker_startup_duration_seconds{worker_id}`, `phpx_worker_shutdown_duration_seconds{worker_id}` (P0-C)
- `phpx_worker_restarts_by_reason_total{worker_id,reason}` (P0-C: crash/timeout/protocol/recycle-budget/soft-limit/hard-limit/demand-retry)
- `phpx_worker_memory_limit_hits_total{worker_id,threshold="soft|hard"}` (P0-C; `sum()` across workers for pool totals)
- `phpx_worker_last_recovery_duration_seconds{worker_id}` (P1-A: detection-to-ready of the last unplanned restart, 0 = none yet; legs in `WorkerSnapshot::last_recovery`)
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
- Prometheus scrape example (15s) + Grafana alert hints: alert on `rate(phpx_pool_rejected_overload_total[5m]) > 0` (saturated) and `phpx_pool_circuit_breaks_total` increases (crash loop).

## OTel note (M7 explicit)

v0.1 has no OTel exporter dependency (keeps release small + auditable). OTel-ready means: `request_id` = trace-local ID, W3C `traceparent` passthrough, JSON logs correlatable. Full OTLP exporter is deferred post-RC; use gateway logs + `/metrics` for now.
