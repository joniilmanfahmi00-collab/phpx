# PHPX RC baseline lock (v0.1, reproducible, NOT SLA)

## Locked fixtures

- App A (wajib): examples/vanilla-php, routes / + /hello.php + /style.css + /metrics
- App B (opsional RC): examples/laravel-minimal hello route (tanpa DB/Redis)
- Runtime: mode=process, workers=4, max_requests=1000, timeout 30s bench / 10s soak
- PHP: catet php --version, ZTS on/off, php -m ext list

## Locked workload

- bench: 200 req, concurrency 10, single-node 127.0.0.1 (default RC)
- bench-repeat (reproducibility record): 3 runs bench config sami persis,
  gate `--max-failures 0 --max-cv-pct 30` (rps/p50/p95), arsip
  `reproducibility.json` + 3x `summary.json` 30 poé
- soak-short: 2000 req, concurrency 8 (smoke stability)
- soak-long: 60 menit, concurrency 8, sample RSS tiap 60 detik (slope KB per menit)
- compare: workload sami persis phpx vs php-builtin vs php-fpm lamun aya

## Hardware yg wajib dicatet tiap run

- OS + kernel, CPU model + core, RAM total, disk type
- phpx --version, php --version, rustc --version, docker tag lamun docker
- env: RUST_LOG, workers, queue_bound, queue_wait
- perintah persis + tanggal UTC + operator

## Cold vs warm (definisi RC)

- cold: request 200 pertama saatos /healthz OK (termasuk spawn PHP pertama)
- warm: request ke-2 sampai N (pool anget, ukur p50/p95/max, bukan rata-rata tok)
- laporan wajib misah: cold_ms + warm p50/p95/max + RPS

## Archive per run

- /metrics snapshot before/after
- bench/soak log + gateway RSS start/end/max + slope
- raw log 30 hari, klaim = delta vs baseline checked-in, bukan SLA absolut
