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
