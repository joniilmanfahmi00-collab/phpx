# PHPX

**Platform aplikasi PHP bertenaga Rust.**

PHPX bertujuan menyediakan runtime terpadu dan platform infrastruktur untuk
aplikasi PHP yang sudah ada, dengan bidang kendali Rust dan arsitektur
eksekusi PHP hibrida.

## Arah proyek

-   **Produk:** PHPX
-   **Bahasa:** Rust
-   **Deployment:** Linux nativ dan Docker
-   **Kompatibilitas:** Baseline ketat; aplikasi PHP biasa tidak boleh
    membutuhkan perubahan sumber
-   **Runtime:** Hybrid Process Runtime dan Embedded Runtime eksperimental
-   **Integrasi awal:** Vanilla PHP, Laravel, WordPress, WooCommerce

## Status

**PHPX v0.1 developer preview** — server aplikasi PHP minimal yang berguna
(M1-M5 + P0-P2 selesai; serve path embedded terkabel, opt-in).
Bukan SLA produksi. Lihat `docs/operations/limitations.md`.

Bukti terbaru (2026-11-25): soak 6 jam persistent/release — 73.600/73.600
request, gagal 0, RSS gateway flat (slope -0,86 KB/menit). Lihat
`docs/operations/soak.md` dan `docs/operations/release-gates.md`.

Quickstart:

- Linux nativ: `docs/operations/quickstart-linux.md`
- Docker (non-root): `docs/operations/quickstart-docker.md`
- Default config: `docs/operations/configuration.md`
- Matriks PHP: `docs/operations/php-version-matrix.md`
- systemd: `docs/operations/systemd.md` + `packaging/systemd/phpx.service`

Health: `GET /healthz` (liveness), `GET /readyz` (readiness). CLI: `phpx doctor`, `phpx serve`, `phpx runtime list|inspect`, `phpx workers status|reload`, `phpx metrics`.

## Dokumentasi

-   [Spesifikasi Teknis v0.1](docs/TECHNICAL_SPEC_v0.1.md)
-   [Roadmap](roadmap.md)
-   [Catatan Keputusan Arsitektur](docs/adr/README.md)
-   [Penilaian gerbang rilis](docs/operations/release-gates.md)
-   [Laporan benchmark (Inggris, edisi developer)](docs/PHPX-v0.1.0-rc1-benchmark-report-en.md)

## Prinsip pengembangan awal

1.  Process Runtime dulu.
2.  Embedded Runtime tetap eksperimental hingga terbukti aman dan
    kompatibel.
3.  PHP tetap bahasa aplikasi.
4.  Rust mengelola infrastruktur dan lifecycle.
5.  Batasi semua antre, worker, dan pemakaian resource.
6.  Uji aplikasi PHP asli, bukan hanya benchmark sintetis.

## Lisensi

MIT License — lihat `LICENSE`.
