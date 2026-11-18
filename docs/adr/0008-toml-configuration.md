## ADR-0008: Configuration format and compatibility

-   **Status:** Accepted
-   **Decision:** Use TOML for the primary human-authored configuration
    file, with Serde-based typed deserialization and explicit schema
    validation.
-   **Context:** Operators need a readable, reviewable configuration
    format.
-   **Consequences:**
    -   Configuration has an explicit schema version.
    -   Unknown or invalid critical values fail fast with actionable
        errors.
    -   Defaults are documented and stable.
    -   Environment-variable overrides use a documented naming
        convention.
    -   Secrets should be supplied through environment variables or
        secret mounts, not committed to TOML files.
