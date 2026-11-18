# PHPX M3 notes - Developer preview v0.1

Goal: useful minimal PHP application server. Status: DONE (developer preview, not production SLA).

## What changed vs M2

- CLI finalized to SPEC 9 surface:
  `serve [--host --port]`, `doctor`, `version`, `runtime list|inspect`, `workers status|reload`,
  `queue status|work` (deferred M5 stub, exit 2), `scheduler run` (deferred M5 stub, exit 2), `metrics` (dev-preview note).
  Global `--config` resolution: `--config` > `./phpx.toml` > `phpx.toml.example`. Global `--json` where supported.
  Exit codes: 0 ok, 1 runtime, 2 config/usage/deferred. See docs/operations/configuration.md.
- Gateway: added `GET /healthz` (liveness, always 200 when running) + `GET /readyz` (readiness, docroot + entrypoint check).
  No PHP dispatch, no body read for health paths. Covers SPEC 11 basic health checks for M3.
- Config: `phpx.toml.example` rewritten with documented defaults + Docker/systemd guidance. `PhpxConfig` derives `Serialize` for JSON status outputs.
- Packaging: `Dockerfile` (multi-stage rust:1.99.1-bookworm -> php:8.4-cli-bookworm, non-root `phpx` uid 10001, `HEALTHCHECK /healthz`, default `/app/phpx.toml` serving vanilla fixture).
  `.dockerignore`, `packaging/systemd/phpx.service` (User=phpx, SIGINT graceful drain, hardening flags).
- Docs: `operations/configuration.md`, `quickstart-linux.md`, `quickstart-docker.md`, `php-version-matrix.md`, `limitations.md`, `systemd.md`.
  README labeled developer preview. Roadmap M3 boxes checked.
- Tests: M2 suites still green. New `tests/m3_cli.rs` (CLI contract: version/doctor/runtime/workers/queue/scheduler/metrics exit codes + JSON) and health coverage in `tests/m3_gateway.rs` (`/healthz` + `/readyz` 200 + x-request-id).
  Scripts: `scripts/smoke-linux.sh`, `scripts/smoke-docker.sh`, `scripts/bench.sh` (reproducible baseline, wrk/curl fallback, NOT a performance claim).

## Exit evidence

- `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo test --workspace` green (M1 5+2, M2 3+2, M3 CLI + gateway health).
- `phpx doctor` M3 prints config check + next-step hint.
- Docker quickstart: `docker build -t phpx:0.1.0-dev .` + `docker run -p 8080:8080` + `/healthz` + `/` + static pass; `whoami` -> `phpx`.
- Native Linux quickstart: `cargo build -p phpx-cli`, `phpx doctor`, `phpx serve --config phpx.toml`, `/healthz` + `/readyz` + `/` + static + traversal 400 pass.
- Vanilla fixture passes (front-controller + static + no source leak, same as M2).
- Limitations documented; v0.1 labeled developer preview everywhere (CLI help, serve banner, README, limitations).

## Deferred (explicit, must not block M3)

- Queue/scheduler/cache/storage backends -> M5. CLI stubs exit 2 with deferred note.
- Prometheus metrics, live per-worker counters, SIGHUP reload, soak tests -> M4.
- Embedded runtime -> M6 experimental.
- HTTP/2 -> gated, not claimed. HTTP/3, CDN, WAF, reverse-proxy parity -> deferred per SPEC 6.2.
- Framework fixtures (Composer/Laravel/WordPress) -> M4.

Traceability: SPEC 15 items 1-4, 8-10 covered by M1-M3; items 5-7 + 11-12 complete in M4.
