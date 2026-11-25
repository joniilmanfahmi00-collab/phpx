# Queue PHPX (M5 fase 1 + susulan Redis)

Status: fase 1 SELESAI (memory + sqlite, retry/backoff/dead-letter, ack tahan-lama, pemulihan lease)
+ driver Redis (butuh server; `PHPX_REDIS_URL` atau `[queue] url`).
Scheduler SELESAI fase 2, cache/storage SELESAI fase 3 (lihat scheduler.md, cache.md, storage.md). Redis app-native via REDIS_HOST WordPress/Laravel tetap jalan langsung dari PHP.

## Driver

- `memory`: hanya pengembangan/pengujian, ephemeral per proses, non-durable.
  `queue push` ditolak pada memory (job akan lenyap saat exit).
  `queue status` selalu melaporkan 0 untuk invokasi CLI baru.
- `sqlite`: baseline satu-node yang tahan-lama (SPEC 7.1).
  File DB di `[queue] database` (mis. `./data/queue.db`).
  Direktori parent otomatis dibuat. Job yang sedang berjalan diantrekan-ulang saat dibuka
  (pemulihan terminasi-paksa). Ack menghapus baris agar DB tetap terbatas.
- `redis`: driver bersama/berjaringan (suite kontrak sama).
  Server di `[queue] url` (mis. `redis://127.0.0.1:6379`) atau env `PHPX_REDIS_URL`.
  Kunci di-namespace (`phpx:q:<namespace>:`; CLI memakai `default`, pengujian memakai
  uuid per kasus + `flush_namespace()` higiene). Ready set adalah ZSET yang diskor berdasar
  waktu visibility (FIFO dalam detik yang sama); perilaku lease/kedaluwarsa seperti
  driver memory. `move` antar proses aman untuk namespace berbeda;
  writer konkuren ke satu namespace adalah last-write-wins per id job (lingkup dev:
  satu gateway writer). Timeout koneksi 5s; kegagalan muncul sebagai
  `Runtime("redis ...")` (tidak pernah diam). Secret: URL tidak pernah di-log
  (hanya namespace yang muncul di error/metrik).

## Semantik (SPEC 7.1)

- enqueue / reserve (klaim) / ack / fail / retry / delay / dead-letter.
- Terbatas: payload maks 256KB, kedalaman maks 10.000, `queue.workers` 1-16.
  Queue penuh mengembalikan error (backpressure, tidak pernah tanpa batas).
- Retry: `max_retries` per job (0-25). Fail menaikkan attempts.
  `attempts > max_retries` masuk dead-letter (`state=dead`).
- Backoff: `min(2^attempts, 300)s + jitter(0-4s)`. Job tertunda tak terlihat
  oleh `reserve` sampai `not_before`.
- Visibility timeout: `reserve` me-lease job (default 60s).
  Lease kedaluwarsa diantrekan-ulang pada `reserve`/`status` berikutnya (pemulihan lease).
- Ack tahan-lama: `ack` sqlite menghapus baris; hitungan `status` dari DB.
- Exactly-once TIDAK dijanjikan. At-least-once + konsumen idempoten diwajibkan.
- Worker graceful: `queue work --max-jobs N` memproses sampai N lalu exit 0.
  Ctrl-C berhenti antar job (tanpa ack parsial).

## CLI (fase 1)

- `phpx queue status [--config ...] [--json]`
  memory: selalu 0 + catatan ephemeral.
  sqlite tanpa file db: 0 + "push a job first" (tanpa file dibuat).
- `phpx queue push --payload ''{"a":1}'' [--max-retries 3] [--delay-secs 0] [--json]`
  Ditolak pada driver memory (exit 2). Membuat DB sqlite + parent sesuai kebutuhan.
- `phpx queue work [--max-jobs 10] [--visibility-timeout 60] [--json] [--config ...]`
  Worker dev: reserve + ack. Payload berisi `fail-job` mensimulasikan kegagalan
  untuk melatih path retry/dead-letter. Dispatch PHP nyata untuk job queue adalah
  fase 1.5 (tidak boleh di thread gateway per SPEC 7.2).

## Config

```toml
[queue]
driver = "sqlite" # memory | sqlite | redis
database = "./data/queue.db" # wajib untuk sqlite
url = "redis://127.0.0.1:6379" # wajib untuk redis (atau env PHPX_REDIS_URL)
workers = 2 # 1-16
```

Validasi (`phpx-config`): driver tak dikenal exit 2, `workers` 1-16,
sqlite tanpa `database` exit 2, redis tanpa `url` (dan tanpa
`PHPX_REDIS_URL`) exit 2.

## Pengujian

- `crates/phpx-queue/tests/contract.rs` (suite yang sama vs semua driver):
  enqueue/reserve/ack, retry-delayed, dead-letter, restart-survival sqlite,
  isolasi namespace redis. Kasus Redis berjalan melawan `PHPX_REDIS_URL` live
  dan SKIP dengan lantang tanpanya.
- `cargo test -p phpx-queue` hijau (3 unit + 10 kontrak).
- CLI: `queue status` nyata (0), `scheduler run` nyata sekali-pass dispatch (0 + catatan bila nonaktif, lihat `scheduler.md`).

## Ditunda (M5 lanjutan)

- Scheduler: SELESAI di fase 2 (parser cron, timezone, history, proteksi dispatch-ganda) — lihat `scheduler.md`.
- Cache: SELESAI di fase 3 (memori terbatas + filesystem + Redis + uji kontrak) — lihat `cache.md`.
- Storage: SELESAI di fase 3 (FS lokal nyata + prototipe S3) — lihat `storage.md`.
- Dispatch PHP untuk job queue (butuh integrasi runtime-process di luar thread gateway).
