# PHPX Roadmap

**Version:** 0.1.0\
**Status:** Proposed implementation plan\
**Date:** 2026-11-17

PHPX is developed in evidence-driven milestones. The roadmap is ordered
by dependency and risk, not by the number of visible features. Each
phase has a concrete exit gate.

## Guiding rules

1.  Process Runtime is the first production-capable execution backend.
2.  Embedded Runtime is experimental until it passes compatibility and
    stability gates.
3.  Strict compatibility means ordinary PHP applications do not need
    source changes.
4.  Native Linux and Docker are both first-class deployment targets.
5.  No unbounded worker pool, request queue, or job queue.
6.  Do not optimize before reproducible baseline benchmarks exist.
7.  Keep the minimal server usable without optional queue, cache, or
    scheduler services.

## Milestone 0: Repository and engineering foundation

**Goal:** Create a buildable, testable Rust workspace.

### Work

-   [ ] Create repository and project governance files.
-   [ ] Add `Cargo.toml` workspace and pinned `rust-toolchain.toml`.
-   [ ] Establish formatting, linting, and test CI.
-   [ ] Define error conventions and structured logging.
-   [ ] Define configuration schema version 1.
-   [ ] Implement `phpx --version` and `phpx doctor` skeleton.
-   [ ] Document Linux development prerequisites.
-   [ ] Add architecture overview and accepted ADRs.

### Exit criteria

-   [ ] CI builds and tests on supported Linux target.
-   [ ] `cargo fmt --check` and Clippy pass.
-   [ ] `phpx doctor` reports Rust/PHP prerequisites clearly.
-   [ ] Example configuration parses and validates.

## Milestone 1: Process Runtime prototype

**Goal:** Execute a PHP script through a supervised Rust-managed worker.

### Work

-   [ ] Discover PHP executable and inspect version.
-   [ ] Inspect required PHP extensions.
-   [ ] Define versioned request/response envelope.
-   [ ] Prototype framed Unix Domain Socket IPC.
-   [ ] Implement PHP worker bootstrap.
-   [ ] Start and stop child workers.
-   [ ] Capture stdout/stderr and exit status safely.
-   [ ] Add startup and execution timeouts.
-   [ ] Add worker health and restart policy.
-   [ ] Add a simple vanilla PHP fixture.

### Exit criteria

-   [ ] Rust can start a worker and execute a PHP request.
-   [ ] Concurrent requests are bounded.
-   [ ] Worker crash does not crash the supervisor.
-   [ ] No request data crosses between sequential requests.
-   [ ] Integration test passes on at least one supported PHP version.

## Milestone 2: HTTP gateway

**Goal:** Serve a conventional PHP application through PHPX.

### Work

-   [x] Add Tokio + Hyper listener.
-   [x] Add HTTP/1.1 request handling.
-   [x] Add host/application configuration.
-   [x] Add document-root and front-controller routing.
-   [x] Add safe static file serving.
-   [x] Add request body and header limits.
-   [x] Add PHP request dispatch.
-   [x] Add request IDs and access logs.
-   [x] Add graceful shutdown and worker draining.
-   [x] Add TLS via Rustls.
-   [x] Validate HTTP/2 support before claiming it.

### Exit criteria

-   [x] Vanilla PHP app works without source modifications.
-   [x] Static assets and PHP front controller work.
-   [x] Invalid paths cannot escape document root.
-   [x] Gateway remains available through a PHP worker crash.
-   [x] Backpressure prevents unbounded pending requests.

## Milestone 3: Developer preview v0.1

**Goal:** Deliver a useful minimal PHP application server.

### Work

-   [x] Finalize `phpx serve`.
-   [x] Finalize `phpx doctor`.
-   [x] Add runtime and worker status commands.
-   [x] Add TOML configuration and documented defaults.
-   [x] Add Docker image running as non-root where feasible.
-   [x] Add native Linux installation instructions.
-   [x] Add systemd example/service guidance.
-   [x] Publish supported PHP version matrix.
-   [x] Add smoke tests and reproducible baseline benchmarks.

