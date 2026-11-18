# PHPX Architecture Decision Records

**Project:** PHPX\
**Document version:** 0.1.0\
**Status:** Accepted baseline, subject to implementation validation\
**Last updated:** 2026-11-17 (ADR-0014 added as Proposed, not yet Accepted)

This directory records architectural decisions that materially shape
PHPX. An ADR is immutable after acceptance: superseding a decision
requires a new ADR that references the earlier one.

## ADR-0001: Product identity and scope

-   **Status:** Accepted
-   **Decision:** The product is named **PHPX** and is a Rust-powered
    PHP application platform, not merely an HTTP server or a PHP process
    manager.
-   **Context:** A server-only scope would leave queue processing,
    scheduling, runtime lifecycle, storage integration, and operational
    tooling fragmented.
-   **Consequences:**
    -   PHPX provides a unified developer-facing CLI and configuration
        model.
    -   The platform is composed of independently testable subsystems.
    -   The first release prioritizes a reliable HTTP + PHP execution
        path; other services are added incrementally.
    -   The name PHPX is a working product identity and must undergo
        repository, package, domain, and trademark checks before
        commercial release.

## ADR-0002: Rust is the platform control plane

-   **Status:** Accepted
-   **Decision:** Rust owns platform infrastructure: gateway, lifecycle
    management, worker supervision, scheduling, queue coordination,
    resource controls, observability, and CLI.
-   **Context:** Rust offers predictable resource control and strong
    systems-programming capabilities. PHP remains the application
    language and execution engine.
-   **Consequences:**
    -   PHP application code is not rewritten in Rust.
    -   Platform-to-runtime boundaries are explicit and versioned.
    -   Async orchestration must not execute blocking PHP work directly
        on Tokio executor threads.
    -   Rust subsystem failure handling and graceful shutdown are
        first-class design requirements.

## ADR-0003: Hybrid PHP runtime architecture

-   **Status:** Accepted
-   **Decision:** PHPX supports two runtime backends behind a common
    runtime abstraction:
    1.  **Process Runtime:** PHP executes in separately supervised OS
        processes. This is the default and production baseline.
    2.  **Embedded Runtime:** PHP Embed SAPI is hosted through a
        carefully controlled native bridge. This remains experimental
        until it passes compatibility, isolation, and stability gates.
-   **Context:** Process execution provides a stronger compatibility and
    fault-isolation boundary. Embedding may reduce boundary overhead and
    simplify some deployment shapes, but introduces ABI, lifecycle,
    thread-safety, extension, and crash-containment complexity.
-   **Consequences:**
    -   Runtime selection is explicit in configuration; no silent
        backend switching.
    -   Both backends implement the same platform-facing lifecycle and
        execution contracts where semantics genuinely match.
    -   Backend-specific capabilities are advertised, not assumed.
    -   Embedded execution must use controlled execution domains. PHP
        NTS must never be concurrently entered from arbitrary Tokio
        threads.
    -   A process boundary remains available for untrusted or
        crash-sensitive workloads.
    -   The embedded backend cannot become the default merely because a
        microbenchmark is faster.

## ADR-0004: Strict PHP application compatibility

-   **Status:** Accepted
-   **Decision:** PHPX targets unmodified, conventional PHP
    applications. Platform-specific APIs are optional enhancements and
    never required for baseline execution.
-   **Context:** The platform must be useful to the existing PHP
    ecosystem, especially Laravel, WordPress, and WooCommerce.
-   **Consequences:**
    -   PHPX must preserve normal PHP request semantics, environment
        variables, document-root/front-controller behavior, headers,
        cookies, request bodies, status codes, and error behavior.
    -   Compatibility is verified using real application fixtures and
        extension matrices.
    -   PHPX does not promise compatibility with every extension or
        every PHP SAPI behavior in v0.1.
    -   Unsupported behavior must be documented precisely; PHPX must
        fail clearly rather than silently alter application semantics.
    -   Optional PHPX APIs require versioning and cannot be
        prerequisites for ordinary application hosting.

## ADR-0005: Docker and native Linux are first-class deployment targets

-   **Status:** Accepted
-   **Decision:** PHPX supports both native Linux installation and
    containerized deployment. Neither is a second-class afterthought.
