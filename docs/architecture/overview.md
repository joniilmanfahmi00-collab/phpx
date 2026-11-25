# Gambaran Arsitektur PHPX (M1-M5 selesai + P0-P2 + M6 serve path, developer preview)

Lingkup M0: workspace Rust yang dapat dibangun, config v1, kerangka `phpx doctor` / `phpx serve` — DONE.
Lingkup M1: prototipe Process Runtime di atas PHP one-shot + BoundedPool — DONE, lihat `docs/architecture/m1-notes.md`.
Lingkup M2: gateway HTTP/1.1 Tokio + Hyper, routing front-controller, serving statik aman, dispatch PHP via pool, backpressure 503, drain graceful, TLS via Rustls - DONE, lihat m2-notes.md. HTTP/2 ditunda, tidak diklaim di M2.
Lingkup M3: Developer preview v0.1 - serve/doctor final, status runtime/workers, default TOML terdokumentasi, Docker non-root, unit systemd, matriks PHP, smoke + bench baseline, /healthz + /readyz - DONE, lihat m3-notes.md. v0.1 adalah developer preview, bukan SLA produksi.
Lingkup M4: Hardening worker + kompat app - pool min/max, kebijakan queue/drain/restart, sirkuit crash-loop, /metrics, bootstrap SAPI-compat ($_GET/$_COOKIE/$_SERVER), fixture Composer/Laravel/WP/Woo, matriks kompat, runbook soak - DONE, lihat m4-notes.md.
Lingkup M5 fase 1: queue tahan lama (memory + sqlite, retry/backoff/dead-letter, recovery lease, `queue status|push|work` nyata) - DONE, lihat `docs/operations/queue.md`.
Lingkup M5 fase 2: scheduler cron sadar-timezone, single-pass dispatch-only, dedup history - DONE, lihat `docs/operations/scheduler.md` + `docs/architecture/m5-notes.md`.
Lingkup M5 fase 3: cache terbatas (memory + filesystem, TTL) + storage (local nyata, s3 prototipe) - DONE, lihat `docs/operations/cache.md` + `docs/operations/storage.md`.
Lingkup P0 (remediasi engineering): mode eksekusi eksplisit (P0-A), pool persistent terawasi (P0-B), guardian RSS + batas memori (P0-C), app asli Laravel/WP/Woo (P0-D) - DONE.
Lingkup P1: matriks resiliensi 9 test + ambang penerimaan (P1-A), bench lab + repeat + multipart + Redis (P1-B) - DONE.
Lingkup P2: pool adaptif opt-in + serve path embedded terkabel (EmbeddedPool single-flight, `runtime.mode=embedded`, 9/9 gerbang linked) - DONE, lihat `docs/operations/embedded.md`.
Lingkup M6: runtime embedded — dahulunya stub kapasitas (M6.1), kini live pada build linked (lihat `docs/operations/embedded.md`); link nativ CI tetap ditunda.

## Subsistem (SPEC 3.1, ADR-0002/0003/0012)

- `phpx-cli`: `serve [--host --port]`, `doctor` (+audit SAPI/ext), `version`, `runtime list|inspect` (field process + kemampuan embedded), `workers status|reload` (min/max/bound/kebijakan + drain), `queue status|push|work` (nyata), `scheduler run [--dry-run] [--catchup-window-secs N]` (nyata single-pass), `metrics` (log + /healthz + /readyz + /metrics), `cache`, `storage`.
- `phpx-core`: error, identitas app, logging terstruktur (tracing).
- `phpx-config`: skema TOML v1, validasi, `phpx.toml.example`.
- `phpx-runtime`: envelope berversi + framing BE 4-byte, cap 16 MiB, PROTOCOL_VERSION=1.
- `phpx-runtime-process`: discovery PHP + `execute_one_shot`.
- `phpx-worker`: `BoundedPool` + `PersistentPool` terawasi (min/max, drain, budget restart + backoff/jitter + circuit breaker, recycled/snapshot/Prometheus) + `EmbeddedPool` single-flight (P2) + guardian memori + autoscale adaptif opt-in.
- `phpx-server` (M4): gateway Hyper + /healthz + /readyz + /metrics, dispatch SAPI-compat, drain graceful Ctrl-C/SIGTERM, access log dengan x-request-id; memilih pool process vs embedded dari `runtime.mode`.
- `phpx-queue` (nyata): trait `QueueDriver`, memory + sqlite + redis, retry/backoff/lease/dead-letter terbatas.
- `phpx-cache` (nyata): trait `CacheDriver`, memory + filesystem + redis, TTL, eviksi terbatas.
- `phpx-storage` (nyata): trait `StorageDriver`, FS lokal nyata, s3 prototipe (config nyata, transport ditunda).
- `phpx-scheduler` (nyata): cron + timezone + dedup history, dispatch-only, tidak pernah mengeksekusi PHP.
- `phpx-runtime-embedded` (P2 live): engine libphp in-process pada satu thread khusus, `live::serve_with_timeout`, asesor gerbang `gates::assess()`.
- Masa depan: `phpx-observability`. Lihat catatan pemetaan SPEC 14.

## Kontrak

- Skema config v1: `schema_version = 1`, worker terbatas (1-1024), tanpa antrean tak terbatas.
- Siklus hidup: `Starting -> Ready -> Busy -> Ready`, `Ready/Busy -> Draining -> Stopped`, `Busy -> Unhealthy -> Restarting -> Ready`, `Starting -> Failed` inkarnasi terminal (SPEC 5.1 + ADR-0014).
- Tokio: hanya orkestrasi async, PHP blocking di luar thread eksekutor via `spawn_blocking` (ADR-0006). Scheduler tidak pernah menjalankan PHP di thread gateway (SPEC 7.2: dispatch-only, single pass + systemd timer). Engine embedded berjalan pada satu thread OS khusus, bukan thread gateway.
- Backpressure wajib (SPEC 5.3, ADR-0014 2.2): overload dipetakan ke HTTP 503, bukan antrean tak terbatas. Kedalaman antre maks 10.000; scheduler maks 100 jadwal, 1 job catchup per jadwal per tick.
- Keamanan statik (SPEC 6.3): kanonikalisasi + cek docroot `starts_with`, tolak traversal, jangan pernah sajikan `.php` mentah.
- TLS (SPEC 6.1, ADR-0007): Rustls; `tls=true` butuh file cert/key, bila tidak error jelas; HTTP polos untuk test; HTTP/2 ditunda.

## Keputusan

Accepted: ADR-0001..0013. Proposed: ADR-0014 (siklus hidup worker + guardian memori).
Lihat `docs/ARCHITECTURE_DECISIONS.md` dan `docs/adr/README.md`.
