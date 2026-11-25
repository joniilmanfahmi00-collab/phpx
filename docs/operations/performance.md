# Baseline performa PHPX (M7 dapat diulang, BUKAN SLA)

## Perintah repro (satu-node, fixture vanilla)

```bash
./scripts/bench.sh --port 18082 --requests 200 --concurrency 10
# Windows:
powershell -File scripts/bench-windows.ps1 -Port 18082 -Requests 200 -Concurrency 10
```

Fungsinya: membangun gateway (`cargo build -p phpx-cli`), melayani `examples/vanilla-php`, me-loop curl `/`, `/hello.php`, `/style.css`, `/metrics`, mengambil sampel RSS gateway, mengikis counter `/metrics`, dan mencetak throughput + p50/p95 + laporan restart.

## Yang diarsipkan per run RC

- `phpx --version`, `php --version`, `rustc --version`, OS + CPU/RAM, tag Docker bila relevan.
- Snapshot `/metrics` penuh sebelum/sesudah (completed/failed/timeouts/rejected/recycled/restarts/circuit/active/waiting).
- Log bench + RSS gateway awal/akhir/maks + slope (KB/request). RSS PHP one-shot itu per-request (OS mereklamasi); RSS gateway harus tetap flat.
- Catatan saturasi: `m7_resilience` membuktikan 8 sleeper lambat pada 2 worker + antre 2 tetap terbatas (200/503/504) + `/metrics` teramati + endpoint cepat pulih 200.

## Ambang kelulusan (RC)

- Tanpa kontaminasi silang, jalur crash + drain + 503 dilatih (suite M4/M7 hijau).
- Ambang recovery/guardian/soak di `resilience.md` hijau (cap per-leg, kill->first-200, latensi recycle, konservasi).
- Smoke bench-lab: `failures == 0` (`--max-failures 0`); rps/p95 dicatat, bukan SLA absolut.
- Klaim adalah delta vs baseline yang checked-in, bukan SLA absolut. Log mentah disimpan 30 hari.

## Kunci baseline (RC)

Lihat baseline.md: fixture vanilla-php wajib + laravel-minimal opsional, workload bench 200/10, soak-short 2000/8, soak-long 60 menit. Tiap run wajib mencatat OS/CPU/RAM + versi phpx/php/rustc + perintah + tanggal UTC.

## Cold vs warm

- cold_ms: request 200 pertama setelah /healthz OK. bench.sh/ps1 kini mengukur cold otomatis.
- warm: request 2..N, lapor p50/p95/max (wrk) atau RPS kasar (curl loop). Jangan campur cold ke rata-rata.

## Compare (kerangka)

- scripts/bench-compare.sh / bench-compare-windows.ps1: phpx vs php -S docroot sama, workload sama. php-fpm/FrankenPHP/RR tinggal ganti langkah 2, prinsip sama: hardware + build + app + tuning sama, klaim = delta vs baseline.

## Laboratorium P1-B (harness + angka release, BUKAN SLA)

Harness: `scripts/bench-lab.sh` / `scripts/bench-lab.ps1`
`[--port 18082] [--requests 200] [--concurrency 10] [--mode one-shot|persistent] [--profile debug|release] [--backend phpx|php-builtin] [--workers 4] [--out-dir data/bench/<auto>]`.

Metodologi (perbandingan adil): host sama, PHP sama, fixture sama
(`examples/vanilla-php`, `/`), mix sama (GET `/` x N), konkurensi sama,
worker/tuning sama, **build release untuk klaim performa**, report
mentah (JSON `summary.json`: cold_ms, warm p50/p95/p99/max/mean, RPS,
RSS gateway awal/maks/akhir, config, versions) + `metrics.txt` + log gateway.
`data/bench/` di-gitignore; dataset mentah disimpan 30 hari.

Hasil lab 2026-11-23 (mesin dev, Windows 10 Pro, PHP 8.3.30, rustc 1.99.1,
200 req, konkurensi 10, workers 4, release + thin LTO):

| Backend | RPS | cold_ms | warm mean/p50/p95/p99/max (ms) | gateway RSS awal/maks (KB) | failures |
|---|---|---|---|---|---|
| phpx one-shot | 3.86 | 215 | 719.7 / 702.3 / 1295.5 / 1649.3 / 1728.1 | 7892 / 9624 | 0 |
| phpx persistent | 5.77 | 16 | 93.4 / 82.0 / 231.1 / 272.6 / 294.0 | 8184 / 9324 | 0 |
| php-builtin (`php -S`) | 6.09 | 118 | 58.8 / 52.5 / 147.5 / 162.0 / 259.2 | n/a | 0 |

Membaca tabel: persistent ~8.5x lebih cepat median dibanding jalur spawn one-shot
(82ms vs 702ms) — beda arsitektur, bukan tuning. php -S single-threaded
mengantre di dalamnya sendiri; gateway PHPX menambah overhead dispatch +
fork PHP per request pada one-shot. Cold persistent 16ms (worker sudah
pre-spawned) vs one-shot 215ms (spawn PHP pertama).
Caveat: mesin dev Windows (spawn proses mahal), satu node, tanpa SLA. Dimensi
lain (reliability, soak, timing drain/restart) ada di P1-A
(`docs/operations/resilience.md`), bukan di tabel ini.

