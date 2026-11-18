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
