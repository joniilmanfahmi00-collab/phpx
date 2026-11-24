# PHPX cache (M5 phase 3 + Redis follow-up)

Status: phase 3 DONE — bounded memory + filesystem + Redis, TTL, contract tests.
App-native Redis via WordPress/Laravel REDIS_HOST tetap jalan langsung dari PHP, tanpa PHPX driver.

SPEC 7.3: PHP OPcache stays owned by PHP. This cache is application data
(strings/bytes) only, never opcode. All three drivers are bounded and share
one contract suite.

## Drivers

- `memory`: HashMap + FIFO eviction, per-process, ephemeral.
  Hits/misses counted in-process. Good for dev + tests.
- `filesystem`: one file per key (`<key>.cache`, 8-byte BE expiry + value),
  oldest-mtime eviction, survives restart. Good for single-node baseline.
- `redis`: shared/networked driver (same contract suite + TTL-expiry + namespace
  isolation pins). Server at `[cache] url` or `PHPX_REDIS_URL` env. Same
  8-byte-BE-expiry envelope as filesystem. Eviction oldest-first via a sorted-set
  index; bytes accounted in a counter key (lazy-expiry drift possible, same class
  as filesystem mtime drift). Hits/misses are process-local counters. Keys are
  namespaced (`phpx:cache:<namespace>:`; CLI uses `default`). Secrets: the URL
  is never logged.

## Semantics (SPEC 7.3)

- Bounded: key 1-256 chars `[A-Za-z0-9 . _ - :]`, value max 1MB,
  entries max 100,000, bytes max 1GB. Full = error (backpressure, never unbounded).
- TTL: `set --ttl-secs N`, `0` = no expiry, max 30d. Expired = miss + lazy delete.
- Operations: `get` / `set` / `delete` / `clear` / `stats`.
- Exactly-once NOT promised for callers; cache is best-effort + TTL.
  App must tolerate miss (recompute) — same as OPcache miss.

## CLI (phase 3 real)

- `phpx cache status [--json]` — entries, hits/misses, bounds.
- `phpx cache get --key foo [--json]` — hit prints value, miss exit 0 + note.
- `phpx cache set --key foo --value bar [--ttl-secs 60] [--json]`
- `phpx cache delete --key foo [--json]` — miss on delete = exit 0 + note.
- `phpx cache clear [--json]` — returns count removed.
- Bad key/value = exit 2 with actionable message.

## Config

```toml
[cache]
driver = "memory" # memory | filesystem | redis
max_memory = "128MB" # 1KB-1GB, parsed by parse_size
dir = "./data/cache" # required when driver=filesystem
url = "redis://127.0.0.1:6379" # required when driver=redis (or PHPX_REDIS_URL env)
max_entries = 1000 # 1-100000
```

Validation (`phpx-config`): unknown driver exits 2, filesystem without
`dir` exits 2, redis without `url` (and without `PHPX_REDIS_URL`) exits 2,
`max_memory` must parse 1KB-1GB, `max_entries` 1-100000.

## Tests

- `crates/phpx-cache/tests/contract.rs` (same suite vs all drivers):
  set/get, TTL/delete/clear, survive-reopen (filesystem only), TTL expiry
  (redis), namespace isolation (redis), rejects bad keys. Redis cases run
  against live `PHPX_REDIS_URL` and SKIP loudly without it.
- `cargo test -p phpx-cache` green (2 unit + 10 contract).
- CLI: `cache status` exit 0, `--json` has `entries`.

## Deferred (later M5)

- Gateway-integrated cache middleware (needs runtime-process integration).
- Cache warming / stampede protection.
