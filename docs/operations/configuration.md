# Konfigurasi PHPX - TOML skema v1, default terdokumentasi (hardening M4 + queue/scheduler/cache/storage M5)

File primer: `phpx.toml`. Contoh: `phpx.toml.example`.
Resolusi untuk perintah `phpx serve` / `runtime` / `workers` / `queue` / `scheduler`:
`--config <path>` > `./phpx.toml` > `phpx.toml.example`.

Kode exit: `0` ok, `1` kegagalan runtime, `2` config/penggunaan.
Semua error config bersifat actionable dan menyertakan path file.

## Config developer minimal

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

## Default penuh (skema v1)

| Kunci | Default | Catatan |
|-----|---------|-------|
| `schema_version` | `1` (wajib) | Hanya `1` yang diterima di M3-M5 |
| `server.host` | `127.0.0.1` | Pakai `0.0.0.0` di Docker/systemd di balik firewall |
| `server.port` | `8080` | Override: `phpx serve --port 9000` |
| `server.tls` | `false` | `true` mewajibkan `tls_cert_file` + `tls_key_file` (PEM), jika tidak exit 2 |
| `server.tls_cert_file` | none | Wajib bila `tls=true` |
| `server.tls_key_file` | none | Wajib bila `tls=true` |
| `server.request_body_limit` | `"16MB"` | `parse_size`: byte atau `K/KB/M/MB/G/GB/B`, mis. `512KB`, `256M`. Kelebihan -> `413` |
| `application.root` | `"."` | Diresolusi terhadap cwd; default Docker `/app/examples/vanilla-php` |
| `application.document_root` | `"public"` | Hanya file di bawah sini yang disajikan; traversal diblokir |
| `application.entrypoint` | `"index.php"` | Wajib ada; fallback front-controller |
| `runtime.mode` | `"process"` | Hanya `process` yang aktif; `embedded` adalah opt-in eksperimental sejak M6.1 (stub) |
| `runtime.php_version` | `"8.4"` | Target untuk `doctor`/`runtime list`; aktual dari `php --version` |
| `runtime.process.workers` | `4` | Ukuran pool maks, terbatas `1-1024`, tidak pernah tanpa batas. Overload/circuit/drain -> `503 + Retry-After: 1` |
| `runtime.process.execution_mode` | `"one-shot"` | P0-A eksplisit, P0-B terimplementasi. `"one-shot"` = `php` baru per request; `"persistent"` = worker terawasi berukuran-tetap (handshake-gated, state/metrik per-worker). Dilaporkan oleh CLI/JSON + `phpx_execution_mode_info` |
| `runtime.process.min_workers` | `1` | Ukuran pool min, `1-1024`, wajib `<= workers`. Lantai kapasitas-ready untuk adaptif masa depan (M4: divalidasi, pool menghormati max) |
| `runtime.process.max_requests` | `1000` | Anggaran recycle logis per slot (`>=1`); one-shot sudah mengisolasi per request; dihitung menuju `recycled` di `/metrics` |
| `runtime.process.request_timeout_seconds` | `30` | Membatasi eksekusi PHP + drain graceful |
| `runtime.process.startup_timeout_seconds` | `10` | Disiapkan untuk worker terawasi (M4) |
| `runtime.process.memory_limit` | `"256M"` | P0-C: batas heap PHP nyata, diteruskan sebagai `-d memory_limit=...` ke setiap anak PHP (one-shot + persistent). Hanya membatasi Zend heap, BUKAN RSS OS. `"0"`/kosong = default php |
| `runtime.process.memory_soft_limit` | `"0"` | Guardian P0-C (hanya persistent): batas lunak OS-RSS, `"0"` = nonaktif. Pada/di atas = recycle graceful (pengganti dulu, staggered). Wajib di bawah hard bila keduanya diset |
| `runtime.process.memory_hard_limit` | `"0"` | Guardian P0-C (hanya persistent): batas keras OS-RSS, `"0"` = nonaktif. Pada/di atas = terminasi protektif + respawn dengan alasan diagnostik |
| `runtime.process.memory_sample_interval_seconds` | `5` | `1-60`. Periode sampling RSS worker idle. Selesai-request adalah titik sampel tambahan |
| `runtime.process.queue_bound` | otomatis (`workers*4`, min 16) | Eksplisit `1-100000` meng-override otomatis. Menunggu melampaui bound -> `503` (tidak pernah tanpa batas) |
| `runtime.process.queue_wait_timeout_seconds` | `2` | `0-120`. Tunggu slot worker sebelum `503` |
| `runtime.process.shutdown_drain_timeout_seconds` | `10` | `1-120`. Anggaran drain SIGTERM/Ctrl-C; pool berhenti menerima intake (`draining` -> `503`), menunggu in-flight |
| `runtime.process.restart_max_retries` | `5` | `0-100`. Anggaran failure-beruntun sebelum circuit crash-loop terbuka (ADR-0014 2.6) |
| `runtime.process.restart_backoff_ms` | `250` | `10-60000`. Backoff dasar untuk jendela circuit |
| `runtime.process.restart_backoff_max_ms` | `30000` | `>= base`, `<=300000`. Cap backoff (eksponensial + jitter) |
| `runtime.process.adaptive_enabled` | `false` | Adaptif P2 (hanya persistent, eksperimental opt-in): mulai di `min_workers`, tumbuh saat tekanan berkelanjutan sampai `workers`, susut saat idle. `false` = pool tetap berisi `workers` (default, perilaku tidak berubah) |
| `runtime.process.adaptive_interval_seconds` | `5` | `1-60`. Periode evaluasi control-loop |
| `runtime.process.adaptive_cooldown_seconds` | `30` | `1-300`. Jeda tenang antar aksi scale (anti-flap, dengan histeresis) |
| `runtime.process.adaptive_scale_up_queue_depth` | `2` | `1-10000`. Scale-up bila `waiting` mencapai ini DAN setiap worker spawn sibuk |
| `runtime.process.adaptive_scale_down_idle_intervals` | `3` | `1-360`. Scale-down setelah evaluasi all-idle beruntun sebanyak ini |
| `runtime.process.adaptive_memory_guard` | `true` | Tolak scale-up saat guardian menunjukkan tekanan memori worker (scale-down tetap boleh) |
| `runtime.embedded.enabled` | `false` | Opt-in eksperimental sejak M6.1 (stub: available=false); process tetap default |
| `queue.driver` | `"sqlite"` | `memory` (dev ephemeral) / `sqlite` (tahan-lama) / `redis` (server bersama). `queue status` nyata exit 0 (M5 fase 1 + Redis) |
| `queue.database` | `"./data/queue.db"` | Wajib bila `driver=sqlite` |
| `queue.url` | none | Wajib bila `driver=redis`, mis. `redis://127.0.0.1:6379` (atau env `PHPX_REDIS_URL`) |
| `queue.workers` | `2` | `1-16` (terbatas) |
| `scheduler.enabled` | `false` | `false` = `scheduler run` exit 0 + catatan. `true` + jadwal = dispatch nyata (M5 fase 2) |
| `scheduler.history_file` | none (memory) | mis. `"./data/scheduler.db"` untuk dedup tahan-lama; kosong = ephemeral |
| `scheduler.poll_interval_secs` | `30` | `1-3600`. Informasional untuk operator timer (CLI sekali-pass) |
| `scheduler.schedules` | `[]` | Maks 100, id unik. Lihat `scheduler.md` untuk kunci `[[scheduler.schedules]]` |
| `cache.driver` | `"memory"` | `memory` (ephemeral) / `filesystem` (tahan-lama) / `redis` (server bersama). `cache status|get|set|clear` nyata exit 0 (M5 fase 3 + Redis) |
| `cache.max_memory` | `"128MB"` | `1KB-1GB` via `parse_size`. Batas untuk memory+filesystem |
| `cache.dir` | none | Wajib bila `driver=filesystem`, mis. `./data/cache` |
| `cache.url` | none | Wajib bila `driver=redis`, mis. `redis://127.0.0.1:6379` (atau env `PHPX_REDIS_URL`) |
| `cache.max_entries` | `1000` | `1-100000` (terbatas) |
| `storage.driver` | `"local"` | `local` (nyata) / `s3` (prototipe: config nyata, transport ditunda) |
| `storage.root` | none | Wajib bila `driver=local`, mis. `./data/storage` |
| `storage.endpoint` | none | Wajib bila `driver=s3`, wajib berawalan `http://`/`https://` |
| `storage.bucket` | none | Wajib bila `driver=s3` |
| `storage.region` | `"auto"` | Region S3, default `auto` untuk prototipe |
| `observability.json_logs` | `true` | `phpx serve --json` mengaktifkan log JSON |
| `observability.metrics` | `true` | M4 = access log + `/healthz` + `/readyz` + `/metrics` (teks Prometheus) |

