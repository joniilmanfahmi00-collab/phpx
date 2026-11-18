# Architecture Decision Records

Accepted architecture decisions for PHPX v0.1 (ADR-0001 to ADR-0013),
plus one proposed extension:

-   [ADR-0001: Product identity and
    scope](0001-product-identity-and-scope.md)
-   [ADR-0002: Rust is the platform control
    plane](0002-rust-control-plane.md)
-   [ADR-0003: Hybrid PHP runtime
    architecture](0003-hybrid-php-runtime.md)
-   [ADR-0004: Strict PHP application
    compatibility](0004-strict-compatibility.md)
-   [ADR-0005: Docker and native Linux
    deployment](0005-docker-and-native-linux.md)
-   [ADR-0006: Tokio orchestration and blocking
    execution](0006-tokio-and-blocking-execution.md)
-   [ADR-0007: HTTP gateway implementation](0007-http-gateway.md)
-   [ADR-0008: TOML configuration](0008-toml-configuration.md)
-   [ADR-0009: Queue durability and adapters](0009-queue-durability.md)
-   [ADR-0010: Built-in observability](0010-observability.md)
-   [ADR-0011: Security and resource
    isolation](0011-security-and-isolation.md)
-   [ADR-0012: Modular Rust workspace](0012-modular-workspace.md)
-   [ADR-0013: Release and compatibility policy](0013-release-policy.md)
-   [ADR-0014: Worker lifecycle, resource management and adaptive
    recycling](0014-worker-lifecycle-resource-management.md) — Status:
    Proposed. Extends SPEC Section 5 and Section 8 with lifecycle,
    soft/hard memory thresholds, and deferred adaptive scaling. Main
    body in Sundanese with English abstract header.

See [all decisions in one document](../ARCHITECTURE_DECISIONS.md) for
context and consequences. ADR-0014 summary is included there as
Proposed, not yet part of the Accepted baseline.
