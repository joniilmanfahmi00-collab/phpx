# Scheduler PHPX (M5 fase 2)

Status: fase 2 SELESAI — cron-kompatibel, sadar-timezone, dispatch-only satu-node.
Queue fase 1 SELESAI (memory + sqlite + Redis). Cache/storage SELESAI fase 3 (lihat cache.md/storage.md). Transport-S3 ditunda.

SPEC 7.2: scheduler **mendispatch** job dengan meng-enqueue-nya ke queue.
Ia tidak pernah mengeksekusi kerja PHP dan tidak boleh berjalan di thread async gateway.
Setiap `phpx scheduler run` adalah satu pass dispatch; jalankan tiap menit
(timer systemd atau cron) untuk operasi kontinu.

## Model

- Crate `phpx-scheduler`: `CronSpec`, `Timezone`, `ScheduleDef`,
  `MemoryHistory` / `SqliteHistory`, `Scheduler::tick_window()` / `tick_at()`.
- `Scheduler` memegang `Box<dyn QueueDriver>` + `Box<dyn HistoryStore>`
  (bukan generik, agar CLI tetap sederhana).
- `tick_window(start, end, dispatch_missed)` adalah titik-masuk pass-tunggal.
  `scheduler run` CLI memakai jendela = `[now - max(catchup,60s), now]`,
  inklusif-kanan (menit berjalan selalu termasuk).
- Catchup dibatasi: maks 1 job per jadwal per tick (slot missed terbaru dulu).
  Tidak pernah mengisi-ulang seluruh jendela — agar dispatch tetap terbatas.
- Cek history terjadi **sebelum** enqueue, sehingga invokasi yang tumpang-tindih / diulang
  tetap aman (proteksi dispatch-ganda, baseline satu-node).

## Cron (5-field `minute hour dom month dow`)

- `*`, `?` (alias `*` untuk dom/dow), `*/step`, `a-b`, `a-b/step`, daftar (`1,15,30`).
- Nama bulan / hari: `JAN-DEC`, `MON-SUN` (case-insensitive, nama penuh juga).
- `?` hanya boleh untuk dom/dow; field lain menolak `?`.
- dom/dow memakai semantik-OR cron standar:
  keduanya restriktif = cocok bila salah satu cocok;
  salah satu `*` = yang lain yang menentukan.
- `next_after` mem-brute-force per-menit sampai ~366 hari (aman-kabisat).
- Contoh: `* * * * *` tiap menit, `*/15 9-17 * * MON-FRI` jam kerja,
  `0 2 * * *` malam 02:00.

## Timezone

- `UTC` / `Z` / `GMT` / kosong = UTC.
- `local` = waktu lokal host (pencocokan + dispatch memakai wall clock lokal).
- Offset: `+07:00`, `+0700`, `+07`, `+7`, dengan prefiks opsional `UTC`/`GMT`
  (mis. `UTC+7`, `GMT+07:00`). Offset tetap, tanpa DST.
- IANA: `Asia/Jakarta`, `Asia/Makassar`, `Europe/Paris`, dll. (via `chrono-tz`).
  Pencocokan memakai bagian wall-clock di zona itu, sehingga transisi DST aman
  (jam yang dilompati = tanpa cocok, jam yang diulang = cocok sekali per menit wall).
- Zona tak dikenal / offset buruk = error config, exit 2 dengan pesan actionable.

## History + proteksi duplikat

- Trait: `was_dispatched(schedule_id, due_at) -> bool` +
  `record_dispatch(schedule_id, due_at, dispatched_at, job_id) -> bool`.
- `MemoryHistory`: ephemeral, dedup hanya dalam satu proses (pratinjau / pengujian).
- `SqliteHistory`: tabel `scheduler_runs (schedule_id, due_at)` PK,
  tahan antar restart. Ini baseline produksi.
- Config: `[scheduler] history_file = "./data/scheduler.db"`.
  Kosong / hilang = memory (catatan ephemeral di output CLI).
- Run duplikat (`schedule_id` + `due_at` menit yang sama) dihitung sebagai
  `duplicate=1`, tidak pernah di-enqueue ganda.

## Kebijakan missed-run (`missed_policy`, eksplisit per SPEC 7.2)

- `skip` (default bila kosong): hanya menit berjalan yang didispatch;
  slot lama dalam jendela dihitung sebagai `missed`.