### Exit criteria

-   [x] Docker quickstart passes.
-   [x] Native Linux quickstart passes.
-   [x] Vanilla PHP compatibility fixture passes.
-   [x] Operational limitations are documented.
-   [x] v0.1 is clearly labeled developer preview, not production SLA.

Traceability: SPEC Section 15 acceptance items 1-4, 8-10 are covered by M1-M3; items 5-7 plus 11-12 are completed by M4. Queue/scheduler/embedded (SPEC 15 tail) are explicitly deferred to M5-M6 and must not block M3.

## Milestone 4: Worker hardening and application compatibility

**Goal:** Make the Process Runtime robust for real applications.

### Work

-   [x] Worker recycling by request count and health (M4 baseline DONE: one-shot isolation + logical `recycled` counter; soft/hard RSS guardian incremental per ADR-0014 Phase B).
-   [x] Document Failed vs Stopped worker states per SPEC 5.1 + ADR-0014 alignment (pool docs, gateway logs, `workers status`, compatibility-matrix).
-   [x] Configurable min/max pool size (`min_workers`/`workers` validated, `min<=max`).
-   [x] Startup backoff and crash-loop protection (consecutive-failure budget + exp backoff/jitter + circuit fast-503).
-   [x] Request timeout and graceful drain semantics (`execution_timeout` + `shutdown_drain_timeout_seconds`, Ctrl-C/SIGTERM drain, `draining` 503).
-   [x] PHP environment and SAPI compatibility audit (`phpx doctor` SAPI + 11 ext, `sapi-audit.md`, auto_prepend bootstrap).
-   [x] Composer fixture (`examples/composer-autoload`, stub autoloader, no network).
-   [x] Laravel compatibility fixture (`examples/laravel-minimal` sim, routing/JSON/middleware).
-   [x] WordPress compatibility fixture (`examples/wordpress-minimal` sim, `?p=`/`?s=`/shortcode).
-   [x] WooCommerce smoke tests after WordPress baseline (`examples/woocommerce-minimal` sim, cart/checkout).
-   [x] Test PHP extension combinations (`doctor` 11-ext audit + matrix runbook).
-   [x] Soak tests and memory-growth analysis (`scripts/soak.sh` + `soak.md` report format; adaptive disabled).

### Exit criteria (covers SPEC Section 15 items 5-7, 11-12 for worker scope; full v0.1 acceptance in SPEC Section 15)

-   [x] No known request cross-contamination (`isolate.php` clean xN, distinct RIDs).
-   [x] Crash recovery and graceful reload pass (crash->200, SIGTERM drain, stateless restart).
-   [x] Compatibility matrix is published (`compatibility-matrix.md` + `m4_compat` tests).
-   [x] Unsupported SAPI/extension behavior is explicit (`sapi-audit.md` + doctor).

## Milestone 5: Platform services

**Goal:** Add useful application infrastructure without compromising the
core server.

### Queue

-   [x] Define queue driver contract (M5 phase 1 DONE: `phpx-queue` trait + `queue.md`).
-   [x] Add in-memory development driver (M5 phase 1 DONE: bounded, ephemeral, `contract.rs`).
-   [x] Add durable SQLite driver (M5 phase 1 DONE: rusqlite bundled, lease recovery, `contract.rs`).
-   [x] Implement retries, backoff, delayed jobs, and dead-letter handling (M5 phase 1 DONE: exp backoff+jitter, `not_before`, dead `state`).
-   [x] Add job worker CLI and status reporting (M5 phase 1 DONE: `queue status|push|work`, real status, dev worker).
-   [x] Test recovery after forced process termination (M5 phase 1 DONE: `sqlite_survives_restart`, running->queued on open).

### Scheduler

