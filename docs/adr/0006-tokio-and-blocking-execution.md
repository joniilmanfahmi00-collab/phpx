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
