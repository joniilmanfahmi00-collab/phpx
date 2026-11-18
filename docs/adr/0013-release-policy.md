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
