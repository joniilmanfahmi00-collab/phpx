# PHPX Engineering Review & Remediation Plan

**Repository:** `joniilmanfahmi00-collab/phpx`\
**Review basis:** repository files inspected on 2026-11-20\
**Status:** recommendations, not implementation claims

> This document records findings and recommended next steps. It does not
> claim that proposed work has already been implemented or tested.
> Re-check the current branch before acting on each item.

## 1. Executive summary

PHPX has a clear direction: a Rust-powered PHP application platform
intended to run conventional PHP applications without source changes,
with Process Runtime as the baseline and Embedded Runtime experimental.

The repository already has useful foundations: a modular Rust workspace,
TOML configuration, HTTP gateway, bounded execution admission, timeout
and overload handling, operational documentation, Docker packaging,
tests, and a proposed worker lifecycle/resource-management design.

The main gap is between the intended **supervised worker and
resource-management model** and the current Process Runtime execution
path. The inspected `phpx-worker` path calls
`phpx_runtime_process::execute_one_shot(...)` for each admitted request.
This gives request-level process isolation, but is not yet a persistent,
individually supervised PHP worker pool with per-worker health and
RSS-aware recycling.

The next step should not be adding more platform features. It should be
making Process Runtime lifecycle and resource behavior explicit,
measurable, and testable.

## 2. Product framing to preserve

PHPX should not be framed as a project whose sole goal is to beat Go,
FrankenPHP, RoadRunner, or another runtime on raw RPS.

Its intended value is broader:

1.  **Resource management:** observe and control worker/process resource
    consumption.
2.  **Runtime lifecycle orchestration:** start, monitor, drain, recycle,
    and replace execution workers predictably.
3.  **Reliability and resilience:** contain failures, recover from
    worker exits, prevent restart loops, and apply bounded backpressure.
4.  **Strict PHP compatibility:** run conventional PHP applications
    without requiring PHPX-specific source changes.
5.  **Performance:** remain an important measurable dimension, but not
    the only success criterion.

Concurrency is a workload condition and a test dimension. It is not the
entire product definition.

### Claims discipline

Do not claim that PHPX: - is leak-free because its control plane is
written in Rust; - cannot crash; - guarantees a particular recovery time
before it is measured; - supports a framework comprehensively because a
minimal fixture passes; - is production-ready solely because a benchmark
produces a high RPS result.

Rust ownership and borrowing reduce important classes of memory-safety
errors in Rust code, but do not prove that the complete system is free
from leaks, deadlocks, resource exhaustion, logic errors, or failures in
PHP and native extensions.

## 3. Repository findings

### 3.1 Existing strengths

The inspected repository shows: - A modular Cargo workspace with
separate crates for core, config, CLI, runtime backends, worker/server,
and platform services. - A Process Runtime-first policy. - An Embedded
Runtime explicitly treated as experimental and disabled by default. - A
technical specification describing bounded queues, backpressure, request
envelopes, runtime lifecycle, and compatibility requirements. - A
proposed ADR-0014 describing worker states, health signals, soft/hard
memory thresholds, recycling, restart protection, and deferred adaptive
scaling. - Operational documentation and scripts for quickstarts,
performance baselines, soak testing, and deployment. - A Docker image
designed to run as a non-root user. - A benchmark report that labels
developer-machine results as non-SLA and warns that Windows sequential
and WSL2 parallel runs are not directly comparable.

### 3.2 Main implementation gap: one-shot versus persistent workers

The inspected `crates/phpx-worker/src/lib.rs` uses
`execute_one_shot(...)` for each admitted request.

Consequences: - The current bounded admission mechanism can cap
concurrent executions and bound waiting requests. - Each request
receives a fresh PHP process, so process-level state does not persist
between requests in the same worker. - The current design does not yet
demonstrate a persistent PHP worker pool with individually tracked
worker identities and lifecycle states. - A logical request/recycling
counter must not be presented as proof that a persistent OS process was
recycled. - Per-worker RSS, uptime, request count, exit reason, health
state, and memory-triggered recycling require a persistent worker
identity and supervisor.

This is an architectural distinction, not automatically a defect.
One-shot execution can remain useful as a compatibility or isolation
mode. Product and documentation must distinguish it from persistent
worker mode.

### 3.3 Resource configuration is not yet the full proposed guardian

The inspected `RuntimeProcessConfig` includes worker count, minimum
workers, maximum requests, request/startup timeouts, a memory-limit
string, queue bound/wait timeout, drain timeout, and restart/backoff
settings.

