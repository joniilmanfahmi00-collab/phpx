# Catatan implementasi M1 (prototipe Process Runtime)

Status: terimplementasi, semua kriteria keluar terverifikasi pada PHP 8.3.30 / Rust 1.99.1 (Windows dev; Linux CI via `.github/workflows/ci.yml`).

## Yang sudah masuk

- `crates/phpx-runtime`: `PROTOCOL_VERSION = 1`, `PhpRequest` / `PhpResponse` / `WorkerOutcome`,
  framing JSON length-prefixed (`encode_frame` / `decode_frame` / `write_frame`, batas 16 MiB).
  Framing yang sama dipakai ulang untuk transport Unix-socket di masa depan (M2); prototipe M1
  melatihnya lewat stdio pipes untuk paritas lintas platform.
- `crates/phpx-runtime-process`: `PhpDiscovery::inspect` (php --version + php -m),
  `execute_one_shot` (`php <entrypoint>` baru per request, env `PHPX_*`, stdin body,
  stdout = body, stderr = diagnostik, `wait_timeout` kill saat kedaluwarsa).
  Worker satu-request menjamin tidak ada state lintas-request secara konstruksi (mode yang diizinkan SPEC 4.2).
- `crates/phpx-runtime-process/bootstrap/worker-bootstrap.php`: prototipe worker persistent berframing
  (penggunaan manual). Hook reset isolasi ditunda ke ADR-0014 2.8 / M4.
- `crates/phpx-worker`: `BoundedPool` (max_workers 1-1024, queue_bound terbatas,
  queue_wait_timeout, execution_timeout, penolakan overload, statistik).
  Crash/timeout menaikkan statistik, tidak pernah menjatuhkan pool.
- Fixture `examples/vanilla-php/public/`: `index.php`, `isolate.php`, `crash.php`, `sleep.php`.
- Pengujian: `m1_execute.rs` (5 lolos), `m1_pool.rs` (2 lolos + 1 unit), plus
  pengujian framing/versi `phpx-runtime` (3 lolos).

## Bukti kriteria keluar

- Start + eksekusi: `executes_vanilla_fixture` (200 + echo rid).
- Terbatas: `bounded_pool_serves_concurrent_requests` (maks 2, 4 konkuren ok),
  `overload_is_rejected_not_queued_unboundedly` (error overload cepat, lalu pulih).
- Isolasi crash: `crash_does_not_take_down_supervisor`.
- Tanpa kontaminasi silang: `sequential_requests_do_not_cross_contaminate` (isolate.php bersih x3).
- Satu versi PHP: PHP 8.3.30 terverifikasi via `phpx doctor` + integration tests.

## Ditunda ke M2/M4

- Transport UnixListener nyata (byte framing sama), evaluasi FastCGI.
- Daur ulang persistent-pool berdasarkan jumlah request/memori (ADR-0014 Fase B-D).
- Pemisahan timeout startup vs eksekusi (M1 memakai satu execution timeout).