### Kunci `[[scheduler.schedules]]` (M5 fase 2)

| Kunci | Wajib | Catatan |
|-----|----------|-------|
| `id` | ya | Non-kosong, unik antar jadwal |
| `cron` | ya | 5-field `minute hour dom month dow`, nama + step + daftar. Lihat `scheduler.md` |
| `timezone` | tidak (default UTC) | `UTC` / `local` / offset `+07:00` / IANA `Asia/Jakarta` |
| `payload` | ya | String JSON non-kosong, maks 256KB, di-enqueue sebagai job queue |
| `enabled` | tidak (default true) | `false` = dihitung sebagai `skipped_disabled`, tidak pernah didispatch |
| `missed_policy` | tidak (default `skip`) | `skip` / `dispatch-now` (alias `catchup` diterima). Eksplisit per SPEC 7.2 |
| `max_retries` | tidak (default 3) | `0-25`, diteruskan ke enqueue queue |

## Aturan validasi (M4-M5)

- `schema_version != 1` -> error, exit 2.
- `port == 0` -> error.
- `tls=true` tanpa cert+key -> error yang menyebut kedua file.
- `request_body_limit` wajib terurai via `parse_size`, jika tidak error dengan format yang diharapkan.
- `application.root` / `entrypoint` wajib non-kosong; docroot wajib ada dan file entry wajib ada saat startup.
- `runtime.mode` wajib `process` atau `embedded`; `embedded` tanpa `[runtime.embedded] enabled=true` ditolak.
- `workers` wajib `1-1024`, `min_workers <= workers`, batas per kunci (lihat tabel).
  Dengan `adaptive_enabled=true`, `workers` adalah langit-langit dan `min_workers` lantai + ukuran awal.
