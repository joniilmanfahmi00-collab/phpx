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
