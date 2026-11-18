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
