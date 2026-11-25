# Keterbatasan operasional PHPX (v0.1 developer preview)

**v0.1 adalah developer preview, bukan SLA produksi.** Jangan pakai untuk
traffic produksi. Daftar di bawah ini terkini per state P2 serve path: apa
yang benar-benar dilakukan kode, apa yang eksplisit tidak dilakukan, dan di
mana operator bertanggung jawab.

## Yang dilakukan v0.1

- Melayani PHP via Process Runtime dalam dua mode eksplisit (`runtime.process.execution_mode`):
  `one-shot` (`php` fresh per request) dan `persistent` (worker terawasi
  berukuran-tetap dengan handshake, state per-worker, budget restart; autoscale
  adaptif P2 adalah opt-in eksperimental via `adaptive_enabled`, nonaktif
  default). Dilaporkan oleh CLI/JSON dan `phpx_execution_mode_info`.
- Melayani statik yang aman (traversal diblokir, symlink escape -> 403, `.php` tidak pernah disajikan mentah).
- Backpressure: overload / sirkuit crash-loop / draining -> `503 + Retry-After: 1`,
  tanpa antrean tak terbatas. Bound antre + timeout tunggu ditegakkan dan diukur.
- Isolasi crash: PHP exit != 0 -> `500`, timeout -> `504`, gateway tetap menyala;
  worker terawasi restart dengan inkarnasi baru (alasan tercatat).
- Resource guardian (mode persistent): OS-RSS soft limit -> recycle graceful,
  hard limit -> terminasi protektif; metrik RSS/peak/uptime/startup/
  shutdown/health/recovery per-worker. Berbasis sampling: overshoot antar
  sampel mungkin terjadi; RSS != heap PHP.
- Batas heap PHP: `memory_limit` diteruskan sebagai `-d memory_limit=...`
  (hanya heap Zend, kedua mode) — bukan cap OS RSS.
- Drain graceful pada Ctrl-C / SIGTERM / `pool.drain()`, dibatasi timeout drain.
- Health: `GET /healthz` (liveness), `GET /readyz` (readiness); Prometheus `/metrics`.
- Access log terstruktur dengan `request_id` + header `x-request-id`; header
  sensitif disunting (`[redacted]`).
- TLS via Rustls bila `tls=true` + cert/key PEM; HTTP polos untuk lokal/test.
- Hanya HTTP/1.1. HTTP/2 ditunda.
- Queue + scheduler + cache + storage itu nyata (`queue.md`, `scheduler.md`,
  `cache.md`, `storage.md`), termasuk driver Redis queue/cache (butuh server;
  Redis app-native via `REDIS_HOST` bekerja langsung dari PHP tanpa driver PHPX).
- Laboratorium benchmark (`scripts/bench-lab.sh/.ps1`): throughput, distribusi
  latensi, cold vs warm, tren RSS gateway — angka mesin developer,
  bukan SLA.
- Smoke app asli: Laravel 12 + WordPress 7.1 boot dan melayani lewat
  PHPX (lihat `compatibility-matrix.md`). Simulasi tersisa untuk set fixture.
- Embedded single-flight (P2, opt-in): `runtime.mode=embedded` + `enabled=true`
  melayani lewat engine in-process dengan timeout eksplisit
  (lihat `embedded.md`); 9/9 gerbang lolos pada build linked, stub fail-closed.
  Default tetap `process`.

## Yang TIDAK dilakukan v0.1

- Tanpa live reload. `phpx workers reload` memvalidasi config; terapkan dengan
  me-restart gateway (`systemctl restart phpx`). Reload SIGHUP zero-downtime tidak didukung.
- Status/header respons set-PHP TIDAK dipropagasi di CLI SAPI:
  `header()` / `http_response_code()` adalah no-op, sehingga halaman 404
  framework tiba sebagai HTTP 200 dengan body renderan, dan redirect kanonis
  ala `?p=N` (pretty permalink WordPress menyala) tiba sebagai 200 kosong.
  Crash -> 500 dan timeout -> 504 dipetakan gateway dan tidak terpengaruh.
  Lihat `sapi-audit.md`.
- Session `$_SESSION` persist sisi-server mengikuti konfigurasi app (terbukti di
  Laravel asli: baris sqlite +1/request); `Set-Cookie` di wire ditelan CLI SAPI
  sehingga id session tak teramati di jaringan. Raw body `php://input` kosong
  di CLI — pakai `phpx_input()`. Multipart/form-data mengisi `$_POST`/`$_FILES`
  (fixture `upload.php` mem-pin kontrak); `move_uploaded_file()` tetap
  false di CLI — baca `tmp_name` atau `rename()`.
- Worker persistent memakai ulang proses PHP: `$GLOBALS`/static yang di-set
  kode app bertahan lintas request (dipin oleh test). Pakai one-shot bila
  isolasi ketat diperlukan. Redefinisi fungsi pada re-include mem-fatalkan
  worker (di-restart, dihitung crash).
- Autoscale adaptif P2 adalah opt-in eksperimental (`adaptive_enabled=false`
  default; pool tetap bila tidak). Pengaman: plafon `workers`, lantai
  `min_workers`, satu worker per interval, cooldown + hysteresis idle, tanpa
  scale-up di bawah tekanan memori guardian atau sirkuit crash-loop terbuka,
  tidak pernah saat draining. Nonaktif default hingga bukti berulang
  menunjukkan ia memperbaiki outcome tanpa osilasi atau tekanan resource
  (gerbang rencana).
- Runtime embedded tetap eksperimental dan opt-in (`available=false` hingga
  semua gerbang lolos; default `process`). Lihat `embedded.md`.
- Tanpa klaim isolasi multi-tenant. Ekstensi PHP + kode app adalah kode
  executable tepercaya (SPEC 10).
- Tanpa WAF, CDN, atau paritas reverse-proxy penuh (SPEC 6.2 ditunda).
- Transport storage S3 adalah prototipe; driver Redis queue/cache butuh server
  live (`PHPX_REDIS_URL`).
- Auto-deteksi CPU/memori cgroup: tidak ada. Gateway tetap terbatas dan
  teramati; enforcement adalah tanggung jawab operator + orkestrator
  (Docker `--cpus/--memory`, systemd `MemoryMax=`/`CPUQuota=`).
- OTel: gateway OTel-ready (passthrough `request_id` + W3C `traceparent` +
  JSON log), tanpa exporter OTLP (menjaga rilis tetap auditable).
- Tanpa encrypted-at-rest (perm file SQLite + enkripsi disk adalah tanggung
  jawab operator); upgrade adalah restart stateless + restore snapshot
  (`upgrade-rollback.md`, `backup-recovery.md`).

## Batas keamanan

- Jalankan sebagai non-root bila memungkinkan (Docker `USER phpx`, systemd `User=phpx`, `NoNewPrivileges=true`).
- Body request terbatas (`request_body_limit`), worker terbatas (`1-1024`), antrean terbatas.
- Jangan pernah mengekspos secret di log/diagnostik. Header sensitif disunting di log
  (nilai tidak dicetak); nilai header penuh hanya diteruskan ke env PHP, bukan baris log.
- Validasi path statik terhadap traversal + symlink escape. Tanpa `..`, `\`, NUL,
  `%2e/%2f/%5c/%00` pra-FS (400), cek kanonis + `starts_with` docroot, escape -> 403.

## Kapan mengadopsi

- Dev lokal, smoke CI, evaluasi Docker, prototyping PHP vanilla: ya.
- Produksi, PII, pembayaran, hosting multi-tenant: tidak — gerbang Beta/RC
  (lihat `release-gates.md`) belum terpenuhi.
