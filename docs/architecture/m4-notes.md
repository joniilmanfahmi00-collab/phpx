# Catatan M4 PHPX - hardening worker + kompatibilitas aplikasi

Tujuan: membuat Process Runtime tangguh untuk aplikasi nyata. Status: SELESAI (pratinjau developer, bukan SLA produksi).

## Yang berubah vs M3

- Config (`phpx-config`): ditambah `min_workers`, `queue_bound` (otomatis `workers*4`/min 16 atau eksplisit),
  `queue_wait_timeout_seconds`, `shutdown_drain_timeout_seconds`, `restart_max_retries`,
  `restart_backoff_ms/max_ms`. Validasi: `min<=max`, batas per kunci. Config lama tetap terurai (serde defaults).
- Pool (`phpx-worker`): `min_workers`, `drain_timeout`, `max_requests_per_slot` (`recycled` logis),
  pelacakan failure beruntun + backoff eksponensial dengan jitter + circuit breaker crash-loop
  (fast-503 saat terbuka), `begin_drain()`/`drain(timeout)`, `snapshot()` (active/waiting/failures),
  `prometheus_text()`. Isolasi one-shot dipertahankan; recycling bersifat logis untuk worker persistent di masa depan.
- Gateway (`phpx-server`): `build_state` memasang semua field M4; `GET /metrics` (teks Prometheus +
  `phpx_build_info`); `is_overload` juga mencocokkan `draining`/`circuit` (503 + `Retry-After: 1`);
  shutdown menunggu Ctrl-C DAN SIGTERM (systemd) lalu `begin_drain()` + `drain(drain_timeout)`.
  Siklus hidup: Failed = upaya terminal + diagnostik; Stopped = sudah di-drain, tanpa kerja baru.
- Kompatibilitas SAPI (`phpx-runtime-process`): `php -d auto_prepend_file=<embedded bootstrap>` per request.
  Bridge env: `PHPX_HEADERS_JSON` + `PHPX_*` yang sudah ada; bootstrap mengisi `$_GET` (query),
  `$_COOKIE`, `$_SERVER` (`REQUEST_METHOD/REQUEST_URI/QUERY_STRING/HTTP_*`), polyfill `getallheaders()`,
  `phpx_input()` (membaca `php://input`, fallback ke `php://stdin`).
- CLI: `workers status` menampilkan min/max/bounds/timeout/policy + petunjuk `/metrics` + catatan siklus hidup;
  `runtime inspect` menampilkan field M4; `metrics` mendokumentasikan kunci `/metrics` + contoh curl;
  `doctor` menambah audit `php-sapi` (hanya `cli` yang didukung) + cek kombinasi 11 ekstensi.
- Fixture (tanpa jaringan): `composer-autoload` (stub autoloader + `$_GET`/`$_COOKIE`/body),
  sim `laravel-minimal` (routing, `/api/hello` JSON, header middleware), sim `wordpress-minimal`
  (`?p=`/`?s=`, shortcode), sim `woocommerce-minimal` (cart/checkout setelah baseline WP).
- Pengujian: `tests/m4_compat.rs` (5 pengujian: isolasi/crash, composer+SAPI, laravel, wp+woo, `/metrics`).
  Suite M1-M3 tetap hijau. `m3_cli` diperbarui untuk banner doctor M4.
- Dokumen: `operations/compatibility-matrix.md` (fixture x PHP/ext, yang tidak didukung eksplisit),
  `sapi-audit.md` (hanya cli, batas `php://input`, runbook audit), `soak.md` (laporan RSS/restart/latensi,
  adaptif dinonaktifkan), `configuration.md` (kunci M4). Skrip: `scripts/soak.sh`.

## Bukti keluar

- `cargo test -p phpx-config -q`, `-p phpx-worker -q`, `-p phpx-runtime-process -q`,
  `-p phpx-server -q`, `-p phpx-cli -q` semuanya hijau (termasuk 5 pengujian kompat M4 baru).
- `phpx doctor` melaporkan `php-sapi cli` + 11 ekstensi; `phpx workers status` menampilkan
  min/max/bounds/policy; `GET /metrics` mengekspos 10 kunci pool.
- `isolate.php` xN tetap `clean` dengan RID berbeda; `crash.php` (500) diikuti `/` (200).
- Matriks kompat dipublikasikan; SAPI/ext yang tidak didukung eksplisit; runbook soak siap.

## Ditunda (eksplisit, tidak boleh memblokir M4)

- Guardian memori RSS lunak/keras (sampling + semantik overshoot): inkremental per ADR-0014 Fase B.
- Min/max scaling adaptif: tetap NONAKTIF (hanya baseline pool tetap).
- Queue/scheduler/cache/storage: M5. Runtime embedded: M6. HTTP/2: gated.
- `$_FILES`/multipart, persistensi `$_SESSION`, semantik FPM: tidak diemulasikan (lihat sapi-audit).

Ketertelusuran: SPEC 15 butir 5-7 + 11-12 selesai di M4 (lingkup worker); penerimaan penuh v0.1 ada di SPEC 15.
