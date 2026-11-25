# Kunci baseline RC PHPX (v0.1, dapat diulang, BUKAN SLA)

## Fixture terkunci

- App A (wajib): examples/vanilla-php, routes / + /hello.php + /style.css + /metrics
- App B (opsional RC): examples/laravel-minimal hello route (tanpa DB/Redis)
- Runtime: mode=process, workers=4, max_requests=1000, timeout 30 dtk bench / 10 dtk soak
- PHP: catat php --version, ZTS on/off, daftar ext php -m

## Workload terkunci

- bench: 200 req, konkurensi 10, satu-node 127.0.0.1 (default RC)
- bench-repeat (rekor reprodusibilitas): 3 run bench config persis sama,
  gerbang `--max-failures 0 --max-cv-pct 30` (rps/p50/p95), arsip
  `reproducibility.json` + 3x `summary.json` 30 hari
- soak-short: 2000 req, konkurensi 8 (smoke stabilitas)
- soak-long: 60 menit (minimum), 6 jam bila memungkinkan; sampel RSS tiap
  60 detik (slope KB per menit). Arsip 6 jam 2026-11-25: `data/bench/soak-6h-persistent-release-win/`
- compare: workload persis sama phpx vs php-builtin vs php-fpm bila ada

## Hardware yang wajib dicatat tiap run

- OS + kernel, model CPU + core, total RAM, tipe disk
- phpx --version, php --version, rustc --version, tag docker bila docker
- env: RUST_LOG, workers, queue_bound, queue_wait
- perintah persis + tanggal UTC + operator

## Cold vs warm (definisi RC)

- cold: request 200 pertama setelah /healthz OK (termasuk spawn PHP pertama)
- warm: request ke-2 sampai N (pool hangat, ukur p50/p95/max, bukan rata-rata saja)
- laporan wajib memisah: cold_ms + warm p50/p95/max + RPS

## Arsip per run

- Snapshot /metrics sebelum/sesudah
- Log bench/soak + RSS gateway awal/akhir/maks + slope
- Log mentah 30 hari, klaim = delta vs baseline checked-in, bukan SLA absolut