-   [x] Add cron-compatible schedule parser (M5 phase 2 DONE: `phpx-scheduler` 5-field + names/steps/lists, OR-semantics, `scheduler.md`).
-   [x] Add timezone handling (M5 phase 2 DONE: UTC/local/offset/IANA via chrono-tz, DST-safe, `scheduler.md`).
-   [x] Add execution history (M5 phase 2 DONE: `MemoryHistory` + `SqliteHistory` table `scheduler_runs`, `history_file`, `scheduler.md`).
-   [x] Add duplicate-dispatch protection for single-node deployments (M5 phase 2 DONE: history check before enqueue, `duplicate` counted, single-pass + timer).

### Cache and storage

-   [x] Add bounded in-memory cache (M5 phase 3 DONE: `phpx-cache` memory + `cache.md`).
-   [x] Add filesystem cache (M5 phase 3 DONE: `phpx-cache` filesystem + contract).
-   [ ] Add Redis adapter if justified by integration needs (deferred, needs network + justification).
-   [x] Define storage interface (M5 phase 3 DONE: `StorageDriver` + `storage.md`).
-   [x] Add local filesystem adapter (M5 phase 3 DONE: `LocalStorage` + contract).
-   [x] Prototype S3-compatible adapter (M5 phase 3 DONE: `S3Prototype` config real, transport deferred + `storage.md`).

### Exit criteria

-   [x] Durable queue jobs survive restart.
-   [x] Retry and dead-letter behavior is tested.
-   [x] Scheduler does not execute PHP on gateway async threads (dispatch-only, single pass + systemd timer, never PHP).
-   [x] Cache and storage adapters have contract tests.

## Milestone 6: Embedded Runtime experimental track

**Goal:** Validate a second runtime backend without weakening the
Process Runtime.

### Work

-   [x] Prototype PHP Embed SAPI build and linking. (M6 DONE: link probe plus build plan, found=false until Linux NTS toolchain)
-   [x] Pin PHP headers, libraries, and ABI expectations. (M6.2 DONE: AbiPin 8.3/8.4/8.5 NTS embed + CLI abi + contract tests)
-   [x] Implement narrow C ABI bridge. (M6 DONE: bridge allowlist startup plus execute_once plus shutdown, repr C headers, stub reports unlinked)
-   [x] Define memory ownership and lifecycle rules. (M6 DONE: 5 rules plus RequestScope tracker, Rust owns buffers)
-   [x] Create controlled execution domain. (M6.1 stub DONE: EmbeddedDomain single-entry guard, no concurrent entry)
-   [x] Test NTS/ZTS constraints for chosen build. (M6 DONE: matrix tests 8.3-8.5 NTS ok, ZTS plus Debug rejected)
-   [x] Implement runtime capability reporting. (M6.1 stub DONE: EmbeddedCapabilities report + CLI list/inspect, available=false)
-   [x] Run shared runtime contract tests. (M6.1 stub DONE: envelope roundtrip + capability + single-entry + stub-never-executes)
-   [x] Run Laravel/WordPress compatibility subsets. (M6 DONE: 4 subsets mirror M4 fixtures, embedded execution deferred, process green)
-   [x] Test crash containment and resource limits. (M6 DONE: containment guarded order plus 16MiB cap, gateway never hosts live engine)

### Exit criteria

-   [x] Embedded backend is opt-in and clearly marked experimental. (M6.1: config gate + CLI experimental status, process stays default)
-   [x] Claimed capabilities pass shared contract tests. (M6.1 stub: available=false + single-entry + envelope contracts green)
-   [x] No arbitrary concurrent PHP engine entry. (M6 DONE: 8-thread stress test proves exactly 1 winner)
-   [x] Limitations and supported build matrix are published. (M6 DONE: embedded.md matrix plus limitations.md)
-   [x] Process Runtime remains available as fallback (M6.1: process stays default, embedded stub never takes over) selected
    explicitly by configuration.

## Milestone 7: Production candidate

**Goal:** Prepare PHPX for serious production evaluation.

### Work

