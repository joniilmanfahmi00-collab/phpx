## ADR-0004: Strict PHP application compatibility

-   **Status:** Accepted
-   **Decision:** PHPX targets unmodified, conventional PHP
    applications. Platform-specific APIs are optional enhancements and
    never required for baseline execution.
-   **Context:** The platform must be useful to the existing PHP
    ecosystem, especially Laravel, WordPress, and WooCommerce.
-   **Consequences:**
    -   PHPX must preserve normal PHP request semantics, environment
        variables, document-root/front-controller behavior, headers,
        cookies, request bodies, status codes, and error behavior.
    -   Compatibility is verified using real application fixtures and
        extension matrices.
    -   PHPX does not promise compatibility with every extension or
        every PHP SAPI behavior in v0.1.
    -   Unsupported behavior must be documented precisely; PHPX must
        fail clearly rather than silently alter application semantics.
    -   Optional PHPX APIs require versioning and cannot be
        prerequisites for ordinary application hosting.
