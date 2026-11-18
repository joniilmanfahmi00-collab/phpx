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