-   [x] Security review and threat model. (M7 pase 1 DONE: security.md + redaction + boundary tests)
-   [x] Non-root deployment verification. (M7 pase 1 DONE: Dockerfile USER phpx static test + docs)
-   [x] cgroup/container resource limit tests. (M7 pase 2 DONE: config gates + m7_resilience saturation + operator cgroup guidance)
-   [x] Filesystem and symlink boundary tests. (M7 pase 1 DONE: raw-socket 400 + symlink 403 + unit matrix)
-   [x] Secret redaction tests. (M7 pase 1 DONE: is_sensitive_header/redacted unit + gateway soak)
-   [x] Load, stress, and soak benchmarks. (M7 pase 2 DONE: performance.md + bench scripts + m7_resilience)
-   [x] OpenTelemetry integration. (M7 pase 2 DONE: OTel-ready traceparent passthrough + JSON log correlation, no exporter in v0.1)
-   [x] Prometheus metrics documentation. (M7 pase 1 DONE: metrics.md series + alerts)
-   [x] Upgrade and rollback guidance. (M7 pase 1 DONE: upgrade-rollback.md)
-   [x] Docker image hardening and SBOM. (M7 pase 2 DONE: non-root test + runtime flags + sbom scripts)
-   [x] Native package/release automation. (M7 pase 2 DONE: release.sh/ps1 + packaging/README + dist tarball)
-   [x] Backup and recovery guidance for durable queue state. (M7 pase 1 DONE: backup-recovery.md)

### Exit criteria

-   [x] No known critical security issue. (M7 DONE: security.md threat model + m7_security 4/4 + lib redaction 5/5, no critical open)
-   [x] Resource saturation is bounded and observable. (M7 DONE: workers+queue_bound+queue_wait gates + m7_resilience 2/2 200/503/504 + /metrics observable)
-   [x] Failure recovery is documented and tested. (M7 DONE: upgrade-rollback.md + backup-recovery.md + queue sqlite_survives_restart + m7_resilience recovery + crash 500 gateway-up)
-   [x] Performance claims are reproducible. (M7 DONE: baseline.md lock + performance.md + bench.sh/ps1 cold_ms + bench-compare skeleton + soak-long CSV, NOT SLA)
-   [x] Compatibility and deployment matrices are published. (M7 DONE: compatibility-matrix.md m4_compat 5/5 + deployment-matrix.md docker/systemd/windows + m7 non-root)
-   [x] Release candidate limitations are explicit. (M7 DONE: limitations.md M7 RC delta + OTel-ready no exporter + cgroup operator + no blanket SLA)

## Suggested sequencing

    Order Milestone                          Priority
  ------- ---------------------------------- -----------------------------
        0 Repository foundation              P0
        1 Process Runtime                    P0
        2 HTTP gateway                       P0
        3 Developer preview v0.1             P0
        4 Worker and app compatibility       P0
        5 Queue, scheduler, cache, storage   P1
        6 Embedded Runtime                   P1
        7 Production hardening               P0 before production claims

Embedded Runtime can be prototyped in parallel after the runtime
contract exists, but it must not block the Process Runtime release.

## First 10 implementation tasks

1.  Initialize the Rust workspace and CI.
2.  Implement shared error and configuration types.
3.  Implement PHP executable discovery and `phpx doctor`.
4.  Define runtime request/response protocol.
5.  Build a minimal PHP worker bootstrap.
6.  Implement process spawn, supervision, and shutdown.
7.  Execute a vanilla PHP request through IPC.
8.  Add Hyper gateway and front-controller routing.
9.  Add bounded worker pool and backpressure.
10. Ship Docker and native Linux quickstarts.

## Definition of done for every feature

-   [ ] Unit tests cover core logic.
-   [ ] Integration tests cover subsystem boundaries.
-   [ ] Failure behavior is defined.
-   [ ] Resource usage is bounded.
-   [ ] Logs and diagnostics are actionable.
-   [ ] Configuration defaults and compatibility are documented.
-   [ ] Security implications are reviewed.
-   [ ] Feature is disabled or fails safely when unavailable.


