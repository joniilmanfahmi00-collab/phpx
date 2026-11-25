# Runbook resiliensi + soak PHPX (P1-A)

Bukti failure-injection dan recovery yang dapat diulang. Semua suite di bawah
berjalan di `cargo test --workspace` (tanpa langkah manual, tanpa layanan
eksternal kecuali DB WordPress P0-D yang di luar lingkup sini).

## Matriks failure-injection (semua otomatis)

| Item rencana | Test | Yang dibuktikan |
|---|---|---|
| Crash saat memproses | `p1a: crash via crash.php` (tercakup pra-P1-A di `m1_execute`, `m4_compat`) + `p0b: crash_restarts_worker_with_new_incarnation` | Gateway selamat; worker terawasi restart dengan inkarnasi + PID baru |
| Crash saat idle | `p1a_resilience::crash_while_idle_restarts_on_next_request` (`kill -9` / `taskkill` eksternal pada PID worker Ready) | Request berikut mendeteksi worker mati, me-restart-nya, melayani 500-lalu-200 |
| Timeout request | `m1_execute::execution_timeout_kills_slow_worker`, `p0b: timeout_kills_and_restarts_hung_worker` | Worker macet di-kill, 504, pengganti melayani |
| Gagal startup | `p0b: startup_failure_reports_diagnostic` | Pool menolak start dengan 0/N ready + alasan (tanpa kapasitas setengah mati) |
| Habis budget restart | `p0b: restart_budget_exhaustion_marks_failed_with_reason` | Worker menjadi Failed dengan diagnostik; eksekusi menyampaikannya (tanpa loop tak berujung) |
| Proteksi restart-loop | `p1a_resilience::crash_loop_opens_circuit_then_admits_again` | Crash beruntun membuka sirkuit (503 cepat); setelah backoff pool menerima lagi |
| Shutdown graceful, in-flight | `p1a: drain_with_in_flight_{persistent,oneshot}_completes_then_stops` | In-flight selesai 200, kerja baru ditolak cepat, worker berakhir Stopped, drain true dalam deadline |
| Cerita reload graceful | `p1a: reload_is_drain_then_rebuild` | Tanpa live reload (terdokumentasi): drain, rebuild, serve — dipin sebagai kontrak |
| Saturasi antre / overload | `m7_resilience`, `m1_pool`, `p0b: queue_bound_is_enforced` | Antre terbatas + 503 + metrics + recovery |
| Recycle ambang memori | Test soft/hard `p0c` (hog 32–64MB terkendali, `mem.php`) | Soft recycle graceful, hard terminate protektif, trigger terukur |
| Kebocoran state | `p1a: persistent_worker_state_leaks_are_visible...` (`leak.php`) | Reuse persistent menunjukkan `$GLOBALS` leak yang didokumentasikan; one-shot tetap bersih |
| Rute tak dikenal / 404 | `p0d_laravel`, `p0d_wordpress` | Body 404 framework dilayani sebagai 200 (batas CLI-SAPI, terdokumentasi) |

## Timeline recovery (pengukuran P1-A)

Dicatat per worker pada setiap restart tak direncanakan
(`WorkerSnapshot::last_recovery`,
`phpx_worker_last_recovery_duration_seconds{worker_id}`):

- `detected_at` — deteksi kegagalan (EOF/timeout exchange, temuan crash-idle, sampel hard-limit).
- `restart_started_at` — inisiasi restart (kill untuk proses lama).
- `ready_at` — pengganti ready (handshake fresh dalam `startup_timeout`).

Dipin oleh `p1a: recovery_timeline_reports_detection_to_ready`: tiap leg
dilaporkan terpisah dan harus memenuhi `total == detection→restart +
restart→ready`, dengan total di dalam budget startup. Time-to-first-success
dibuktikan oleh request setelah recovery yang kembali 200.

## Ambang penerimaan (RC-track, ditegakkan)

Lingkup dev satu-node; cap longgar agar menangkap regresi, bukan noise mesin.
Budget recovery berasal dari konfig pool di test (`startup_timeout` 15 dtk,
`execution_timeout` 10 dtk).

