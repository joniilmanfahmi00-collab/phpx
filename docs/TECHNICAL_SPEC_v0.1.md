# PHPX Technical Specification v0.1

**Status:** Baseline specification\
**Version:** 0.1.0\
**Date:** 2026-11-17\
**Product:** PHPX

## 1. Summary

PHPX is a Rust-powered PHP application platform. It provides an HTTP
gateway, PHP runtime management, supervised workers, and a path toward
integrated queues, scheduling, cache, storage, and observability.

PHPX does not replace PHP as an application language. It runs existing
PHP applications with a strict compatibility goal and provides optional
platform capabilities.

### 1.1 Product goals

1.  Run conventional PHP applications without source-code changes.
2.  Provide one operational interface for server and PHP worker
    lifecycle.
3.  Support both native Linux and Docker deployments.
4.  Support process and embedded PHP runtime backends behind a common
    abstraction.
5.  Provide safe defaults, bounded resource use, and actionable
    diagnostics.
6.  Grow from a reliable HTTP/PHP execution core into a complete
    application platform.

### 1.2 Non-goals for v0.1

-   Reimplementing the PHP language or Zend Engine.
-   Replacing Composer or PHP extensions.
-   Claiming universal compatibility with every PHP SAPI or extension.
-   Native Windows production support.
-   HTTP/3.
-   Guaranteed hostile multi-tenant code isolation.
-   Replacing Redis, PostgreSQL, or object storage.
-   Making embedded runtime the default production backend.

## 2. Supported platform baseline

### 2.1 Operating systems

-   Linux is the primary and supported v0.1 host.
-   Docker deployment targets Linux containers.
-   Windows development may use WSL2 or Docker Desktop.
-   macOS may be used for development where dependencies permit, but is
    not a v0.1 production support commitment.

### 2.2 PHP versions

Initial compatibility targets are PHP 8.3, 8.4, and 8.5, subject to
availability of supported upstream builds, ABI compatibility, and CI
coverage. The supported matrix must be published per release.

The Process Runtime is the baseline compatibility path. Embedded Runtime
support may initially cover a narrower matrix.

### 2.3 Rust

Use a pinned stable Rust toolchain in `rust-toolchain.toml`. Minimum
supported Rust version (MSRV) is to be established after dependency
selection and CI validation, then recorded in release metadata.

## 3. System architecture

### 3.1 Logical components

-   **CLI:** command parsing, diagnostics, runtime and platform
    operations.
-   **Core:** shared types, application identity, errors, lifecycle
    contracts.
-   **Config:** TOML loading, environment overrides, validation,
    defaults.
-   **Gateway:** listeners, TLS, HTTP protocol handling, static assets.
-   **Router:** host/path dispatch and PHP front-controller mapping.
-   **Runtime Manager:** selects and supervises runtime backends.
-   **Process Runtime:** starts and communicates with PHP child
    processes.
-   **Embedded Runtime:** experimental PHP Embed SAPI bridge.
-   **Worker Manager:** pool sizing, readiness, health, recycling,
    draining.
-   **Queue:** job lifecycle and backend adapters.
-   **Scheduler:** recurring and delayed task dispatch.
-   **Cache:** local cache and external cache adapters.
-   **Storage:** local and S3-compatible object storage adapters.
-   **Observability:** logs, metrics, traces, health endpoints.
-   **Security:** permissions, limits, secret handling, isolation
    policy.

### 3.2 Request lifecycle

1.  Accept a connection at the Rust gateway.
2.  Parse and validate the HTTP request.
3.  Resolve the application and route.
4.  Serve eligible static assets directly.
5.  For dynamic requests, construct a bounded execution envelope.
6.  Acquire an available PHP worker for the configured runtime.
7.  Forward request metadata and body through the selected transport.
8.  Execute the application entry point.
9.  Return status, headers, and response body to the gateway.
10. Release or recycle the worker according to health and policy.
11. Emit correlated request and runtime telemetry.

The gateway must apply backpressure when no worker capacity is
available. It must not create an unbounded queue of pending requests.

### 3.3 Request envelope

The runtime boundary must define a versioned request envelope
containing, at minimum:

-   protocol version;
-   request ID and trace context;
-   method;
-   scheme;
-   host;
-   path and query string;
-   normalized headers;
-   remote address metadata where available;
-   content length and body stream or bounded body payload;
-   application root and entry point;
-   deadline/timeout metadata;
-   environment profile identifier.