-   **Context:** Native deployments are useful for direct host
    integration and low-overhead installations. Containers are central
    to repeatable deployments and orchestration.
-   **Consequences:**
    -   The same core binary and configuration model should work in both
        environments.
    -   Filesystem paths, signals, permissions, cgroups, and process
        supervision must be tested in both.
    -   Docker images must support non-root execution by default where
        feasible.
    -   Native packaging and service integration (for example systemd)
        are documented separately from container orchestration.
    -   Windows is not a v0.1 production target; Windows development may
        use WSL2 or containers.

## ADR-0006: Tokio orchestration, isolated blocking execution

-   **Status:** Accepted
-   **Decision:** Tokio is used for asynchronous platform I/O and
    orchestration. PHP execution and other blocking operations run
    outside Tokio worker threads.
-   **Context:** PHP execution is not an async Tokio future and can
    block for arbitrary durations.
-   **Consequences:**
    -   Process workers communicate over bounded IPC channels.
    -   Embedded PHP, if used, runs in dedicated controlled threads or
        execution domains with serialized access according to the PHP
        build and SAPI constraints.
    -   Backpressure and bounded queues are mandatory.
    -   Cancellation of an HTTP request does not imply unsafe
        interruption of an executing PHP interpreter; timeout handling
        is backend-specific and documented.

## ADR-0007: HTTP gateway implementation

-   **Status:** Accepted
-   **Decision:** Use Hyper with Tokio for the initial HTTP gateway, and
    Rustls for TLS.
-   **Context:** These components provide a modular Rust HTTP foundation
    without requiring PHP to own the network listener.
-   **Consequences:**
    -   HTTP/1.1 is mandatory for v0.1; HTTP/2 is a target subject to
        integration testing.
    -   HTTP/3 is out of scope for v0.1.
    -   Static file serving, request limits, graceful shutdown, and PHP
        front-controller dispatch are part of the gateway.
    -   Reverse proxy and advanced edge features are deferred.

## ADR-0008: Configuration format and compatibility

-   **Status:** Accepted
-   **Decision:** Use TOML for the primary human-authored configuration
    file, with Serde-based typed deserialization and explicit schema
    validation.
-   **Context:** Operators need a readable, reviewable configuration
    format.
-   **Consequences:**
    -   Configuration has an explicit schema version.
    -   Unknown or invalid critical values fail fast with actionable
        errors.
    -   Defaults are documented and stable.
    -   Environment-variable overrides use a documented naming
        convention.
    -   Secrets should be supplied through environment variables or
        secret mounts, not committed to TOML files.

## ADR-0009: Queue durability and storage adapters

-   **Status:** Accepted
-   **Decision:** Queue and storage are pluggable interfaces. SQLite is
    the initial durable single-node queue backend; Redis is an optional
    adapter. In-memory queue is development-only.
-   **Context:** A platform queue must not lose acknowledged jobs on
    process restart.
-   **Consequences:**
    -   Job state transitions, retries, leases/visibility timeouts, and
        dead-letter handling require transactional semantics.
    -   The in-memory driver is explicitly marked non-durable.
    -   PostgreSQL and other distributed backends are deferred.
    -   Filesystem and S3-compatible storage adapters are introduced
        incrementally.

## ADR-0010: Observability is built in

-   **Status:** Accepted
-   **Decision:** Structured logs, request correlation, metrics, and
    tracing are platform capabilities, not optional afterthoughts.
-   **Context:** A runtime platform must be diagnosable under real
    production load.
-   **Consequences:**
    -   Use the Rust `tracing` ecosystem.
    -   Expose Prometheus-compatible metrics.
    -   Add OpenTelemetry integration incrementally.
    -   Never log secrets, authorization headers, or sensitive request
        payloads by default.
    -   Runtime and worker metrics must be attributable to an
        application and worker.

## ADR-0011: Security and resource isolation

-   **Status:** Accepted
-   **Decision:** PHPX applies explicit resource limits and
    least-privilege execution. A PHP worker must not inherit unnecessary
    privileges from the platform supervisor.
-   **Context:** PHP applications and extensions execute arbitrary
    application code and may be compromised.