- `runtime.process.execution_mode` wajib `one-shot` atau `persistent` (P0-B: keduanya runnable; persistent = pool terawasi berukuran-tetap, atau adaptif bila diaktifkan).
- Titik penegakan (P0-C): `memory_limit` = heap PHP (`-d`, hanya Zend, kedua mode);
  `memory_soft_limit`/`memory_hard_limit` = kebijakan supervisor OS RSS (hanya worker
  persistent); soft wajib < hard bila keduanya diset; `memory_sample_interval_seconds` 1-60.
  RSS != heap PHP: perilaku allocator, ekstensi native, shared library dan
  fragmentasi memengaruhi RSS teramati; overshoot sampling antar titik sampel
  dimungkinkan dan tidak menyiratkan leak (atau membuktikan ketiadaannya).
- `queue.driver` wajib `memory`, `sqlite` atau `redis`; `workers` 1-16; sqlite mewajibkan `database`; redis mewajibkan `url` (atau `PHPX_REDIS_URL`).
- `scheduler.poll_interval_secs` 1-3600; maks 100 jadwal; id unik.
  Setiap jadwal divalidasi via `ScheduleDef::new` (cron, timezone, payload,
  policy, retry) — config dan CLI tidak pernah bisa berbeda. Jadwal buruk = exit 2
  dengan pesan rata `invalid schedule: ...`.
- `cache.driver` wajib `memory`, `filesystem` atau `redis`; filesystem mewajibkan `dir`; redis mewajibkan `url` (atau `PHPX_REDIS_URL`); `max_memory` 1KB-1GB; `max_entries` 1-100000.
- `storage.driver` wajib `local` atau `s3`; local mewajibkan `root`; s3 mewajibkan `endpoint` + `bucket` (prototipe, transport ditunda).

## Override CLI (M3 difinalkan, queue/scheduler/cache/storage nyata M5)

- `phpx serve --host 0.0.0.0 --port 9000 --config /etc/phpx/phpx.toml`
- `phpx serve --json` mencetak startup JSON termasuk `health: {liveness: /healthz, readiness: /readyz}`.
- `phpx doctor [--json]`, `phpx runtime list|inspect [--json]`, `phpx workers status|reload [--json]`, `phpx metrics [--json]`.
- `phpx queue status|push|work` nyata (M5 fase 1, exit 0; `push` ditolak pada driver memory dengan exit 2).
- `phpx cache status|get|set|delete|clear` nyata (M5 fase 3, exit 0; kunci/value buruk exit 2).
- `phpx storage status|put|get|delete|list` nyata (M5 fase 3, exit 0; op data s3 catatan ditunda, path buruk exit 2).
- `phpx scheduler run [--dry-run] [--catchup-window-secs N]` dispatch sekali-pass nyata (M5 fase 2, exit 0 + catatan bila nonaktif; exit 2 pada cron/TZ buruk atau driver memory tanpa `--dry-run`).

Lihat juga: `queue.md`, `scheduler.md`, `cache.md`, `storage.md`, `quickstart-linux.md`, `quickstart-docker.md`, `php-version-matrix.md`, `limitations.md`, `systemd.md`.
