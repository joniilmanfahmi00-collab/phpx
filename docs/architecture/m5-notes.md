# Catatan M5 PHPX - layanan platform (fase 1 queue + fase 2 scheduler + fase 3 cache/storage)

Tujuan: infrastruktur aplikasi yang berguna tanpa mengorbankan server inti.
Status: fase 1 + fase 2 + fase 3 SELESAI (queue + scheduler + cache/storage nyata, dispatch-only)
+ driver Redis queue/cache. Transport S3 tetap ditunda ke M5 lanjutan.

## Fase 1: queue (SELESAI)

- Crate `phpx-queue`: trait `QueueDriver` (`enqueue/reserve/ack/fail/status`),
  `MemoryQueue` (terbatas, ephemeral) + `SqliteQueue` (rusqlite bundled, tahan lama).
- Semantik (SPEC 7.1): payload terbatas 256KB, kedalaman maks 10.000, worker 1-16,
  retry 0-25, backoff `min(2^attempts,300)s + jitter(0-4s)`, `not_before` untuk tunda,
  lease visibility-timeout + pemulihan (`running -> queued` saat dibuka),
  ack tahan lama (baris sqlite dihapus agar DB tetap terbatas), dead-letter `state=dead`.
- CLI nyata: `queue status` (memory 0 + catatan ephemeral, sqlite 0 + petunjuk push bila belum ada db),
  `queue push --payload ... [--max-retries] [--delay-secs]` (ditolak pada memory, exit 2),
  `queue work --max-jobs --visibility-timeout` (worker dev reserve+ack, `fail-job` mensimulasikan kegagalan).
- Validasi: driver memory|sqlite, worker 1-16, sqlite mewajibkan path database.
- Pengujian: `contract.rs` suite yang sama untuk kedua driver (enqueue/reserve/ack, retry-delayed,
  dead-letter, `sqlite_survives_restart`), 3 unit + 6 kontrak hijau.
- Dokumen: `docs/operations/queue.md`.

## Fase 2: scheduler (SELESAI, dispatch-only per SPEC 7.2)

- Crate `phpx-scheduler` (~950 baris + uji kontrak):
  `CronSpec` (5-field, `*`/`?`/`*/step`/`a-b`/`a-b/step`/lists/nama, semantik-OR dom/dow),
  `Timezone` (Utc/Local/Fixed/Iana via chrono-tz, aman-DST wall-clock match),
  `HistoryStore` (`MemoryHistory` + `SqliteHistory` tabel `scheduler_runs`),
  `Scheduler` (`Box<dyn QueueDriver>` + `Box<dyn HistoryStore>`, `plan_window` + `tick_window` + `tick_at`),
  `TickOutcome {dispatched, skipped_disabled, skipped_missed, duplicate}`.
- Jendela: `start = end_min.saturating_sub(catchup.max(60))` (kiri-eksklusif, menit berjalan termasuk).
  Kebijakan missed `skip` / `dispatch-now` (alias `catchup`); catchup dibatasi 1 job/jadwal/tick (terbaru dulu).
  History dicek **sebelum** enqueue (dedup aman-tumpang-tindih, baseline satu-node).
- Config: `SchedulerConfig{enabled, history_file, poll_interval_secs 1-3600, schedules}` +
  `ScheduleConfig{id, cron, timezone, payload, enabled, missed_policy, max_retries}`.
  Divalidasi via `ScheduleDef::new` + cek duplikat-id + batas maks-100; pesan diratakan
  (`invalid schedule: ...`, tanpa prefiks ganda `configuration error:`).
- CLI nyata: `scheduler run [--dry-run] [--catchup-window-secs 0-86400]`.
  Nonaktif / tanpa jadwal = exit 0 + catatan. driver memory tanpa `--dry-run` = exit 2.
  Cron/TZ buruk = exit 2. `--dry-run` = hanya rencana (tanpa sentuh queue/history).
  `--json` mengeluarkan `dispatched/skipped/missed/duplicate/history`.
- Pengujian: 8 unit + 10 kontrak hijau
  (urai/cocok, semantik-OR, nama, next_after, TZ Jakarta, dedup, skip vs catchup terbatas,
  penghitungan nonaktif, penolakan duplikat-id).
  CLI `m3_cli.rs` diperbarui (scheduler run exit 0 + JSON `dispatched`).
