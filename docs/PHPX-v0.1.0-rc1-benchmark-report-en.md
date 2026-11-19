# PHPX v0.1.0-rc1 — Test and Benchmark Report

> Audience: developers evaluating or contributing to PHPX.
> Status: release candidate on a developer machine. This report characterises behaviour; it is not a production SLA.

## 1. Scope

- Test evidence for the v0.1.0-rc1 gate: six required suites plus the M6 embedded containment gates.
- Reproducible single-node baselines for the vanilla fixture on Windows, WSL2 Debian, and Docker.
- Known limitations and exactly how to reproduce every number below.

## 2. Method

- Fixtures are checked in: examples/vanilla-php (mandatory), examples/laravel-minimal, wordpress-minimal, woocommerce-minimal (M4 compat).
- Runtime: process mode, workers=4, debug build, unless stated otherwise.
- Workloads: bench 200 requests / concurrency 10; soak-short 2000 / 8; soak-long 60 min (runbook in scripts/soak.sh).
- Cold means the first request after /healthz OK (includes the first PHP spawn); warm means requests 2..N (report p50/p95/max, never blend cold into the mean).
- Claims are deltas against the checked-in baseline, never absolute promises.

## 3. Environment

| Item | Windows native | WSL2 Debian 12 |
|---|---|---|
| CPU / RAM | AMD A8-9600, 6.9 GB | same host via WSL2 |
| OS | Windows 10 Pro | Debian 12 (kernel via WSL2) |
| PHP | 8.3.30 ZTS (dev box; rejected for embed by design) | 8.3.36 NTS |
| Rust | rustc 1.99.1 | rustc 1.99.1 |
| Docker | n/a | Engine under WSL, image phpx:0.1.0-dev |

## 4. Test results (M7 exit 6/6, verified 2026-11-19)

| Suite | Result |
|---|---|
| phpx-server lib (incl. 3x M7 redaction/traversal) | 5 passed |
| m4_compat (vanilla, composer, laravel, wp+woo, metrics) | 5 passed |
| m7_security (traversal 400, symlink 403/404, header soak, non-root) | 4 passed |
| m7_resilience (8x slow sleepers on 2 workers stay 200/503/504 + recovery) | 2 passed |
| CLI m3_cli (version, doctor, queue, cache/storage, runtime/workers, metrics) | 6 passed |
| Queue contract (SQLite enqueue/reserve/ack + restart) | 6 passed |
| Embedded linked-mode gates (WSL, libphp linked) | 28 lib + 12 contract passed |
| Earlier full workspace run (2026-11-18) | 122 passed, 0 failed |

OTel posture: OTel-ready (request_id + W3C traceparent passthrough + JSON logs). No OTLP exporter in v0.1, deliberately, to keep the release auditable. cgroup and OS limits remain operator duty (Docker --cpus/--memory, systemd MemoryMax/CPUQuota).

## 5. Benchmark results (developer machine only)

### 5.1 Windows native (PowerShell, sequential curl loop)

- Command: bench-windows.ps1 -Port 18082 -Requests 60 -Concurrency 1
- cold_ms=255; 60 requests in about 13 s (roughly 5 req/s; sequential loop, no wrk).
- doctor 16/16 green.

### 5.2 WSL2 Debian (parallel curl loop, xargs -P10, 2026-11-18T23:59:24Z)

- Command: bench.sh --port 18082 --requests 200 --concurrency 10
- cold_ms=712 (first request incl. first PHP spawn, debug build); 200 requests in 8 s (roughly 25 req/s).
- smoke-linux.sh: ALL GREEN (gateway + queue + scheduler + cache + storage).

### 5.3 Docker (image phpx:0.1.0-dev, 2026-11-19)

- Build 11.5 s (layer cache); smoke-docker.sh ALL GREEN as non-root (whoami=phpx).
- Operator limit probe (--cpus=1 --memory=512m): /healthz OK; /metrics counters 0 on a fresh container (healthz bypasses the pool, so zeroes are expected, not a fault).
- Base: rust:1.99.1-bookworm builder + php:8.4-cli-bookworm runtime, user phpx (uid 10001).

### 5.4 Reading the numbers honestly

- Windows vs Linux are not apple-to-apple: sequential vs parallel harnesses differ in method, not platform.
- cold_ms differs (712 vs 255) because it captures the first PHP spawn on a debug build; it is recorded as-is, not a regression.
- Use these figures only as a baseline for deltas on the same machine and harness.

## 6. Verdict

M7 exit criteria are met as code plus tests plus docs: saturation is bounded and observable, secrets are never logged, recovery is documented and tested, and every claim above reproduces from the commands cited. Embedded execution stays experimental (available=false) with explicit per-subset blockers; the process runtime remains the default.

## 7. Known limitations (RC)

- Windows is dev tooling; production target is Linux/Docker.
- Embedded runtime is experimental until its compatibility, isolation, and stability gates pass.
- Observability is OTel-ready, not an OTLP exporter. Metrics and logs are the RC surface.
- Resource enforcement belongs to the operator (cgroups via Docker/systemd).
- Full detail: docs/operations/limitations.md and the rc checklist.

## 8. Reproducibility

- Per-run archive: versions, exact command, UTC date, OS/CPU/RAM, /metrics before/after, bench log, gateway RSS trend. Raw logs retained 30 days.
- Indonesian edition (Word/PDF, generated on the author PC): docs/PHPX-v0.1.0-rc1-Hasil-Test-Benchmark.docx and .pdf.
- This English edition is the developer-facing revision; the PDF rendition is attached at release (see README TODO).
