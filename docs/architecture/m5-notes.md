# PHPX M5 notes - Platform services (phase 1 queue + phase 2 scheduler + phase 3 cache/storage)

Goal: useful application infrastructure without compromising the core server.
Status: phase 1 + phase 2 + phase 3 DONE (queue + scheduler + cache/storage real, dispatch-only).
Redis/S3-transport remain deferred to later M5.

## Phase 1: queue (DONE)

- Crate `phpx-queue`: `QueueDriver` trait (`enqueue/reserve/ack/fail/status`),
  `MemoryQueue` (bounded, ephemeral) + `SqliteQueue` (rusqlite bundled, durable).
- Semantics (SPEC 7.1): bounded payload 256KB, depth max 10,000, workers 1-16,
  retries 0-25, backoff `min(2^attempts,300)s + jitter(0-4s)`, `not_before` delay,
  visibility-timeout lease + recovery (`running -> queued` on open),
  durable ack (sqlite row delete keeps DB bounded), dead-letter `state=dead`.
- CLI real: `queue status` (memory 0 + ephemeral note, sqlite 0 + push hint when no db),
  `queue push --payload ... [--max-retries] [--delay-secs]` (refused on memory, exit 2),
  `queue work --max-jobs --visibility-timeout` (dev worker reserve+ack, `fail-job` simulates failure).
- Validation: driver memory|sqlite, workers 1-16, sqlite requires database path.
- Tests: `contract.rs` same suite vs both drivers (enqueue/reserve/ack, retry-delayed,
  dead-letter, `sqlite_survives_restart`), 3 unit + 6 contract green.
- Doc: `docs/operations/queue.md`.

## Phase 2: scheduler (DONE, dispatch-only per SPEC 7.2)

- Crate `phpx-scheduler` (~950 lines + contract tests):
  `CronSpec` (5-field, `*`/`?`/`*/step`/`a-b`/`a-b/step`/lists/names, OR-semantics dom/dow),
  `Timezone` (Utc/Local/Fixed/Iana via chrono-tz, DST-safe wall-clock match),
  `HistoryStore` (`MemoryHistory` + `SqliteHistory` table `scheduler_runs`),
  `Scheduler` (`Box<dyn QueueDriver>` + `Box<dyn HistoryStore>`, `plan_window` + `tick_window` + `tick_at`),
  `TickOutcome {dispatched, skipped_disabled, skipped_missed, duplicate}`.
- Window: `start = end_min.saturating_sub(catchup.max(60))` (left-exclusive, current minute included).
  Missed policy `skip` / `dispatch-now` (`catchup` alias); catchup bounded to 1 job/schedule/tick (newest first).
  History checked **before** enqueue (overlap-safe dedup, single-node baseline).
- Config: `SchedulerConfig{enabled, history_file, poll_interval_secs 1-3600, schedules}` +
  `ScheduleConfig{id, cron, timezone, payload, enabled, missed_policy, max_retries}`.
  Validated via `ScheduleDef::new` + duplicate-id + max-100 check; messages flattened
  (`invalid schedule: ...`, no double `configuration error:` prefix).
- CLI real: `scheduler run [--dry-run] [--catchup-window-secs 0-86400]`.
  Disabled / no schedules = exit 0 + note. memory driver without `--dry-run` = exit 2.
  Bad cron/TZ = exit 2. `--dry-run` = plan only (no queue/history touch).
  `--json` emits `dispatched/skipped/missed/duplicate/history`.
- Tests: 8 unit + 10 contract green
  (parse/match, OR-semantics, names, next_after, TZ Jakarta, dedup, skip vs bounded catchup,
  disabled counting, duplicate-id reject).
  CLI `m3_cli.rs` updated (scheduler run exit 0 + JSON `dispatched`).
- Doc: `docs/operations/scheduler.md`. Example: `[[scheduler.schedules]] tick * * * * * Asia/Jakarta`.
- Demo (sqlite queue+history): run1 `dispatched: tick due_at=... job=...`,
  run2 `dispatched 0 (duplicate=1)`, `queue status` depth=1.
  `--catchup-window-secs 3600` + skip = `missed=59`.

## What changed vs M4

- `Cargo.toml` workspace: + `phpx-scheduler` member, deps `chrono 0.4` + `chrono-tz 0.10`.
- `phpx.toml.example`: queue real + scheduler real section
  (`history_file`, `poll_interval_secs`, commented `[[scheduler.schedules]]` example).
- `configuration.md`: scheduler keys table + validation + CLI real notes.
- `queue.md`: scheduler tail fixed (no longer deferred).
- `overview.md`: subsystems queue/scheduler marked real, contracts bounded dispatch.
- `roadmap.md`: Scheduler 4 boxes `[x]` + exit criterion dispatch-only `[x]`.
- Smoke: `smoke-linux.sh` + `smoke-windows.ps1` expect queue/scheduler exit 0 (was stale exit 2),
  banner M3+M5.

## Exit evidence (M5 phase 1+2)

- `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test -p phpx-queue -p phpx-scheduler -p phpx-config -p phpx-cli` green
  (cli 5, config 4, scheduler unit 8, contract 10, queue contract 6).
- `phpx queue status` exit 0 (driver note), `phpx scheduler run` exit 0 (disabled note),
  `--json` both parse.
- Scheduler demo duplicate-protection + catchup bound verified (see above).

## Deferred (explicit, later M5 / M6)

- Redis adapter (queue + cache) with integration justification.
- Cache: bounded memory + filesystem + contract tests.
- Storage: local FS adapter + S3 prototype.
- PHP dispatch for queue jobs (needs runtime-process integration off gateway threads).
- Multi-node dispatch coordination (current history PK is single-node baseline).
- Embedded runtime (M6 experimental), HTTP/2 gated.

Traceability: SPEC 7.1 queue + 7.2 scheduler dispatch-only + 7.3 cache + 7.4 storage covered by M5 phase 1+2+3;
SPEC 15 tail covered (queue/scheduler/cache/storage); Redis/S3-transport still open.

## Phase 3: cache + storage (DONE, bounded per SPEC 7.3/7.4)

- Crate `phpx-cache`: `CacheDriver` (`get/set/delete/clear/stats`),
  `MemoryCache` (HashMap + FIFO eviction, hits/misses) +
  `FilesystemCache` (one file per key, 8-byte expiry + value, oldest-mtime eviction, survives reopen).
  Bounds: key 1-256 `[A-Za-z0-9 . _ - :]`, value max 1MB, entries max 100k, bytes max 1GB.
  TTL 0 = no expiry, max 30d. Redis deferred (needs network + justification).
- Crate `phpx-storage`: `StorageDriver` (`put/get/delete/list/status`),
  `LocalStorage` (root + parents auto, traversal blocked, list max 1000 sorted, survives reopen) +
  `S3Prototype` (endpoint+bucket+region validated real, data ops deferred with actionable note, `status` works).
  Bounds: path 1-512, segments `[A-Za-z0-9 . _ - @]`, object max 4MB.
- Config: `CacheConfig{driver, max_memory, dir, max_entries}` +
  `StorageConfig{driver, root, endpoint, bucket, region}` with validation;
  old configs keep parsing (serde defaults). Example updated.
- CLI real: `cache status|get|set|delete|clear`, `storage status|put|get|delete|list`
  (exit 0 + notes, JSON `entries`/`objects`; bad key/path exit 2; s3 data ops deferred note).
- Tests: cache 2 unit + 4 contract, storage 2 unit + 3 contract, CLI `cache_and_storage_status_real` green.
- Docs: `operations/cache.md` + `operations/storage.md`.