Configuration alone does not prove that OS-level per-worker RSS
sampling, soft-threshold graceful recycling, hard-threshold protective
termination, or adaptive resource control are implemented.

Document each setting according to its actual enforcement point: -
PHP-level limit; - PHPX process-supervision policy; - OS/container
enforcement; - advisory/diagnostic threshold.

Do not imply that PHP `memory_limit` is equivalent to an OS process RSS
limit.

### 3.4 Embedded Runtime status

Keep Embedded Runtime experimental and disabled by default until real
PHP Embed SAPI execution, ABI handling, lifecycle constraints,
compatibility, and failure containment have been validated.

A capability stub or availability report is not equivalent to a working
embedded PHP execution backend.

### 3.5 Compatibility evidence needs explicit levels

Minimal fixtures are useful for contract and smoke tests, but are not
equivalent to comprehensive tests against real framework/application
distributions.

  -----------------------------------------------------------------------
  Level                               Evidence
  ----------------------------------- -----------------------------------
  Fixture tested                      A minimal purpose-built fixture
                                      passes.

  Framework smoke tested              An actual framework installation
                                      boots and serves basic requests.

  Integration tested                  Relevant framework lifecycle,
                                      routing, sessions, uploads, and
                                      dependencies are tested.

  Application validated               A representative real application
                                      passes a documented test suite.

  Production validated                A production-like deployment and
                                      workload have been exercised with
                                      documented limitations.
  -----------------------------------------------------------------------

Do not describe Laravel, WordPress, or WooCommerce as fully supported
beyond the strongest evidence available for the specific release.

### 3.6 Benchmark evidence is currently a developer baseline

The inspected `docs/operations/performance.md` records a small
developer-machine baseline and explicitly warns that the Windows
sequential run and WSL2 parallel run are not apples-to-apples. This is
good documentation discipline.

These results are useful for regression tracking, not broad performance
or production claims. Extend methodology before drawing conclusions
about sustained throughput, tail latency, memory efficiency, long-run
memory behavior, persistent worker stability, crash recovery, or
comparisons with alternatives.

## 4. Recommended implementation roadmap

### P0-A: Make execution modes explicit

**Goal:** prevent one-shot execution from being confused with persistent
worker supervision.

Tasks: - Name and document the current execution mode as one-shot if
that matches the actual code path. - Keep one-shot mode available where
its isolation/compatibility trade-off is useful. - Define a separate
persistent-worker execution contract. - Define worker identity and
incarnation identity. - Define request transport and protocol version
between Rust supervisor and PHP worker. - Ensure blocking PHP execution
never runs directly on Tokio executor threads. - Document which
applications can use persistent mode and how state-reset limitations are
handled.

Acceptance criteria: - CLI/configuration reports the actual selected
execution mode. - Metrics distinguish one-shot executions from
persistent worker requests. - Tests prove that one-shot process lifetime
is request-scoped. - Persistent mode is not advertised until a real
long-lived PHP process is supervised.

### P0-B: Implement deterministic persistent worker lifecycle

**Goal:** build a fixed-size supervised worker pool before adaptive
scaling.

Suggested states:

`Starting -> Ready -> Busy -> Ready`

Additional transitions: - `Ready -> Draining -> Stopped` -
`Busy -> Draining -> Stopped` after the current request -
`Busy -> Unhealthy -> Restarting -> Ready` - `Starting -> Failed`

Tasks: - Spawn a configured fixed number of PHP worker processes. -
Track PID, worker ID, incarnation, state, start time, completed request
count, and exit reason. - Implement startup handshake and readiness
timeout. - Assign no more than one request per process worker by
default. - Implement bounded queue and explicit overload behavior. -
Drain workers during reload and shutdown. - Apply restart backoff,
jitter, rolling restart budget, and crash-loop protection. - Avoid
replacing every worker simultaneously during routine recycling.

Acceptance criteria: - Worker state transitions are covered by
deterministic tests. - A worker crash does not crash the gateway. - A
replacement becomes ready only after a successful handshake. - Queue
capacity and wait time are bounded and observable. - Graceful shutdown
drains in-flight work within a configured deadline. - Repeated startup
failures stop at the configured restart budget and expose a diagnostic
reason.

### P0-C: Implement Resource Health & Memory Guardian

**Goal:** make resource management an observable control loop, not
merely a configuration promise.

