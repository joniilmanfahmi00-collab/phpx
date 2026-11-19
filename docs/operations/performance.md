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
## Hasil benchmark RC (dicatet 2026-11-19, developer machine Aa — BUKAN SLA)

Hardware: AMD A8-9600, RAM 6,9 GB, Windows 10 Pro + WSL2 Debian 12.
Fixture: examples/vanilla-php, workers=4, mode=process, debug build.

### Windows native (PowerShell, curl loop sekuensial)

- Perintah: powershell -File scripts/bench-windows.ps1 -Port 18082 -Requests 60 -Concurrency 1
- cold_ms=255 (request pertama saatos healthz, kalebet spawn PHP pertama)
- 60 requests réngsé ~13 detik (~5 req/s, loop sekuensial PS5.1, wrk teu aya)
- doctor: 16/16 héjo. cargo test --workspace: 122 passed, 0 failed.

### WSL2 Debian 12 (curl loop paralel xargs -P10) — 2026-11-18T23:59:24Z

- Perintah: bash scripts/bench.sh --port 18082 --requests 200 --concurrency 10
- cold_ms=712 (request pertama saatos healthz, kalebet spawn PHP pertama)
- 200 requests réngsé 8 detik (~25 req/s, mesin dev saja)
- php=8.3.36, rust=rustc 1.99.1
- smoke-linux.sh: ALL GREEN (gateway + queue + scheduler + cache + storage)

### Catetan perbandingan

- Angka Windows vs Linux teu apple-to-apple: Windows loop sekuensial (concurrency 1),
  Linux paralel (-P10). Bédana métode, lain bédana platform.
- cold_ms Linux (712) > Windows (255): request pertama + spawn PHP pertama dina debug build.
  Lain regresi; baseline dicatet apa adanya.
- Baseline lock tetep: klaim = delta vs angka di luhur, lain SLA absolut. Raw log disimpen 30 poé.