-   **Consequences:**
    -   Configure execution timeout, memory limit, worker count, request
        size, and queue limits.
    -   Support non-root operation.
    -   Separate supervisor privileges from application worker
        privileges where practical.
    -   Document that PHP `memory_limit` is not a complete OS-level
        memory boundary.
    -   Container/cgroup limits and OS sandboxing are recommended for
        untrusted tenants.
    -   Multi-tenant hostile-code isolation is not claimed by v0.1.

## ADR-0012: Modular Rust workspace and feature-gated subsystems

-   **Status:** Accepted
-   **Decision:** Use a Cargo workspace with small crates organized
    around stable subsystem boundaries. Optional platform features are
    enabled through Cargo features and runtime configuration where
    appropriate.
-   **Context:** A monolithic crate would make testing and ownership
    difficult; excessive fragmentation too early would slow development.
-   **Consequences:**
    -   Start with a small set of foundational crates and split only
        when boundaries are clear.
    -   Avoid cyclic dependencies.
    -   Keep runtime contracts in a low-level crate.
    -   Optional adapters must not be mandatory dependencies of the
        minimal server.

## ADR-0013: Release and compatibility policy

-   **Status:** Accepted
-   **Decision:** PHPX follows semantic versioning after its first
    stable release. Before 1.0, configuration and CLI may evolve, but
    breaking changes must be documented.
-   **Context:** Operators need predictable upgrades and developers need
    to understand compatibility guarantees.
-   **Consequences:**
    -   v0.1 is an experimental developer preview, not a
        production-support promise.
    -   Every release publishes supported Rust, PHP, OS, and extension
        matrices.
    -   Runtime backend capability differences are documented.
    -   Compatibility tests are release gates.

## ADR-0014: Worker lifecycle, resource management and adaptive recycling (Proposed)

-   **Status:** Proposed (not yet part of Accepted baseline)
-   **Decision:** Add Runtime Health and Memory Guardian to Process
    Runtime. Canonical lifecycle is `Starting -> Ready -> Busy -> Ready`,
    with `Ready/Busy -> Draining -> Stopped`,
    `Busy -> Unhealthy -> Restarting -> Ready`, and
    `Starting -> Failed`. `Failed` is terminal for that worker
    incarnation with operator-visible diagnostics. See
    `adr/0014-worker-lifecycle-resource-management.md` for full rationale
    (Sundanese body + normative English abstract).
-   **Context:** Persistent PHP workers risk cross-request state leakage
    and memory growth. Resource control is the realistic PHPX
    differentiator, not automatic speedup.
-   **Consequences:**
    -   Bounded pool + bounded queue + backpressure remain mandatory.
        Default is one concurrent request per process worker.
    -   Soft memory threshold triggers graceful recycle; hard threshold
        allows protective termination. Overshoot is possible due to
        sampling/termination latency and must be documented.
    -   Restart uses exponential backoff + jitter + restart budget +
        circuit breaker to avoid infinite respawn loops.
    -   Detailed lifecycle/memory/adaptive keys live under
        `[runtime.process.lifecycle]`, `[runtime.process.memory]`, and
        `[runtime.process.adaptive]` (adaptive `enabled = false` by
        default). SPEC Section 8 example is simplified and points here
        for the full model.
    -   Benchmarks for this work compare against FrankenPHP, RoadRunner,
        Swoole/OpenSwoole, and Nginx + PHP-FPM on identical builds with
        p50/p95/p99/max, RSS, restarts, and recovery metrics. No parity
        claim without repeatable results.
    -   Until accepted, SPEC Section 5 remains normative for v0.1
        baseline; ADR-0014 refines it.

## Deferred decisions

The following are intentionally not fixed in v0.1:

-   Final embedded PHP ABI strategy and supported PHP build variants.
-   Whether PHP-CGI, PHP CLI worker protocol, or FastCGI is the
    preferred process transport in every deployment.
-   Distributed queue semantics and PostgreSQL driver.
-   Windows native support.
-   HTTP/3.
-   Multi-tenant hostile-code sandbox guarantees.
-   Plugin ABI stability.

These decisions require prototypes and evidence rather than assumptions.

