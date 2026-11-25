# Catatan kandidat produksi M7 PHPX (fase 1 SELESAI)

## Tujuan

Gerbang RC: tanpa isu sec kritis, saturasi terbatas + teramati, pemulihan terdokumentasi + teruji, klaim reprodusibel.

## Fase 1 SELESAI (perubahan ini)

- Model ancaman + batas dipublikasikan (`docs/operations/security.md`).
- Helper redaksi secret + uji unit (`is_sensitive_header`, `redacted_header_value`, `extract_traceparent`).
- Uji integrasi batas (`m7_security`: traversal encode mentah via raw-socket 400, sensitive-header soak, symlink escape 403/404, cek statis Dockerfile non-root).
- Dokumen observabilitas (`metrics.md`: deret /healthz /readyz /metrics + siaga; catatan OTel-ready, tanpa exporter di v0.1).
- Dokumen ops (`upgrade-rollback.md`, `backup-recovery.md`).

Bukti: `cargo test -p phpx-server --lib` (5 ok termasuk 3 m7), `cargo test -p phpx-server --test m7_security` (4 ok).

## Sisa (fase 2)

- Uji batas cgroup (saturasi adversarial -> 503 terbatas, tanpa antre tanpa batas).
- Kait reprodusibel bench/soak (`bench.sh` baseline + arsip snapshot `/metrics`).
- Hardening Docker + skrip SBOM (`scripts/sbom.sh` + ps1 windows, fallback syft-or-cargo-tree).
- Otomasi paket native (`packaging/` deb/rpm/tar + checklist rilis).
- Kotak Roadmap M7 + kata RC limitations + matriks compatibility/deployment dipublikasikan.

## Fase 2 SELESAI (perubahan ini)

- Korelasi OTel-ready: `traceparent` diekstrak di `handle_one` + `dispatch_php`, ditambahkan ke setiap access log (`request_id + traceparent`), diteruskan ke env PHP. Tanpa dep exporter.
- Ketahanan: `m7_resilience` (workers=2/queue=2 vs 8x `sleep.php` 5s) tetap 200/503/504 + deret pool `/metrics` + pemulihan endpoint-cepat + `x-request-id`. Gate config ditegaskan ulang (`workers=0`/`queue_bound=0` ditolak).
- Ops: `deployment-matrix.md` (Docker/systemd/Windows), `performance.md` (bench repro + daftar arsip + bar RC), `packaging/README.md` + `release.sh/ps1` (fmt/clippy/tests/build/dist+SBOM tarball).
- Kata RC: delta `limitations.md` M7 + tabel `compatibility-matrix.md` M7.

Bukti: lib 5 ok, m7_security 4 ok, m7_resilience 2 ok, cli 6 ok, clippy bersih, fmt bersih.
