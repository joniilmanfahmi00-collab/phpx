# PHPX performance baseline (M7 reproducible, NOT an SLA)

## Repro command (single-node, vanilla fixture)

```bash
./scripts/bench.sh --port 18082 --requests 200 --concurrency 10
# Windows:
powershell -File scripts/bench-windows.ps1 -Port 18082 -Requests 200 -Concurrency 10
```

What it records: gateway build (`cargo build -p phpx-cli`), serves `examples/vanilla-php`, curls `/`, `/hello.php`, `/style.css`, `/metrics` in a loop, samples gateway RSS, scrapes `/metrics` counters, prints throughput + p50/p95 + restart report.

## What to archive per RC run

- `phpx --version`, `php --version`, `rustc --version`, OS + CPU/RAM, Docker tag where relevant.
- Full `/metrics` snapshot before/after (completed/failed/timeouts/rejected/recycled/restarts/circuit/active/waiting).
- bench log + gateway RSS start/end/max + slope (KB/request). One-shot PHP RSS is per-request (OS reclaims); gateway RSS must stay flat.
- Saturation note: `m7_resilience` proves 8x slow sleepers on 2 workers + queue 2 stay bounded (200/503/504) + `/metrics` observable + fast endpoint recovers 200.

## Pass bar (RC)

- No cross-contamination, crash + drain + 503 paths exercised (M4/M7 suites green).
- Claims are deltas vs checked-in baseline, never absolute SLA. Raw logs kept 30 days.
## Baseline lock (RC)

Lihat baseline.md: fixture vanilla-php wajib + laravel-minimal opsional, workload 200/10 bench, 2000/8 soak-short, 60 mnt soak-long. Tiap run wajib catet OS/CPU/RAM + phpx/php/rustc version + perintah + tanggal UTC.

## Cold vs warm

- cold_ms: request 200 pertama saatos /healthz OK. bench.sh/ps1 ayeuna ngukur cold otomatis.
- warm: request 2..N, lapor p50/p95/max (wrk) atanapi RPS kasar (curl loop). Tong campur cold kana rata-rata.

## Compare (skeleton)

- scripts/bench-compare.sh / bench-compare-windows.ps1: phpx vs php -S docroot sami, workload sami. php-fpm/FrankenPHP/RR tinggal ganti langkah 2, prinsip sami: hardware + build + app + tuning sami, klaim = delta vs baseline.

## Soak-long

- scripts/soak-long.sh / soak-long-windows.ps1: sample RSS tiap 60 dtk ka CSV, slope KB/menit. Gateway harus flat, PHP one-shot direclaim OS.