- `dispatch-now` (alias `catchup` diterima): pindai tertua→terbaru, tetapi dibatasi
  1 job catchup per jadwal per tick (slot missed terbaru).
  Sisanya dihitung sebagai `missed`.
- Jendela berasal dari `--catchup-window-secs 0-86400` (default 0 = menit berjalan
  + lantai 60s). Tanpa flag, slot missed hanya dihitung, tidak pernah didispatch.

## CLI

- `phpx scheduler run [--config ...] [--json] [--dry-run] [--catchup-window-secs N]`
  - `enabled=false` atau tanpa jadwal = exit 0 + catatan
    (`scheduler disabled: ...`, JSON `{"dispatched":0,...}`), tidak pernah error.
  - `driver=memory` tanpa `--dry-run` = exit 2 (job akan lenyap).
    Pakai `driver=sqlite` untuk dispatch tahan-lama, atau `--dry-run` untuk pratinjau.
  - `--dry-run` = hanya `plan_window()`: mencetak apa yang *akan* didispatch,
    tanpa menyentuh queue maupun history.
  - `--catchup-window-secs 3600` + `dispatch-now` = maks 1 job catchup per jadwal.
  - Cron / timezone / payload / policy buruk = exit 2 dengan pesan rata
    (`invalid schedule: ...`), tidak pernah panic.
- JSON: `{"dispatched":N,"skipped":N,"missed":N,"duplicate":N,"history":"..."}`
  (nama field cocok dengan `TickOutcome`).

## Config

```toml
[scheduler]
enabled = true
history_file = "./data/scheduler.db"  # kosong = memory (ephemeral)
poll_interval_secs = 30               # 1-3600, informasional untuk operator timer

[[scheduler.schedules]]
id = "tick"
cron = "* * * * *"
timezone = "Asia/Jakarta"
payload = "{\"job\":\"tick\"}"
enabled = true
missed_policy = "skip"   # skip | dispatch-now (alias catchup diterima)
max_retries = 3          # 0-25, diteruskan ke enqueue queue
```

Validasi (`phpx-config`, semantik penuh via `ScheduleDef::new`):

- `poll_interval_secs` 1-3600, maks 100 jadwal, id unik + non-kosong.
- cron 5 field, range dicek, `?` hanya dom/dow.
- timezone: UTC / local / offset / IANA, jika tidak exit 2.
- payload non-kosong, maks 256KB.
- `missed_policy` kosong = `skip`; selain itu `skip` / `dispatch-now` / `catchup`.
- `max_retries` 0-25.

## Timer systemd (disarankan, tiap menit)

```ini
# /etc/systemd/system/phpx-scheduler.service
[Service]
Type=oneshot
ExecStart=/usr/local/bin/phpx scheduler run --config /etc/phpx/phpx.toml
# /etc/systemd/system/phpx-scheduler.timer
[Timer]
OnCalendar=*:*:00
Persistent=true
[Install]
WantedBy=timers.target
```

Satu-node saja. Untuk multi-node, pilih satu dispatcher atau pakai satu
file history sqlite bersama — dua writer akan mendispatch ganda (batas terdokumentasi).

## Pengujian

- `crates/phpx-scheduler/tests/contract.rs` (10 pengujian):
  urai/cocok cron, semantik-OR, nama, next_after, TZ Jakarta, dedup history,
  skip vs dispatch-now catchup terbatas, penghitungan nonaktif, penolakan duplikat-id.
- `cargo test -p phpx-scheduler` hijau (8 unit + 10 kontrak).
- CLI: `scheduler run` exit 0 (catatan nonaktif), `--json` punya `dispatched`,
  `--catchup-window-secs 3600` + skip = `missed=59`, cron buruk = exit 2.
- Demo: queue sqlite + history, `* * * * * Asia/Jakarta`:
  run1 `dispatched: tick`, run2 `dispatched 0 (duplicate=1)`, `queue status` depth=1.

## Ditunda (M5 lanjutan)

- Cache: SELESAI fase 3 (memori terbatas + filesystem + Redis + uji kontrak) — lihat `cache.md`.
- Storage: SELESAI fase 3 (FS lokal nyata + prototipe S3) — lihat `storage.md`.
- Dispatch PHP untuk job queue (butuh integrasi runtime-process di luar thread gateway).
- Koordinasi dispatch multi-node (dedup saat ini baseline satu-node).
