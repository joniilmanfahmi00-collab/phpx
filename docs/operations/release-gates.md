# PHPX release gates — evidence assessment (post P1-B + integration)

Assessed against `docs/phpx-engineering-review-remediation-plan.md` §7.
Verdicts: DONE / PARTIAL / NOT DONE. Evidence links to suites, scripts, docs.

## Developer preview: DONE

- Basic gateway and execution paths work (one-shot + persistent, queue/
  scheduler/cache/storage surfaces, TLS, drain, health/readiness).
- Known limitations documented (`limitations.md`, `sapi-audit.md`,
  `compatibility-matrix.md`); no production SLA implied anywhere user-facing.
- Evidence: `cargo test --workspace` green (183 passed, 0 failed),
  `scripts/smoke-*.sh/ps1` green, `phpx doctor` green.

## Alpha: DONE

| Criterion | Verdict | Evidence |
|---|---|---|
| Fixed worker lifecycle is deterministic | DONE | P0-B state machine (`Starting→Ready→Busy→Ready`, drain, `Failed` terminal) + unit tests `lifecycle_transitions_are_deterministic`, `failed_is_terminal_except_shutdown` |
| Bounded queue and backpressure are tested | DONE | `m7_resilience` (saturation + overload + metrics + recovery), `m1_pool`, `p0b: queue_bound_is_enforced` |
| Worker crash and restart behavior are observable | DONE | Per-worker snapshots (state/PID/incarnation/exit reason), `phpx_worker_*` series, recovery timeline (`last_recovery`, `phpx_worker_last_recovery_duration_seconds`), P1-A crash/idle/timeout/circuit tests |
| Core compatibility tests pass | DONE | Fixture suite (`m4_compat` incl. urlencoded `$_POST`) + real Laravel 12 (`p0d_laravel` 5/5) + real WordPress 7.1 (`p0d_wordpress` 8/8) |

## Beta: DONE (single-node developer scope; 6h/24h trends remain future work)

| Criterion | Verdict | Evidence / gap |
|---|---|---|
| Resource guardian is implemented and tested | DONE | P0-C: OS-RSS soft (recycle) / hard (terminate) policy, idle sampler + request-completion sampling, per-worker memory + lifecycle metrics, soft/hard/sampler tests with controlled workloads, `memory_limit` enforced as PHP `-d` heap limit |
| Real Laravel, WordPress + WooCommerce smoke/integration tests exist | DONE (with explicit SAPI limits) | Smoke + integration: Laravel routing, error JSON, server-side sessions, multipart grace (`p0d_laravel` 5/5); WordPress REST, login flow, comment round-trip, media auth chain (`p0d_wordpress` 13/13 incl. WooCommerce shop loop, product page, cart/checkout redirect docs, `wc/v3` catalog JSON); `$_POST` urlencoded at fixture level (`m4_compat`). Explicit limits: upload *bytes* blocked (`php://input`/`$_FILES` unavailable — pinned as JSON error codes), session/`Set-Cookie` round-trip swallowed, PHP status/headers not propagated |
| Soak and failure-injection tests are repeatable | DONE (single-node scope) | Failure-injection automated: P1-A matrix (`p1a_resilience` 9/9 incl. app-exit resync) plus P0-B/P0-C suites. Long soak archived: `data/bench/soak-long-persistent-release/` (60 min, release, 12.600/12.600, RSS flat +3.6 KB/min, 25 graceful recycles, 0 errors) — see `SUMMARY.md` there and `performance.md`. 6h/24h trends stay future work, not a Beta blocker |
| Security and deployment limitations are documented | DONE | `security.md` (threat model, boundaries, secret rules), `deployment-matrix.md` (Docker/systemd/Windows), `limitations.md` refreshed to the current state, `sapi-audit.md` (CLI-SAPI limits incl. status/header propagation) |

Beta is met on single-node developer scope: the integration suite above stays
green on CI (provisioned installs), one archived 60-min soak exists per the
baseline convention (`data/bench/soak-long-persistent-release/`), and the
remaining upload-bytes gap needs a multipart parser (future SAPI work) that
does not block Beta — the boundary is tested and documented.
6h/24h trends stay RC-track work.

## Release candidate: NOT DONE

- Benchmarks are developer-machine, single-node; P1-B lab adds release numbers
  (one-shot 3.86 / persistent 5.77 / php-builtin 6.09 rps, 200/10) plus a
  methodology for repeats — but no multi-run reproducibility record or
  load/soak SLOs are defined yet.
- Recovery and memory behavior are measured per-incident but have no
  acceptance thresholds yet (only observed values).
- Compatibility matrix marks uploads/`Set-Cookie` round-trip as explicit SAPI
  limits with pinned tests; sessions/JSON/forms/comments/REST are covered on
  real apps.
- No unresolved critical security issues known; data-integrity story is
  upgrade-rollback + backup-recovery docs (operator-owned restores).

## Stable 1.0: NOT DONE

- All 1.0 claims need test evidence (Beta gaps first).
- Upgrade/rollback procedures exist as docs; production hardening
  (multi-tenant isolation, WAF parity, encrypted-at-rest) is out of scope.
- Tag `v0.1.0-rc1` predates this assessment and does not imply the RC gate
  above; keep it as a developer-preview marker, not a release claim.
