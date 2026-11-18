## ADR-0007: HTTP gateway implementation

-   **Status:** Accepted
-   **Decision:** Use Hyper with Tokio for the initial HTTP gateway, and
    Rustls for TLS.
-   **Context:** These components provide a modular Rust HTTP foundation
    without requiring PHP to own the network listener.
-   **Consequences:**
    -   HTTP/1.1 is mandatory for v0.1; HTTP/2 is a target subject to
        integration testing.
    -   HTTP/3 is out of scope for v0.1.
    -   Static file serving, request limits, graceful shutdown, and PHP
        front-controller dispatch are part of the gateway.
    -   Reverse proxy and advanced edge features are deferred.