The response envelope contains:

-   request ID;
-   status code;
-   response headers;
-   response body stream or bounded payload;
-   execution duration;
-   worker outcome and diagnostic classification.

Header normalization must preserve application-visible semantics and
must not permit hop-by-hop header misuse. Body handling must be bounded
and support streaming where the transport implementation allows it.

## 4. Runtime backends

### 4.1 Common runtime contract

The runtime abstraction should expose lifecycle operations conceptually
equivalent to:

-   `start(config)`
-   `health()`
-   `execute(request)`
-   `drain()`
-   `recycle(reason)`
-   `shutdown()`
-   `capabilities()`

The actual Rust API may use async methods for orchestration, but
blocking engine execution must not run on Tokio executor threads.

### 4.2 Process Runtime

**Status:** Required for v0.1 baseline.

Responsibilities:

-   Discover and validate PHP executable.
-   Verify PHP version and required extensions.
-   Start PHP child workers.
-   Establish bounded IPC.
-   Track process IDs and exit status.
-   Enforce request deadlines and process resource limits where
    available.
-   Restart unhealthy workers with backoff.
-   Drain workers during reload and shutdown.

Transport selection:

-   Prototype a framed Unix Domain Socket protocol for a PHP CLI worker.
-   Evaluate FastCGI as an interoperability mode.
-   Do not assume PHP-FPM is required.
-   Keep transport implementation behind an internal interface.

The PHP worker bootstrap must be small, auditable, and compatible with
normal PHP request semantics. Long-lived workers must reset
request-scoped state or be recycled. If reliable reset cannot be
guaranteed for an application, use one-request workers or a compatible
conventional execution mode.

### 4.3 Embedded Runtime

**Status:** Experimental; disabled by default.

Implementation considerations:

-   Use PHP Embed SAPI and a narrow C ABI bridge.
-   Pin and validate PHP build configuration and ABI.
-   Keep Zend/PHP lifecycle calls within a controlled execution domain.
-   Do not call PHP NTS concurrently from arbitrary threads.
-   Avoid holding Rust locks across callbacks into PHP.
-   Define ownership and lifetime rules for strings, buffers, and engine
    resources.
-   Contain crashes using a process boundary when required.
-   Advertise capabilities and limitations at runtime.

Embedded Runtime must pass the same application compatibility suite as
Process Runtime for every capability it claims. It must not be promoted
to production default based on throughput tests alone.

## 5. Worker management

### 5.1 Worker states

`Starting -> Ready -> Busy -> Ready`

Additional transitions:

-   `Ready -> Draining -> Stopped`
-   `Busy -> Draining -> Stopped` after the current request
-   `Busy -> Unhealthy -> Restarting -> Ready`
-   `Starting -> Failed` (Failed is terminal for that worker incarnation, with operator-visible diagnostics; Stopped is graceful/drained stop. Full lifecycle, restart budget, and soft/hard memory policy are refined in ADR-0014, which extends this section.)

### 5.2 Worker policies

Configurable controls:

-   minimum and maximum workers;
-   maximum concurrent requests per worker (default 1 for process
    workers);
-   maximum requests before recycling;
-   request execution timeout;
-   startup timeout;
-   graceful drain timeout;
-   restart backoff;
-   health-check interval;
-   per-worker memory target (refined as soft limit for graceful recycle and hard limit for protective termination in ADR-0014; overshoot possible due to sampling/termination latency);
-   queue wait timeout.

Worker pool limits must be bounded. A worker crash must not crash the
gateway.

### 5.3 Backpressure

When all workers are busy:

-   queue only up to a configured bound;
-   reject or time out excess requests predictably;
-   expose saturation metrics;
-   avoid unbounded memory growth.

## 6. HTTP gateway

### 6.1 v0.1 requirements

-   HTTP/1.1 support.
-   HTTP/2 target, gated by integration tests.
-   TLS via Rustls.
-   Host and path routing.
-   PHP front-controller dispatch.
-   Static file serving.
-   Configurable request and response limits.
-   Graceful shutdown.
-   Request IDs and structured access logs.

### 6.2 Deferred

-   HTTP/3.
-   Built-in CDN behavior.
-   Full reverse-proxy feature parity with Nginx/Caddy.
-   Advanced WAF functionality.

### 6.3 Static file safety

