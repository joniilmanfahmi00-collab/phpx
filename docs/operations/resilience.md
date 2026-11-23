# PHPX resilience + soak runbook (P1-A)

Repeatable failure-injection and recovery evidence. All suites below run in
`cargo test --workspace` (no manual steps, no external services except the
P0-D WordPress DB which is out of scope here).

## Failure-injection matrix (all automated)

| Plan item | Test | What is proven |
|---|---|---|
| Crash while processing | `p1a: crash via crash.php` (covered pre-P1-A in `m1_execute`, `m4_compat`) + `p0b: crash_restarts_worker_with_new_incarnation` | Gateway survives; supervised worker restarts with a new incarnation + PID |
| Crash while idle | `p1a_resilience::crash_while_idle_restarts_on_next_request` (external `kill -9` / `taskkill` of the Ready worker's PID) | Next request detects the dead worker, restarts it, serves 500-then-200 |
| Request timeout | `m1_execute::execution_timeout_kills_slow_worker`, `p0b: timeout_kills_and_restarts_hung_worker` | Hung worker killed, 504, replacement serves |
| Startup failure | `p0b: startup_failure_reports_diagnostic` | Pool refuses to start with 0/N ready + reasons (no half-dead capacity) |
| Restart budget exhaustion | `p0b: restart_budget_exhaustion_marks_failed_with_reason` | Worker goes Failed with a diagnostic; executes surface it (no infinite loop) |
| Restart-loop protection | `p1a_resilience::crash_loop_opens_circuit_then_admits_again` | Consecutive crashes open the circuit (fast 503); after backoff the pool admits again |
| Graceful shutdown, in-flight | `p1a: drain_with_in_flight_{persistent,oneshot}_completes_then_stops` | In-flight finishes 200, new work fast-rejected, workers end Stopped, drain returns true within the deadline |
| Graceful reload story | `p1a: reload_is_drain_then_rebuild` | No live reload (documented): drain, rebuild, serve — pinned as the contract |
| Queue saturation / overload | `m7_resilience`, `m1_pool`, `p0b: queue_bound_is_enforced` | Bounded queue + 503 + metrics + recovery |
| Memory-threshold recycling | `p0c` soft/hard tests (controlled 32–64MB hog, `mem.php`) | Soft recycles gracefully, hard terminates protectively, triggers metered |
| State leakage | `p1a: persistent_worker_state_leaks_are_visible...` (`leak.php`) | Persistent reuse shows the documented `$GLOBALS` leak; one-shot stays clean |
| Unknown routes / 404s | `p0d_laravel`, `p0d_wordpress` | Framework 404 bodies served as 200 (CLI-SAPI limit, documented) |

## Recovery timeline (P1-A measurement)

Recorded per worker on every unplanned restart (`WorkerSnapshot::last_recovery`,
`phpx_worker_last_recovery_duration_seconds{worker_id}`):

- `detected_at` — failure detection (exchange EOF/timeout, idle-crash discovery, hard-limit sample).
- `restart_started_at` — restart initiation (kill issued for the old process).
- `ready_at` — replacement ready (fresh handshake inside `startup_timeout`).

Pinned by `p1a: recovery_timeline_reports_detection_to_ready`: legs are
reported separately and must satisfy `total == detection→restart +
restart→ready`, with total inside the startup budget. Time-to-first-success
is proven by the request after recovery returning 200.

## Soak

- CI gate (repeatable): `p1a: short_soak_mixed_workload_stays_healthy`
  (120 requests, fast + crash every 10th, 2 workers): conservation
  (`completed + failed == total`), restart accounting matches injected
  crashes, all workers Ready at end, no Failed.
- Manual long soak: `scripts/soak.sh [--execution-mode one-shot|persistent]`
  (Linux) / `scripts/soak-windows.ps1 [-Mode ...]` (Windows), plus
  `scripts/soak-long.sh` (60-min RSS CSV). Archive per run: raw logs +
  environment metadata (OS, PHP `--version`, rustc, UTC date) + config +
  `/metrics` snapshot + result summary; keep raw datasets 30 days
  (`docs/operations/baseline.md` convention). The gateway RSS should stay
  flat; worker RSS follows the workload and the guardian policy.
