# Soak + memory-growth analysis (M4)

One-shot workers isolate state by construction, so a soak here looks for
gateway/pool regressions (RSS growth, restart storms, latency drift), not
in-worker PHP leaks (each PHP process exits per request).

## Quick soak (Linux/macOS/WSL2)

```bash
./scripts/soak.sh --port 18081 --requests 2000 --concurrency 8
```

What it does: builds `phpx` (debug), serves `examples/vanilla-php`, curls
`/`, `/hello.php`, `/style.css`, `/metrics` in a loop, samples gateway RSS
(`ps -o rss=`), scrapes `/metrics` counters, and prints a restart/latency report.

## What to record (ADR-0014 Section 5)

- Gateway RSS trend (start/end/max) + slope (KB/request). One-shot PHP RSS is
  per-request and reclaimed by the OS; gateway RSS must stay flat.
- `/metrics` counters: `completed`, `failed`, `timeouts`, `rejected_overload`,
  `recycled`, `restarts`, `circuit_breaks`; plus `active`/`waiting` peaks.
- Restart count + reason (5xx vs timeout vs circuit), throughput (req/s),
  p50/p95 latency from the script report.
- Adaptive scaling stays DISABLED in M4 (fixed pool baseline per ADR-0014 Phase D).

## Pass bar (M4)

- No cross-contamination (`isolate.php` stays `clean`).
- Crash + drain + 503 paths exercised without gateway restart.
- RSS slope ~flat; no unbounded growth; circuit opens only under injected
  failure (e.g. `crash.php` loop), then recovers.
- Report checked into the release notes; raw logs kept for 30 days.