- Dokumen: `docs/operations/scheduler.md`. Contoh: `[[scheduler.schedules]] tick * * * * * Asia/Jakarta`.
- Demo (sqlite queue+history): run1 `dispatched: tick due_at=... job=...`,
  run2 `dispatched 0 (duplicate=1)`, `queue status` depth=1.
  `--catchup-window-secs 3600` + skip = `missed=59`.

## Yang berubah vs M4

- Workspace `Cargo.toml`: + anggota `phpx-scheduler`, dep `chrono 0.4` + `chrono-tz 0.10`.
- `phpx.toml.example`: queue nyata + bagian scheduler nyata
  (`history_file`, `poll_interval_secs`, contoh `[[scheduler.schedules]]` yang dikomentari).
- `configuration.md`: tabel kunci scheduler + validasi + catatan CLI nyata.
- `queue.md`: ekor scheduler diperbaiki (tidak lagi ditunda).
- `overview.md`: subsistem queue/scheduler ditandai nyata, kontrak dispatch terbatas.
- `roadmap.md`: 4 kotak Scheduler `[x]` + kriteria keluar dispatch-only `[x]`.
- Smoke: `smoke-linux.sh` + `smoke-windows.ps1` mengharapkan queue/scheduler exit 0 (dulu exit 2 basi),
  banner M3+M5.

## Bukti keluar (M5 fase 1+2)

- `cargo fmt --check` bersih, `cargo clippy --workspace --all-targets -- -D warnings` bersih.
- `cargo test -p phpx-queue -p phpx-scheduler -p phpx-config -p phpx-cli` hijau
  (cli 5, config 4, scheduler unit 8, kontrak 10, queue kontrak 6).
- `phpx queue status` exit 0 (catatan driver), `phpx scheduler run` exit 0 (catatan nonaktif),
  `--json` keduanya terurai.
- Demo scheduler perlindungan-duplikat + batas catchup terverifikasi (lihat atas).

## Ditunda (eksplisit, M5 lanjutan / M6)

- Cache: memori terbatas + filesystem + Redis + uji kontrak.
- Storage: adaptor FS lokal + prototipe S3.
- Dispatch PHP untuk job queue (butuh integrasi runtime-process di luar thread gateway).
- Koordinasi dispatch multi-node (PK history saat ini baseline satu-node).
- Runtime embedded (M6 eksperimental), HTTP/2 gated.

Ketertelusuran: SPEC 7.1 queue + 7.2 scheduler dispatch-only + 7.3 cache + 7.4 storage tercakup oleh M5 fase 1+2+3 + susulan Redis;
ekor SPEC 15 tercakup (queue/scheduler/cache/storage); transport-S3 masih terbuka.

## Fase 3: cache + storage (SELESAI, terbatas per SPEC 7.3/7.4)

- Crate `phpx-cache`: `CacheDriver` (`get/set/delete/clear/stats`),
  `MemoryCache` (HashMap + eviksi FIFO, hit/miss) +
  `FilesystemCache` (satu file per kunci, 8-byte expiry + value, eviksi oldest-mtime, tahan dibuka-ulang) +
  `RedisCache` (envelope sama, indeks eviksi sorted-set, isolasi namespace).
  Batas: kunci 1-256 `[A-Za-z0-9 . _ - :]`, value maks 1MB, entri maks 100k, byte maks 1GB.
  TTL 0 = tanpa kedaluwarsa, maks 30 hari. Redis berbagi suite kontrak (isolasi namespace + pin TTL-expiry).
- Crate `phpx-storage`: `StorageDriver` (`put/get/delete/list/status`),
  `LocalStorage` (root + parent otomatis, traversal diblokir, list maks 1000 terurut, tahan dibuka-ulang) +
  `S3Prototype` (endpoint+bucket+region tervalidasi nyata, op data ditunda dengan catatan actionable, `status` jalan).
  Batas: path 1-512, segmen `[A-Za-z0-9 . _ - @]`, objek maks 4MB.
- Config: `CacheConfig{driver, max_memory, dir, max_entries}` +
  `StorageConfig{driver, root, endpoint, bucket, region}` dengan validasi;
  config lama tetap terurai (serde defaults). Contoh diperbarui.
- CLI nyata: `cache status|get|set|delete|clear`, `storage status|put|get|delete|list`
  (exit 0 + catatan, JSON `entries`/`objects`; kunci/path buruk exit 2; op data s3 catatan ditunda).
- Pengujian: cache 2 unit + 4 kontrak, storage 2 unit + 3 kontrak, CLI `cache_and_storage_status_real` hijau.
- Dokumen: `operations/cache.md` + `operations/storage.md`.
