# PHPX scheduler (M5 phase 2)

Status: phase 2 DONE — cron-compatible, timezone-aware, single-node dispatch-only.
Queue phase 1 DONE (memory + sqlite + Redis). Cache/storage DONE phase 3 (see cache.md/storage.md). S3-transport deferred.

SPEC 7.2: scheduler **dispatches** jobs by enqueueing them into the queue.
It never executes PHP work and must not run on gateway async threads.
Each `phpx scheduler run` is one dispatch pass; run it every minute
(systemd timer or cron) for continuous operation.

## Model

- `phpx-scheduler` crate: `CronSpec`, `Timezone`, `ScheduleDef`,
  `MemoryHistory` / `SqliteHistory`, `Scheduler::tick_window()` / `tick_at()`.
- `Scheduler` owns `Box<dyn QueueDriver>` + `Box<dyn HistoryStore>`
  (not generic, so CLI stays simple).
- `tick_window(start, end, dispatch_missed)` is the single-pass entry point.
  CLI `scheduler run` uses window = `[now - max(catchup,60s), now]`,
  right-inclusive (current minute always included).
- Catchup is bounded: max 1 job per schedule per tick (newest missed slot first).
  Never backfills the whole window — keeps dispatch bounded.
- History check happens **before** enqueue, so overlapping / retried
  invocations stay safe (duplicate-dispatch protection, single-node baseline).

## Cron (5-field `minute hour dom month dow`)

- `*`, `?` (dom/dow alias for `*`), `*/step`, `a-b`, `a-b/step`, lists (`1,15,30`).
- Month / weekday names: `JAN-DEC`, `MON-SUN` (case-insensitive, full names too).
- `?` only allowed for dom/dow; other fields reject `?`.
- dom/dow use standard cron OR-semantics:
  both restricted = match when either matches;
  one `*` = the other decides.
- `next_after` brute-forces per-minute up to ~366 days (leap-safe).
- Examples: `* * * * *` every minute, `*/15 9-17 * * MON-FRI` business hours,
  `0 2 * * *` nightly 02:00.

## Timezone

- `UTC` / `Z` / `GMT` / empty = UTC.
- `local` = host local time (matching + dispatch use local wall clock).
- Offset: `+07:00`, `+0700`, `+07`, `+7`, with optional `UTC`/`GMT` prefix
  (e.g. `UTC+7`, `GMT+07:00`). Fixed offset, no DST.
- IANA: `Asia/Jakarta`, `Asia/Makassar`, `Europe/Paris`, etc. (via `chrono-tz`).
  Matching uses wall-clock parts in that zone, so DST transitions are safe
  (skipped hour = no match, repeated hour = matches once per wall minute).
- Unknown zone / bad offset = config error, exit 2 with actionable message.

## History + duplicate protection

- Trait: `was_dispatched(schedule_id, due_at) -> bool` +
  `record_dispatch(schedule_id, due_at, dispatched_at, job_id) -> bool`.
- `MemoryHistory`: ephemeral, dedup only within one process (preview / tests).
- `SqliteHistory`: table `scheduler_runs (schedule_id, due_at)` PK,
  durable across restarts. This is the production baseline.
- Config: `[scheduler] history_file = "./data/scheduler.db"`.
  Empty / missing = memory (ephemeral note in CLI output).
- Duplicate run (same `schedule_id` + `due_at` minute) = counted as
  `duplicate=1`, never double-enqueued.

## Missed-run policy (`missed_policy`, explicit per SPEC 7.2)

- `skip` (default when empty): only the current minute dispatches;
  older slots in window counted as `missed`.
- `dispatch-now` (alias `catchup` accepted): oldest→newest scan, but bounded
  to 1 catchup job per schedule per tick (newest missed slot).
  Rest counted as `missed`.
- Window comes from `--catchup-window-secs 0-86400` (default 0 = current minute
  + 60s floor). Without the flag, missed slots are only counted, never dispatched.

## CLI

- `phpx scheduler run [--config ...] [--json] [--dry-run] [--catchup-window-secs N]`
  - `enabled=false` or no schedules = exit 0 + note
    (`scheduler disabled: ...`, JSON `{"dispatched":0,...}`), never an error.
  - `driver=memory` without `--dry-run` = exit 2 (jobs would vanish).
    Use `driver=sqlite` for durable dispatch, or `--dry-run` to preview.
  - `--dry-run` = `plan_window()` only: prints what *would* dispatch,
    touches neither queue nor history.
  - `--catchup-window-secs 3600` + `dispatch-now` = 1 catchup job max per schedule.
  - Bad cron / timezone / payload / policy = exit 2 with flattened message
    (`invalid schedule: ...`), never panics.
- JSON: `{"dispatched":N,"skipped":N,"missed":N,"duplicate":N,"history":"..."}`
  (field names match `TickOutcome`).

## Config

```toml
[scheduler]
enabled = true
history_file = "./data/scheduler.db"  # empty = memory (ephemeral)
poll_interval_secs = 30               # 1-3600, informational for timer operators

[[scheduler.schedules]]
id = "tick"
cron = "* * * * *"
timezone = "Asia/Jakarta"
payload = "{\"job\":\"tick\"}"
enabled = true
missed_policy = "skip"   # skip | dispatch-now (catchup alias accepted)
max_retries = 3          # 0-25, passed to queue enqueue
```

Validation (`phpx-config`, full semantic via `ScheduleDef::new`):

- `poll_interval_secs` 1-3600, max 100 schedules, ids unique + non-empty.
- cron 5 fields, ranges checked, `?` only dom/dow.
- timezone: UTC / local / offset / IANA, else exit 2.
- payload non-empty, max 256KB.
- `missed_policy` empty = `skip`; else `skip` / `dispatch-now` / `catchup`.
- `max_retries` 0-25.

## systemd timer (recommended, every minute)

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

Single-node only. For multi-node, elect one dispatcher or share one
sqlite history file — two writers will double-dispatch (documented limit).

## Tests

- `crates/phpx-scheduler/tests/contract.rs` (10 tests):
  cron parse/match, OR-semantics, names, next_after, TZ Jakarta, history dedup,
  skip vs dispatch-now bounded catchup, disabled counting, duplicate-id reject.
- `cargo test -p phpx-scheduler` green (8 unit + 10 contract).
- CLI: `scheduler run` exit 0 (disabled note), `--json` has `dispatched`,
  `--catchup-window-secs 3600` + skip = `missed=59`, bad cron = exit 2.
- Demo: sqlite queue + history, `* * * * * Asia/Jakarta`:
  run1 `dispatched: tick`, run2 `dispatched 0 (duplicate=1)`, `queue status` depth=1.

## Deferred (later M5)

- Cache: DONE phase 3 (bounded memory + filesystem + Redis + contract tests) — see `cache.md`.
- Storage: DONE phase 3 (local FS real + S3 prototype) — see `storage.md`.
- PHP dispatch for queue jobs (needs runtime-process integration off gateway threads).
- Multi-node dispatch coordination (current dedup is single-node baseline).
