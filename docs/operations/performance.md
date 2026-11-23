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

## P1-B laboratory (harness + release numbers, NOT an SLA)

Harness: `scripts/bench-lab.sh` / `scripts/bench-lab.ps1`
`[--port 18082] [--requests 200] [--concurrency 10] [--mode one-shot|persistent] [--profile debug|release] [--backend phpx|php-builtin] [--workers 4] [--out-dir data/bench/<auto>]`.

Methodology (perbandingan adil): host sami, PHP sami, fixture sami
(`examples/vanilla-php`, `/`), mix sami (GET `/` x N), concurrency sami,
worker/tuning sami, **release build kanggo klaim performa**, report
mentah (JSON `summary.json`: cold_ms, warm p50/p95/p99/max/mean, RPS,
gateway RSS start/max/end, config, versions) + `metrics.txt` + gateway log.
`data/bench/` gitignored; raw dataset disimpen 30 poé.

Hasil lab 2026-11-23 (mesin dev Aa, Windows 10 Pro, PHP 8.3.30, rustc 1.99.1,
200 req, concurrency 10, workers 4, release + thin LTO):

| Backend | RPS | cold_ms | warm mean/p50/p95/p99/max (ms) | gateway RSS start/max (KB) | failures |
|---|---|---|---|---|---|
| phpx one-shot | 3.86 | 215 | 719.7 / 702.3 / 1295.5 / 1649.3 / 1728.1 | 7892 / 9624 | 0 |
| phpx persistent | 5.77 | 16 | 93.4 / 82.0 / 231.1 / 272.6 / 294.0 | 8184 / 9324 | 0 |
| php-builtin (`php -S`) | 6.09 | 118 | 58.8 / 52.5 / 147.5 / 162.0 / 259.2 | n/a | 0 |

Maca tabel: persistent ~8.5x leuwih gancang median batan one-shot spawn path
(82ms vs 702ms) — bédana arsitektur, lain tuning. php -S single-threaded
ngantri di jerona sorangan; gateway PHPX nambahkeun overhead dispatch +
fork PHP per request dina one-shot. Cold persistent 16ms (worker tos
pre-spawned) vs one-shot 215ms (spawn PHP pertama).
Caveat: mesin dev Windows (spawn process mahal), single node, teu aya SLA.
Dimensi séjén (reliability, soak, drain/restart timing) aya di P1-A
(`docs/operations/resilience.md`), lain di tabel ieu.

## Soak-long

- scripts/soak-long.sh / soak-long-windows.ps1: sample RSS tiap 60 dtk ka CSV, slope KB/menit. Gateway harus flat, PHP one-shot direclaim OS.
- **Hasil arsip 2026-11-23** (`data/bench/soak-long-persistent-release/`, release, persistent, 60 mnt, 4 workers, vanilla `/`, 200 req/ronde sekuensial): **12.600/12.600 sukses** (failed/timeout/rejected/circuit = 0); gateway RSS 8732 -> 8948 KB (slope +3,6 KB/mnt, band 324 KB — flat, lain leak); **25 recycle graceful** `recycle-budget` tepat sesuai `max_requests=500` (12600/500), 0 error. Detail + caveat (serial load PS5.1, dev box, 6h/24h masih perlu) di `SUMMARY.md` dalam dir arsip.
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

### Docker (WSL Engine, image phpx:0.1.0-dev) — 2026-11-19

- Build: sudo docker build -t phpx:0.1.0-dev . → 11.5 detik, 19/19 FINISHED (layer cache).
- smoke-docker.sh: ALL GREEN (non-root check: whoami = phpx ✅, healthz/readyz//style.css/traversal-400 lulus).
- Limit test ala operator: --cpus=1 --memory=512m → /healthz ok, /metrics counters 0 semua.
  Wajar: container fresh + /healthz teu lewat pool, jadi completed/failed/timeouts/rejected/restarts = 0.
- Base: rust:1.99.1-bookworm (builder) + php:8.4-cli-bookworm (runtime), user phpx (uid 10001).