Collect per worker where supported: - RSS and peak RSS; - uptime; -
completed request count; - request duration; - startup and shutdown
duration; - exit code and termination reason; - restart count and
restart frequency; - current worker state; - last successful health
check.

Define separate policies:

**Soft memory threshold** - Mark the worker for recycling. - Stop
assigning new work. - Let the current request finish within the drain
deadline. - Shut down and replace the worker.

**Hard memory threshold** - Trigger a documented protective action. - If
graceful recovery is no longer safe, terminate the worker. - Record the
reason and recovery outcome.

Important limitations: - Sampling intervals and termination latency can
cause threshold overshoot. - RSS is not the same as PHP heap usage. -
Stable RSS does not prove there is no leak. - RSS remaining high does
not automatically prove a leak. - PHP allocator behavior, native
extensions, shared libraries, and memory fragmentation affect observed
memory.

Acceptance criteria: - Metrics expose per-worker memory and lifecycle
data. - Soft-threshold recycling is tested with a controlled memory
workload. - Hard-threshold action is tested in an isolated
environment. - Threshold overshoot and sampling interval are
documented. - Worker replacement and recovery are observable. - No claim
of leak-free behavior is made solely from RSS measurements.

### P0-D: Build real compatibility tests

**Goal:** validate strict compatibility using actual PHP applications.

Start with: 1. Vanilla PHP application. 2. A real Composer-based
application. 3. A real Laravel installation. 4. A real WordPress
installation. 5. WooCommerce after WordPress coverage is reliable.

Test at minimum: - front-controller routing; - `$_SERVER`, request
method, query string, and headers; - request body and JSON; - cookies
and sessions; - file uploads; - response headers and status; -
environment variables; - Composer autoload; - error handling and output
buffering; - static assets and path safety; - shutdown behavior; -
persistent-worker state leakage, if persistent mode is enabled.

Publish a version/extension matrix per release. Mark unsupported
extensions and runtime limitations explicitly.

Acceptance criteria: - Tests run against real upstream
framework/application packages. - Each compatibility claim links to
relevant test evidence. - Fixtures and real application tests are
reported separately. - No source modification is required for the
conventional compatibility baseline, unless a limitation is clearly
documented.

### P1-A: Resilience and soak testing

Add repeatable tests for: - worker crash while idle; - worker crash
while processing a request; - request timeout; - startup failure; -
restart-loop protection; - graceful reload; - graceful shutdown with
in-flight requests; - queue saturation and overload response; -
memory-threshold recycling; - long-running memory trend; - state leakage
between requests.

Run short and long soak tests. Archive raw logs, environment metadata,
configuration, and result summaries.

Measure recovery time from clearly defined events: - failure detection
timestamp; - restart initiated timestamp; - replacement ready
timestamp; - first successful request timestamp.

Report these separately rather than using one ambiguous "recovery time".

### P1-B: Benchmark laboratory

Keep performance, resource efficiency, stability, and reliability as
separate dimensions.

  Dimension             Suggested measurements
  --------------------- -------------------------------------------------
  Throughput            Requests per second
  Latency               p50, p95, p99, maximum
  Cold start            First request and process startup
  Warm path             Steady-state request latency
  Resource efficiency   Idle/peak RSS, CPU, requests per MB
  Memory stability      RSS trend over 1h, 6h, 24h
  Reliability           Failure rate, crash frequency, recovery success
  Operations            Drain time, restart time, overload behavior

Comparison rules: - Same hardware and operating system. - Same PHP
version and extensions. - Same application and datastore. - Same request
mix and payload. - Same concurrency and duration. - Equivalent tuning
and worker limits. - Release builds for performance claims. - Report raw
measurements and configuration.

Compare PHPX with relevant alternatives only when the setup is fair and
reproducible. Do not invent absolute RPS targets before establishing
representative workloads.

### P2: Adaptive worker management

Defer dynamic scaling until fixed-pool behavior is stable.

Potential signals: - queue depth and queue wait time; - active/ready
worker counts; - CPU pressure; - memory headroom.

Required safeguards: - minimum and maximum workers; - scale-up and
scale-down thresholds; - hysteresis; - cooldown; - host-level memory
safeguards; - minimum ready capacity.

Adaptive mode should remain disabled by default until repeatable
evidence shows that it improves outcomes without causing oscillation or
resource pressure.

### P2: Embedded Runtime

Keep Embedded Runtime experimental and disabled by default.

