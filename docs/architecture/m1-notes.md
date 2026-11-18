# M1 implementation notes (Process Runtime prototype)

Status: implemented, all exit criteria verified on PHP 8.3.30 / Rust 1.99.1 (Windows dev; Linux CI via .github/workflows/ci.yml).

## What landed

- `crates/phpx-runtime`: `PROTOCOL_VERSION = 1`, `PhpRequest` / `PhpResponse` / `WorkerOutcome`,
  length-prefixed JSON framing (`encode_frame` / `decode_frame` / `write_frame`, 16 MiB cap).
  Same framing is reused for the future Unix-socket transport (M2); M1 prototype
  exercises it over stdio pipes for cross-platform parity.
- `crates/phpx-runtime-process`: `PhpDiscovery::inspect` (php --version + php -m),
  `execute_one_shot` (fresh `php <entrypoint>` per request, env `PHPX_*`, stdin body,
  stdout = body, stderr = diagnostics, `wait_timeout` kill on expiry).
  One-request workers guarantee no cross-request state by construction (SPEC 4.2 allowed mode).
- `crates/phpx-runtime-process/bootstrap/worker-bootstrap.php`: persistent framed worker
  prototype (manual use). Isolation reset hooks deferred to ADR-0014 2.8 / M4.
- `crates/phpx-worker`: `BoundedPool` (max_workers 1-1024, queue_bound bounded,
  queue_wait_timeout, execution_timeout, overload rejection, stats).
  Crash/timeout increments stats, never takes down the pool.
- Fixtures `examples/vanilla-php/public/`: `index.php`, `isolate.php`, `crash.php`, `sleep.php`.
- Tests: `m1_execute.rs` (5 passed), `m1_pool.rs` (2 passed + 1 unit), plus
  `phpx-runtime` framing/version tests (3 passed).

## Exit criteria evidence

- Start + execute: `executes_vanilla_fixture` (200 + rid echo).
- Bounded: `bounded_pool_serves_concurrent_requests` (max 2, 4 concurrent ok),
  `overload_is_rejected_not_queued_unboundedly` (fast overload error, then recovery).
- Crash isolation: `crash_does_not_take_down_supervisor`.
- No cross-contamination: `sequential_requests_do_not_cross_contaminate` (isolate.php clean x3).
- One PHP version: PHP 8.3.30 verified via `phpx doctor` + integration tests.

## Deferred to M2/M4

- Real UnixListener transport (same framing bytes), FastCGI evaluation.
- Persistent-pool recycling by request count/memory (ADR-0014 Phase B-D).
- Startup vs execution timeout split (M1 uses single execution timeout).
