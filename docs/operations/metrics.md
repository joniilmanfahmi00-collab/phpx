# Metrik + observabilitas PHPX (M7)

## Endpoint (tanpa auth di v0.1, bind 127.0.0.1 atau jaringan privat)

- `GET /healthz` -> 200 `ok` (liveness, tanpa PHP).
- `GET /readyz` -> 200 `ready` bila docroot + entrypoint ada, jika tidak 500 (readiness).
- `GET /metrics` -> teks Prometheus (`text/plain; version=0.0.4`).

## Deret (dari `prometheus_text()` pool + `phpx_build_info`)

- `phpx_execution_mode_info{execution_mode,protocol_version} 1` (P0-A: membedakan one-shot vs persistent)
- `phpx_pool_completed_total`, `phpx_pool_failed_total`, `phpx_pool_timeouts_total`
- `phpx_pool_rejected_overload_total`, `phpx_pool_recycled_total`, `phpx_pool_restarts_total`, `phpx_pool_circuit_breaks_total`
- `phpx_pool_active`, `phpx_pool_waiting`, `phpx_pool_max_workers`, `phpx_pool_min_workers` (gauge)
- `phpx_worker_state{worker_id}`, `phpx_worker_requests_total{worker_id}`, `phpx_worker_uptime_seconds{worker_id}` (P0-B)
- `phpx_worker_rss_bytes{worker_id}`, `phpx_worker_peak_rss_bytes{worker_id}`, `phpx_worker_healthy{worker_id}` (guardian P0-C; rss `0` = belum pernah disampel pada incarnation ini)
- `phpx_worker_startup_duration_seconds{worker_id}`, `phpx_worker_shutdown_duration_seconds{worker_id}` (P0-C)
- `phpx_worker_restarts_by_reason_total{worker_id,reason}` (P0-C: crash/timeout/protocol/recycle-budget/soft-limit/hard-limit/demand-retry)
- `phpx_worker_memory_limit_hits_total{worker_id,threshold="soft|hard"}` (P0-C; `sum()` antar worker untuk total pool)
- `phpx_worker_last_recovery_duration_seconds{worker_id}` (P1-A: deteksi-ke-ready dari restart tak-terencana terakhir, 0 = belum ada; kaki di `WorkerSnapshot::last_recovery`)
- `phpx_build_info{version="x.y.z"} 1`

## CLI

```bash
phpx metrics                 # snapshot manusia (pool + petunjuk endpoint)
phpx metrics --json          # JSON mesin bila didukung
curl -fsS http://127.0.0.1:8080/metrics
```

## Log (JSON bila `RUST_LOG=json` atau `phpx serve --json`)

- Field: `request_id`, `method`, `path`, `status`, `duration_ms`, gema header `x-request-id`.
- Secret tidak pernah di-log (lihat security.md). `traceparent` masuk diteruskan ke env PHP untuk korelasi OTel; tanpa exporter OTel di v0.1 (gateway OTel-ready, bukan OTel-enabled).
- Contoh scrape Prometheus (15s) + petunjuk siaga Grafana: siaga bila `rate(phpx_pool_rejected_overload_total[5m]) > 0` (jenuh) dan bila `phpx_pool_circuit_breaks_total` naik (crash loop).

## Catatan OTel (M7 eksplisit)

v0.1 tanpa dep exporter OTel (agar rilis kecil + teraudit). OTel-ready artinya: `request_id` = ID jejak-lokal, passthrough W3C `traceparent`, log JSON terkorelasi. Exporter OTLP penuh ditunda pasca-RC; pakai log gateway + `/metrics` untuk saat ini.