Promotion gates: - real PHP Embed SAPI execution; - supported PHP build
and ABI matrix; - controlled execution domain and thread constraints; -
clear ownership/lifetime rules across the C ABI; - extension
compatibility tests; - request state isolation tests; - failure
containment strategy; - parity on the compatibility suite for every
claimed capability.

Do not promote it based on throughput alone.

## 5. Suggested metrics and terminology

Use precise names and avoid ambiguous aggregate counters.

### Worker metrics

-   `phpx_worker_state`
-   `phpx_worker_rss_bytes`
-   `phpx_worker_peak_rss_bytes`
-   `phpx_worker_uptime_seconds`
-   `phpx_worker_requests_total`
-   `phpx_worker_restarts_total{reason=...}`
-   `phpx_worker_startup_duration_seconds`
-   `phpx_worker_shutdown_duration_seconds`

### Queue and gateway metrics

-   `phpx_requests_total{outcome=...}`
-   `phpx_request_duration_seconds`
-   `phpx_queue_depth`
-   `phpx_queue_wait_duration_seconds`
-   `phpx_overload_rejections_total`
-   `phpx_worker_capacity`
-   `phpx_worker_active`

Metric names are proposals. Align them with repository naming
conventions before implementation. Avoid high-cardinality labels such as
request IDs, raw URLs, or user identifiers.

### Recovery measurements

Record: - detection latency; - restart scheduling delay; - worker
startup duration; - readiness delay; - time to first successful
request; - total recovery duration.

## 6. Documentation corrections

Update documentation so implemented behavior, tested behavior,
experimental behavior, and planned behavior are visibly distinct.

Recommended capability labels: - **Implemented:** code path exists. -
**Tested:** automated or documented test evidence exists. -
**Experimental:** available for evaluation, not a stable contract. -
**Stub:** interface or placeholder exists, but real execution is
absent. - **Deferred:** intentionally postponed. - **Unsupported:**
explicitly not supported.

Specific documentation work: - README: summarize actual v0.1 capability
and limitations. - Technical specification: distinguish target
architecture from current implementation. - ADR-0014: retain as proposed
until accepted; link implementation PRs/tests when decisions are
implemented. - Performance report: preserve current developer baseline
and add future release-mode/soak results separately. - Compatibility
matrix: separate fixtures from real framework/application validation. -
CLI help: report actual runtime capabilities, not aspirational
capabilities.

## 7. Release gates

Do not promote a release merely because the planned feature list is
complete.

### Developer preview

-   Basic gateway and execution path works.
-   Known limitations are documented.
-   No production SLA implied.

### Alpha

-   Fixed worker lifecycle is deterministic.
-   Bounded queue and backpressure are tested.
-   Worker crash and restart behavior are observable.
-   Core compatibility tests pass.

### Beta

-   Resource guardian is implemented and tested.
-   Real Laravel and WordPress smoke/integration tests exist.
-   Soak and failure-injection tests are repeatable.
-   Security and deployment limitations are documented.

### Release candidate

-   Release-mode benchmarks are reproducible.
-   Recovery and memory behavior meet documented acceptance criteria.
-   Compatibility matrix is published.
-   No unresolved critical security or data-integrity issues.

### Stable 1.0

-   All published 1.0 claims have test evidence.
-   Upgrade, rollback, and operational procedures are documented.
-   Supported PHP/OS/extension matrix is explicit.
-   Release artifacts are reproducible and verified.

## 8. Immediate next actions

Recommended order for the next engineering cycle:

1.  Audit and label the current one-shot execution path accurately.
2.  Write the persistent worker protocol and lifecycle contract.
3.  Implement fixed-size persistent worker supervision.
4.  Add worker identity, state transitions, and lifecycle metrics.
5.  Implement per-worker RSS sampling and soft/hard memory policy.
6.  Add deterministic crash, drain, restart-budget, and memory-threshold
    tests.
7.  Add real Laravel and WordPress smoke tests.
8.  Expand soak and release-mode benchmark evidence.
9.  Revisit adaptive scaling only after the fixed-pool baseline is
    stable.

## 9. Reviewer conclusion

PHPX has a coherent product direction and useful architectural
groundwork. The most important engineering risk is not the choice of
Rust or PHP; it is whether the runtime actually implements and proves
the worker lifecycle, resource monitoring, recycling, compatibility, and
recovery behavior that the product intends to offer.

The next milestone should prioritize **measurable runtime behavior over
feature count**.

> Build the control loop. Measure the resources. Inject failures. Prove
> recovery. Then make the claims.