-   Resolve paths relative to configured document root.
-   Reject path traversal.
-   Do not follow symlinks outside the allowed root unless explicitly
    configured.
-   Set MIME types safely.
-   Support conditional requests and range requests as implementation
    matures.
-   Never expose PHP source files as static content.

## 7. Platform services

### 7.1 Queue

The queue API must support enqueue, reserve/claim, acknowledge, fail,
retry, delay, and dead-letter operations.

Initial drivers:

-   in-memory: development/testing only, non-durable;
-   SQLite: durable single-node baseline;
-   Redis: optional adapter.

Required semantics:

-   durable acknowledgement for durable drivers;
-   bounded retries;
-   exponential backoff with jitter;
-   visibility timeout or lease recovery;
-   idempotency guidance;
-   dead-letter handling;
-   graceful worker shutdown.

Exactly-once execution is not promised. Consumers must be designed for
at-least-once delivery and idempotent job handling.

### 7.2 Scheduler

-   Cron-compatible expressions.
-   Timezone-aware schedules.
-   Persistent execution records where configured.
-   Duplicate-dispatch protection for a single-node baseline.
-   Missed-run policy must be explicit.
-   Scheduler dispatches jobs; it does not execute long-running PHP work
    on the gateway thread.

### 7.3 Cache

Initial implementations:

-   bounded in-memory cache;
-   filesystem cache;
-   optional Redis adapter.

PHP OPcache remains owned by the PHP runtime. PHPX cache is not a
replacement for OPcache.

### 7.4 Storage and sessions

Storage abstraction targets local filesystem first and S3-compatible
storage later. Session handling must support conventional PHP session
behavior where possible. Any platform-native session API is optional.

## 8. Configuration

Primary file: `phpx.toml`.

Illustrative configuration:

``` toml
schema_version = 1

[server]
host = "127.0.0.1"
port = 8080
tls = false
request_body_limit = "16MB"

[application]
root = "."
document_root = "public"
entrypoint = "index.php"

[runtime]
mode = "process"
php_version = "8.4"

[runtime.process]
workers = 4
max_requests = 1000
request_timeout_seconds = 30
startup_timeout_seconds = 10
memory_limit = "256M" # simplified; full model uses [runtime.process.lifecycle] + [runtime.process.memory] soft/hard limits per ADR-0014

[runtime.embedded]
enabled = false

[queue]
driver = "sqlite"
database = "./data/queue.db"
workers = 2

[scheduler]
enabled = false

[cache]
driver = "memory"
max_memory = "128MB"

[observability]
json_logs = true
metrics = true
```

This is illustrative and simplified, not a frozen schema. The detailed lifecycle/memory/adaptive model (\[runtime.process.lifecycle\], \[runtime.process.memory\], \[runtime.process.adaptive\] with adaptive disabled by default) is defined in ADR-0014 Section 2.9 and takes precedence for worker resource management. All fields require typed
validation, documented defaults, and migration/versioning rules. Secrets
must not be stored in committed configuration.

## 9. CLI contract

Initial command families:

``` text
phpx serve
phpx serve --config phpx.toml
phpx doctor
phpx version
phpx runtime list
phpx runtime inspect
phpx workers status
phpx workers reload
phpx queue status
phpx queue work
phpx scheduler run
phpx metrics
```

CLI output should be human-readable by default and offer
machine-readable JSON where practical. Exit codes must be stable and
documented.

## 10. Security requirements

-   Run as a non-root user by default where feasible.
-   Separate supervisor responsibilities from application execution.
-   Bound request sizes, queue sizes, worker counts, and execution
    times.
-   Do not expose secrets in logs or diagnostics.
-   Restrict application filesystem access to configured roots where
    practical.
-   Validate static file paths against traversal and symlink escapes.
-   Apply OS/container memory and CPU limits for production workloads.
-   Treat PHP extensions and application code as executable trusted code
    unless an explicit sandbox is configured.
-   Do not claim strong multi-tenant isolation in v0.1.

## 11. Observability

Required baseline:

-   structured logs;
-   request ID propagation;
-   worker lifecycle events;
-   readiness and liveness health;
-   request count and latency;
-   active, idle, busy, failed worker counts;
-   PHP execution duration;
-   queue depth and job outcomes;
-   restart and timeout counters.

Prometheus metrics are the initial metrics interface. OpenTelemetry
tracing is introduced incrementally. Sensitive headers, tokens, cookies,
and request bodies must be redacted by default.

