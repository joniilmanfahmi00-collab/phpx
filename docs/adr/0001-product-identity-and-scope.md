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
