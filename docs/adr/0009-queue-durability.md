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
