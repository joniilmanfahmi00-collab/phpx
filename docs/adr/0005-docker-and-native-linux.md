## ADR-0005: Docker and native Linux are first-class deployment targets

-   **Status:** Accepted
-   **Decision:** PHPX supports both native Linux installation and
    containerized deployment. Neither is a second-class afterthought.
-   **Context:** Native deployments are useful for direct host
    integration and low-overhead installations. Containers are central
    to repeatable deployments and orchestration.
-   **Consequences:**
    -   The same core binary and configuration model should work in both
        environments.
    -   Filesystem paths, signals, permissions, cgroups, and process
        supervision must be tested in both.
    -   Docker images must support non-root execution by default where
        feasible.
    -   Native packaging and service integration (for example systemd)
        are documented separately from container orchestration.
    -   Windows is not a v0.1 production target; Windows development may
        use WSL2 or containers.
