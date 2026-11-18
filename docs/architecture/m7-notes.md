# M7 production candidate notes (pase 1 DONE)

## Goal

RC gate: no critical sec issue, saturation bounded + observable, recovery documented + tested, claims reproducible.

## Pase 1 DONE (this change)

- Threat model + boundaries published (`docs/operations/security.md`).
- Secret redaction helpers + unit tests (`is_sensitive_header`, `redacted_header_value`, `extract_traceparent`).
- Boundary integration tests (`m7_security`: raw-socket encoded-traversal 400, sensitive-header soak, symlink escape 403/404, Dockerfile non-root static check).
- Observability docs (`metrics.md`: /healthz /readyz /metrics series + alerts; OTel-ready note, no exporter in v0.1).
- Ops docs (`upgrade-rollback.md`, `backup-recovery.md`).

Evidence: `cargo test -p phpx-server --lib` (5 ok incl 3 m7), `cargo test -p phpx-server --test m7_security` (4 ok).

## Sisa (pase 2)

- cgroup limit test (adversarial saturation -> 503 bounded, no unbounded queue).
- Soak/bench reproducible report hook (`bench.sh` baseline + `/metrics` snapshot archival).
- Docker hardening + SBOM script (`scripts/sbom.sh` + windows ps1, syft-or-cargo-tree fallback).
- Native package automation (`packaging/` deb/rpm/tar + release checklist).
- Roadmap M7 checkboxes + limitations RC wording + compatibility/deployment matrices publish.

## Pase 2 DONE (this change)

- OTel-ready correlation: `traceparent` extracted in `handle_one` + `dispatch_php`, added to every access log (`request_id + traceparent`), forwarded to PHP env. No exporter dep.
- Resilience: `m7_resilience` (workers=2/queue=2 vs 8x `sleep.php` 5s) stays 200/503/504 + `/metrics` pool series + fast-endpoint recovery + `x-request-id`. Config gates re-asserted (`workers=0`/`queue_bound=0` rejected).
- Ops: `deployment-matrix.md` (Docker/systemd/Windows), `performance.md` (repro bench + archive list + RC bar), `packaging/README.md` + `release.sh/ps1` (fmt/clippy/tests/build/dist+SBOM tarball).
- RC wording: `limitations.md` M7 delta + `compatibility-matrix.md` M7 table.

Evidence: lib 5 ok, m7_security 4 ok, m7_resilience 2 ok, cli 6 ok, clippy clean, fmt clean.