| Sinyal | Ambang | Ditegakkan oleh |
|---|---|---|
| Recovery detection -> inisiasi restart | <= 5_000 ms | `p1a_resilience::recovery_timeline_reports_detection_to_ready` |
| Recovery restart -> pengganti ready | <= 17_000 ms (15 dtk startup + 2 dtk slack) | test yang sama |
| Recovery total (detection -> ready) | <= 20_000 ms (15 dtk + 5 dtk slack) | test yang sama |
| Kill -> 500 gagal -> 200 pertama di pengganti | <= 30_000 ms (total + satu budget eksekusi 10 dtk) | `p1a_resilience::time_to_first_success_after_idle_crash_within_budget` |
| Recycle soft guardian: hog -> recycle -> 200 berikut | <= 30_000 ms (10 dtk exec + 15 dtk startup + 5 dtk slack) | `persistent::soft_limit_recycles_gracefully...` |
| Fail-fast sirkuit crash-loop | < 4_000 ms | `p1a_resilience::crash_loop_...` (eksisting) |
| Penolakan kerja baru saat drain | < 4_000 ms | `p1a_resilience::drain_with_in_flight_...` (eksisting) |
| Konservasi short soak (120 req, crash tiap ke-10) | completed + failed == total, restarts == 12, semua Ready, tanpa Failed | `p1a_resilience::short_soak_...` (eksisting) |
| Smoke bench-lab (CI) | failures == 0 | `bench-lab.sh --max-failures 0` (rps/p95 tetap input RC manual, bukan SLA lintas mesin) |

Slope RSS long-soak tetap observasi saja untuk saat ini (arsip 60 menit:
+3,6 KB/menit; run 6 jam 2026-11-25: -0,86 KB/menit, 73.600/73.600 tanpa
gagal): tren 24 jam masih diperlukan sebelum cap slope formal ditetapkan.

## Autoscale adaptif (P2, eksperimental opt-in)

Nonaktif default (`adaptive_enabled=false` menjaga pool tetap di atas tidak
berubah). Bila aktif, pool mulai di `min_workers`, tumbuh pada tekanan
berkelanjutan hingga `workers`, dan menyusut kembali saat idle.

- Sinyal: kedalaman antre + jumlah aktif vs kapasitas terspawn (saturasi =
  setiap worker terspawn sibuk dengan antrean di belakang); tekanan memori
  guardian (hit soft/hard terakhir); state sirkuit crash-loop; state drain.
- Pengaman: plafon (`workers`), lantai (`min_workers`, juga ukuran awal),
  satu worker per interval evaluasi, cooldown antar aksi, scale-up butuh
  `adaptive_scale_up_queue_depth` antrean di belakang pool jenuh, scale-down
  butuh `adaptive_scale_down_idle_intervals` evaluasi idle beruntun, tanpa
  scale-up di bawah tekanan memori atau sirkuit terbuka, tidak pernah saat
  draining, pensiun highest-id-first (id rendah tetap stabil).
- Hukum: fungsi murni `adaptive_decide` (matriks unit-test: saturate/up,
  hysteresis/down, blocked-hold, memory-blocks-up-only, heal-to-range).
- Observabilitas: `phpx_pool_workers_current`,
  `phpx_pool_adaptive_enabled{0,1}`,
  `phpx_pool_adaptive_scale_events_total{direction=up|down}`; spare tampil
  sebagai `stopped` di seri per-worker; `workers status` melaporkan tuning.
- Bukti: `p2_adaptive` (start lantai, tekanan -> plafon, idle -> lantai,
  jumlah event persis; pool tetap tidak berubah di bawah beban sama).

## Soak

- Gerbang CI (dapat diulang): `p1a: short_soak_mixed_workload_stays_healthy`
  (120 request, cepat + crash tiap ke-10, 2 worker): konservasi
  (`completed + failed == total`), akuntansi restart cocok dengan crash
  injeksi, semua worker Ready di akhir, tanpa Failed.
- Soak panjang manual: `scripts/soak-long.sh` (Linux/WSL) /
  `scripts/soak-long-windows.ps1` (Windows), plus arsip per run: log mentah +
  metadata lingkungan (OS, PHP `--version`, rustc, tanggal UTC) + konfig +
  snapshot `/metrics` + ringkasan hasil; simpan dataset mentah 30 hari
  (konvensi `docs/operations/baseline.md`). RSS gateway harus tetap flat;
  RSS worker mengikuti beban dan kebijakan guardian. Rekaman run: `soak.md`.
