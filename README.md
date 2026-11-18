# PHPX

**A Rust-powered PHP application platform.**

PHPX aims to provide a unified runtime and infrastructure platform for
existing PHP applications, with a Rust control plane and a hybrid PHP
execution architecture.

## Project direction

-   **Product:** PHPX
-   **Language:** Rust
-   **Deployment:** Native Linux and Docker
-   **Compatibility:** Strict baseline; ordinary PHP applications should
    not require source changes
-   **Runtime:** Hybrid Process Runtime and experimental Embedded
    Runtime
-   **Initial integrations:** Vanilla PHP, Laravel, WordPress

## Status

**PHPX v0.1 developer preview** - useful minimal PHP application server (M1-M3 complete).
Not a production SLA. See `docs/operations/limitations.md`.

Quickstarts:

- Native Linux: `docs/operations/quickstart-linux.md`
- Docker (non-root): `docs/operations/quickstart-docker.md`
- Config defaults: `docs/operations/configuration.md`
- PHP matrix: `docs/operations/php-version-matrix.md`
- systemd: `docs/operations/systemd.md` + `packaging/systemd/phpx.service`

Health: `GET /healthz` (liveness), `GET /readyz` (readiness). CLI: `phpx doctor`, `phpx serve`, `phpx runtime list|inspect`, `phpx workers status|reload`, `phpx metrics`.

## Documentation

-   [Technical Specification v0.1](docs/TECHNICAL_SPEC_v0.1.md)
-   [Roadmap](roadmap.md)
-   [Architecture Decision Records](docs/adr/README.md)

## Initial development principles

1.  Process Runtime first.
2.  Embedded Runtime remains experimental until proven safe and
    compatible.
3.  PHP stays the application language.
4.  Rust manages infrastructure and lifecycle.
5.  Bound all queues, workers, and resource usage.
6.  Test real PHP applications, not only synthetic benchmarks.

## License

MIT License — see `LICENSE`.