## Bench repeat (rekor reprodusibilitas, BUKAN SLA)

`scripts/bench-repeat.sh` / `bench-repeat.ps1` menjalankan `bench-lab.sh` N kali
(config persis sama) lalu mengagregat `run-i/summary.json` menjadi
`reproducibility.json` (mean/min/max/stdev/CV per metrik) + gerbang
`--max-cv-pct` (default 30) untuk rps + warm p50/p95. `cold_ms` dilaporkan
tanpa gerbang (spawn noise by design). Tiap run juga memaksa `--max-failures`
(default 0).

```
bash scripts/bench-repeat.sh --runs 3 --requests 200 --concurrency 10 \
  --mode persistent --profile release --out-dir data/bench/<rc-record> [--max-cv-pct 30]
```

Bukti metodologi 2026-11-24 (mesin dev, debug, one-shot, N kecil 2x20 — BUKAN
rekor RC): rps CV 2.33%, warm p50 CV 6.61%, p95 CV 16.93% → PASS gerbang 30%.
Rekor RC harus: release + workload dikunci (`baseline.md`: 3 run, 200 req,
konkurensi 10) + hardware RC + arsip `reproducibility.json` 30 hari.

## Soak-long

- scripts/soak-long.sh / soak-long-windows.ps1: sampel RSS tiap 60 dtk ke CSV, slope KB/menit. Gateway harus flat, PHP one-shot direklamasi OS.
- **Hasil arsip 2026-11-23** (`data/bench/soak-long-persistent-release/`, release, persistent, 60 mnt, 4 workers, vanilla `/`, 200 req/ronde sekuensial): **12.600/12.600 sukses** (failed/timeout/rejected/circuit = 0); gateway RSS 8732 -> 8948 KB (slope +3,6 KB/mnt, pita 324 KB — flat, bukan leak); **25 recycle graceful** `recycle-budget` tepat sesuai `max_requests=500` (12600/500), 0 error. Detail + caveat (beban serial PS5.1, dev box) di `SUMMARY.md` dalam dir arsip.
- **Hasil arsip 2026-11-25** (`data/bench/soak-6h-persistent-release-win/`, release, persistent, 6 jam, 4 workers, vanilla `/`, 200 req/ronde, konkurensi 8, Windows nativ): **73.600/73.600 sukses** (failed/timeout/rejected/circuit = 0); gateway RSS 17744 -> 17436 KB (min 17424, maks 18004, slope **-0,86 KB/mnt** — flat, bukan leak); **147 recycle graceful** tepat sesuai `max_requests=500` (73600/500), 0 error. `SUMMARY.md` dalam dir arsip.

## Hasil benchmark RC (dicatat 2026-11-19, mesin developer — BUKAN SLA)

Hardware: AMD A8-9600, RAM 6,9 GB, Windows 10 Pro + WSL2 Debian 12.
Fixture: examples/vanilla-php, workers=4, mode=process, debug build.

### Windows nativ (PowerShell, curl loop sekuensial)

- Perintah: powershell -File scripts/bench-windows.ps1 -Port 18082 -Requests 60 -Concurrency 1
- cold_ms=255 (request pertama setelah healthz, termasuk spawn PHP pertama)
- 60 request selesai ~13 detik (~5 req/dtk, loop sekuensial PS5.1, wrk tidak ada)
- doctor: 16/16 hijau. cargo test --workspace: 122 passed, 0 failed.

### WSL2 Debian 12 (curl loop paralel xargs -P10) — 2026-11-18T23:59:24Z

- Perintah: bash scripts/bench.sh --port 18082 --requests 200 --concurrency 10
- cold_ms=712 (request pertama setelah healthz, termasuk spawn PHP pertama)
- 200 request selesai 8 detik (~25 req/dtk, mesin dev saja)
- php=8.3.36, rust=rustc 1.99.1
- smoke-linux.sh: ALL GREEN (gateway + queue + scheduler + cache + storage)

### Catatan perbandingan

- Angka Windows vs Linux tidak apple-to-apple: Windows loop sekuensial (konkurensi 1),
  Linux paralel (-P10). Beda metode, bukan beda platform.
- cold_ms Linux (712) > Windows (255): request pertama + spawn PHP pertama pada debug build.
  Bukan regresi; baseline dicatat apa adanya.
- Kunci baseline tetap: klaim = delta vs angka di atas, bukan SLA absolut. Log mentah disimpan 30 hari.

### Docker (WSL Engine, image phpx:0.1.0-dev) — 2026-11-19

- Build: sudo docker build -t phpx:0.1.0-dev . → 11,5 detik, 19/19 FINISHED (layer cache).
- smoke-docker.sh: ALL GREEN (cek non-root: whoami = phpx ✅, healthz/readyz//style.css/traversal-400 lulus).
- Limit test ala operator: --cpus=1 --memory=512m → /healthz ok, /metrics counter 0 semua.
  Wajar: container fresh + /healthz tidak lewat pool, jadi completed/failed/timeouts/rejected/restarts = 0.
- Base: rust:1.99.1-bookworm (builder) + php:8.4-cli-bookworm (runtime), user phpx (uid 10001).
