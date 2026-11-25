# Gerbang rilis PHPX — penilaian bukti (pasca P1-B + integrasi + P2 serve path + soak 6 jam)

Dinilai terhadap `docs/phpx-engineering-review-remediation-plan.md` §7.
Vonis: DONE / PARTIAL / NOT DONE. Bukti merujuk ke suite, skrip, dok.

## Developer preview: DONE

- Jalur gateway dan eksekusi dasar bekerja (one-shot + persistent, permukaan
  queue/scheduler/cache/storage, TLS, drain, health/readiness).
- Keterbatasan yang diketahui didokumentasikan (`limitations.md`,
  `sapi-audit.md`, `compatibility-matrix.md`); tidak ada SLA produksi yang
  diisyaratkan di mana pun yang menghadap pengguna.
- Bukti: `cargo test --workspace` hijau, `scripts/smoke-*.sh/ps1` hijau,
  `phpx doctor` hijau.

## Alpha: DONE

| Kriteria | Vonis | Bukti |
|---|---|---|
| Siklus hidup worker tetap deterministik | DONE | Mesin state P0-B (`Starting→Ready→Busy→Ready`, drain, `Failed` terminal) + unit test `lifecycle_transitions_are_deterministic`, `failed_is_terminal_except_shutdown` |
| Antrean terbatas dan backpressure teruji | DONE | `m7_resilience` (saturasi + overload + metrics + recovery), `m1_pool`, `p0b: queue_bound_is_enforced` |
| Crash dan restart worker teramati | DONE | Snapshot per-worker (state/PID/inkarnasi/alasan keluar), seri `phpx_worker_*`, timeline recovery (`last_recovery`, `phpx_worker_last_recovery_duration_seconds`), test crash/idle/timeout/sirkuit P1-A |
| Uji kompatibilitas inti lolos | DONE | Suite fixture (`m4_compat` termasuk `$_POST` urlencoded + multipart `$_POST`/`$_FILES`) + Laravel 12 asli (`p0d_laravel` 5/5) + WordPress 7.1 asli (`p0d_wordpress` 13/13 termasuk WooCommerce) |

## Beta: DONE (lingkup developer satu-node; tren 24 jam tetap pekerjaan lanjutan)

| Kriteria | Vonis | Bukti / celah |
|---|---|---|
| Resource guardian diimplementasikan dan teruji | DONE | P0-C: kebijakan OS-RSS soft (recycle) / hard (terminate), sampler idle + sampling saat request selesai, metrik memori + lifecycle per-worker, test soft/hard/sampler dengan beban terkendali, `memory_limit` ditegakkan sebagai batas heap PHP `-d` |
| Smoke/integration test Laravel, WordPress + WooCommerce asli ada | DONE (dengan batas SAPI eksplisit) | Smoke + integrasi: routing Laravel, JSON error, session sisi-server, multipart (`p0d_laravel` 5/5); REST WordPress, alur login, round-trip komentar, rantai auth media (`p0d_wordpress` 13/13 termasuk loop toko WooCommerce, halaman produk, dok redirect cart/checkout, `wc/v3` katalog JSON); `$_POST` urlencoded + multipart di level fixture (`m4_compat`). Batas eksplisit: `move_uploaded_file()` tetap false di CLI, propagasi status/header PHP terbatas (terdokumentasi di `sapi-audit.md`; upload *bytes* kini diparsing — `$_POST`/`$_FILES` terisi). Catatan: satu test multipart sempat merah di mount WSL 9p (stall I/O `p9_client_rpc`, terbukti lolos di ext4/Windows) — artefak lingkungan, bukan regresi |
| Soak dan failure-injection dapat diulang | DONE (lingkup satu-node) | Failure-injection otomatis: matriks P1-A (`p1a_resilience` 9/9 termasuk app-exit resync) plus suite P0-B/P0-C. Soak panjang terarsip: 60 menit (`data/bench/soak-long-persistent-release/`: 12.600/12.600, RSS flat +3,6 KB/menit, 25 recycle graceful, 0 error) DAN 6 jam (`data/bench/soak-6h-persistent-release-win/`: 73.600/73.600, gagal 0, timeout 0, ditolak 0, RSS 17.744 -> 17.436 KB, slope -0,86 KB/menit, 147 recycle-budget, 0 sirkuit) — lihat `SUMMARY.md` di tiap arsip dan `soak.md`. Tren 24 jam tetap pekerjaan lanjutan, bukan penghambat Beta |
| Keterbatasan keamanan dan deployment didokumentasikan | DONE | `security.md` (model ancaman, batas, aturan secret), `deployment-matrix.md` (Docker/systemd/Windows), `limitations.md` sesuai keadaan kini, `sapi-audit.md` (batas CLI-SAPI) |
| Jalur serve embedded terpagar dan terkabel | DONE (eksperimental, opt-in) | 9/9 gerbang promosi lolos di build linked (`gates::assess`, `available=true`); `EmbeddedPool` single-flight + timeout eksplisit terkabel ke gateway (`runtime.mode=embedded`, `p2_embedded_gateway` 5/5 di stub dan linked); stub fail-closed; default tetap `process` |

Beta terpenuhi pada lingkup developer satu-node: suite integrasi di atas tetap
hijau di CI (instalasi terprovisi), arsip soak 60 menit + 6 jam ada per
konvensi baseline (`data/bench/`), dan serve path embedded terkabel dengan
fail-closed yang jujur.

## Release candidate: NOT DONE

- Benchmark adalah mesin developer, satu-node; lab P1-B menambah angka release
  plus metodologi pengulangan: `bench-repeat.sh/.ps1` mengagregat N run menjadi
  `reproducibility.json` dengan gerbang CV. Rekor RC itu sendiri (release,
  workload 3x200/10 yang dikunci, hardware RC) dan SLO beban/soak formal masih
  pekerjaan lanjutan — namun bukti soak 6 jam kini ada (73,6k request, 0 gagal,
  slope -0,86 KB/menit, di mesin dev; bukan hardware RC).
- Perilaku recovery dan memori kini punya ambang penerimaan (lihat
  `resilience.md`: cap recovery per-leg, cap kill->first-200, cap recycle
  guardian, cap fail-fast sirkuit/drain, konservasi soak, bench-lab
  `--max-failures 0` di CI). Rekor reprodusibilitas multi-run dan SLO
  (floor rps/p95, cap slope RSS) masih pekerjaan lanjutan.
- Matriks kompatibilitas menandai sisa batas SAPI dengan test yang dipin;
  session/JSON/form/komentar/REST tercakup di app asli.
- Tidak ada isu keamanan kritis yang belum selesai; cerita integritas data
  adalah dok upgrade-rollback + backup-recovery (restore milik operator).

## Stabil 1.0: NOT DONE

- Semua klaim 1.0 butuh bukti test (celah Beta dulu).
- Prosedur upgrade/rollback ada sebagai dok; hardening produksi
  (isolasi multi-tenant, paritas WAF, encrypted-at-rest) di luar lingkup.
- Tag `v0.1.0-rc1` mendahului penilaian ini dan tidak menyiratkan gerbang RC
  di atas; perlakukan sebagai penanda developer-preview, bukan klaim rilis.