## 12. Testing and compatibility

### 12.1 Test layers

-   unit tests for core types, config, routing, and state machines;
-   process integration tests using multiple PHP versions;
-   HTTP integration tests;
-   runtime contract tests shared by both backends;
-   application fixtures for vanilla PHP, Laravel, and WordPress;
-   queue durability and recovery tests;
-   failure injection and worker crash tests;
-   load and soak tests;
-   security tests for path traversal, request limits, and privilege
    boundaries.

### 12.2 Compatibility fixtures

Minimum initial fixtures:

1.  Vanilla PHP front controller.
2.  Composer autoloading.
3.  Laravel request lifecycle and common middleware.
4.  WordPress front controller, permalinks, uploads, cookies, and admin.
5.  WooCommerce checkout/session smoke tests when WordPress baseline is
    stable.

Tests must verify response status, headers, cookies, request body, query
parameters, filesystem behavior, and error handling.

### 12.3 Release gates

A release candidate requires:

-   no known critical security issue;
-   deterministic startup and shutdown;
-   worker crash recovery;
-   bounded behavior under saturation;
-   no request cross-contamination in persistent worker tests;
-   compatibility results published for each claimed runtime/backend
    combination;
-   documented limitations.

## 13. Performance methodology

Do not publish performance claims without reproducible tests.

Benchmark against appropriate baselines including Nginx + PHP-FPM and, where relevant for worker lifecycle claims, FrankenPHP, RoadRunner, and Swoole/OpenSwoole, using identical PHP builds,
extensions, application code, hardware, and workload.

Measure:

-   throughput;
-   p50, p95, and p99 latency;
-   CPU and RSS memory;
-   startup time;
-   worker restart behavior;
-   saturation and recovery;
-   sustained load and memory growth.

Report both simple PHP endpoint results and representative
Laravel/WordPress workloads. A faster microbenchmark is not sufficient
evidence for production readiness.

## 14. Initial crate layout

Start with a small workspace and split crates as boundaries become
concrete:

``` text
phpx/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── phpx.toml.example
├── crates/
│   ├── phpx-cli/
│   ├── phpx-core/
│   ├── phpx-config/
│   ├── phpx-runtime/
│   ├── phpx-runtime-process/
│   ├── phpx-runtime-embedded/
│   ├── phpx-worker/
│   ├── phpx-server/
│   ├── phpx-queue/
│   ├── phpx-scheduler/
│   ├── phpx-cache/
│   └── phpx-observability/
├── integrations/
│   ├── laravel/
│   └── wordpress/
├── examples/
│   └── vanilla-php/
├── tests/
│   ├── integration/
│   ├── compatibility/
│   ├── stress/
│   └── security/
└── docs/
    ├── adr/
    ├── architecture/
    └── operations/
```

Do not create empty crates solely to match this tree. Introduce each
crate when there is code and a stable responsibility.

Mapping note: phpx-server implements the Gateway in Section 3.1; phpx-worker implements Worker Manager supervision; phpx-runtime holds the common runtime contract. Storage, Security policy, and Router start as modules inside existing crates and split into dedicated crates only when boundaries stabilize. Docs live in docs/: adr/, ARCHITECTURE_DECISIONS.md, TECHNICAL_SPEC_v0.1.md, architecture/overview.md, architecture/m1-notes.md, and operations/linux-prerequisites.md. Router, Storage, and Security policy start as modules inside existing crates and split only when boundaries stabilize.

## 15. v0.1 acceptance criteria

PHPX v0.1 developer preview is acceptable when:

1.  `phpx doctor` detects PHP and reports missing prerequisites.
2.  `phpx serve` starts the Rust gateway.
3.  A vanilla PHP application is served through the Process Runtime.
4.  Static assets are served safely.
5.  Multiple PHP workers are supervised.
6.  Worker crashes are detected and recovered without taking down the
    gateway.
7.  Request concurrency and queueing are bounded.
8.  Graceful shutdown drains active requests within configured limits.
9.  Docker and native Linux quickstarts work.
10. Configuration errors are actionable.
11. Basic logs, health checks, and metrics are available.
12. Known compatibility limitations are documented.

Queue, scheduler, embedded runtime, and broad framework compatibility
may be delivered incrementally; they must not delay a reliable first
HTTP/PHP execution path.


