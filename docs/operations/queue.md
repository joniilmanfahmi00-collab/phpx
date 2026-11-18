# PHPX queue (M5 phase 1)

Status: phase 1 DONE (memory + sqlite, retries/backoff/dead-letter, durable ack, lease recovery).
Scheduler DONE phase 2, cache/storage DONE phase 3 (see scheduler.md, cache.md, storage.md). PHPX Redis/S3-transport deferred. App-native Redis via WordPress/Laravel REDIS_HOST tetap jalan langsung dari PHP.

## Drivers

- `memory`: development/testing only, ephemeral per process, non-durable.
  `queue push` is refused on memory (jobs would vanish on exit).
  `queue status` always reports 0 for a fresh CLI invocation.
- `sqlite`: durable single-node baseline (SPEC 7.1).
  DB file at `[queue] database` (e.g. `./data/queue.db`).
  Parent dirs are auto-created. Running jobs are re-queued on open
  (forced-termination recovery). Ack deletes the row to keep the DB bounded.

PHPX Redis driver deferred to later M5 (needs network + justification). App-native Redis tetap jalan via REDIS_HOST.

## Semantics (SPEC 7.1)

- enqueue / reserve (claim) / ack / fail / retry / delay / dead-letter.
- Bounded: payload max 256KB, depth max 10,000, `queue.workers` 1-16.
  Full queue returns an error (backpressure, never unbounded).
- Retries: `max_retries` per job (0-25). Fail increments attempts.
  `attempts > max_retries` goes to dead-letter (`state=dead`).
- Backoff: `min(2^attempts, 300)s + jitter(0-4s)`. Delayed jobs are invisible
  to `reserve` until `not_before`.
- Visibility timeout: `reserve` leases the job (default 60s).
  Expired leases are re-queued on next `reserve`/`status` (lease recovery).
- Durable ack: sqlite `ack` deletes the row; `status` counts are from the DB.
- Exactly-once is NOT promised. At-least-once + idempotent consumers required.
- Graceful worker: `queue work --max-jobs N` processes up to N then exits 0.
  Ctrl-C stops between jobs (no partial ack).

## CLI (phase 1)

- `phpx queue status [--config ...] [--json]`
  memory: always 0 + ephemeral note.
  sqlite with no db file yet: 0 + "push a job first" (no file created).
- `phpx queue push --payload ''{"a":1}'' [--max-retries 3] [--delay-secs 0] [--json]`
  Refused on memory driver (exit 2). Creates sqlite DB + parents as needed.
- `phpx queue work [--max-jobs 10] [--visibility-timeout 60] [--json] [--config ...]`
  Dev worker: reserve + ack. Payload containing `fail-job` simulates failure
  to exercise retry/dead-letter path. Real PHP dispatch for queue jobs is
  phase 1.5 (must not run on gateway threads per SPEC 7.2).

## Config

```toml
[queue]
driver = "sqlite" # memory | sqlite (redis deferred)
database = "./data/queue.db" # required for sqlite
workers = 2 # 1-16
```

Validation (`phpx-config`): unknown driver exits 2, `workers` 1-16,
sqlite without `database` exits 2. Cache driver `memory|filesystem` validated;
redis deferred.

## Tests

- `crates/phpx-queue/tests/contract.rs` (same suite vs both drivers):
  enqueue/reserve/ack, retry-delayed, dead-letter, sqlite restart survival.
- `cargo test -p phpx-queue` green (3 unit + 6 contract).
- CLI: `queue status` real (0), `scheduler run` real single-pass dispatch (0 + note when disabled, see `scheduler.md`).

## Deferred (later M5)

- PHPX Redis driver (queue + cache) with integration justification (app-native Redis tetap jalan via WP/Laravel, tanpa PHPX).
- Scheduler: DONE in phase 2 (cron parser, timezone, history, duplicate-dispatch protection) — see `scheduler.md`.
- Cache: DONE in phase 3 (bounded memory + filesystem + contract tests) — see `cache.md`.
- Storage: DONE in phase 3 (local FS real + S3 prototype) — see `storage.md`.
- PHP dispatch for queue jobs (needs runtime-process integration off gateway threads).

