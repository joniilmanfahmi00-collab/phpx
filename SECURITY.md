# Security Policy — PHPX

## Status proyek

PHPX v0.1 adalah **developer preview**, bukan rilis produksi dengan SLA.
Fokus keamanan tahap ini: isolasi proses dasar, batasan request/header,
timeout, penanganan secret yang aman, dan IPC yang terkendali.
Lihat `docs/operations/security.md` dan `docs/operations/limitations.md`
untuk batasan yang diketahui.

## Versi yang didukung (Supported Versions)

| Versi | Status | Dukungan keamanan |
| ----- | ------ | ----------------- |
| 0.1.x (preview, `v0.1.0-rc1`) | Didukung | Best-effort, patch di rilis preview berikutnya |
| < 0.1 (snapshot awal / fork lama) | Tidak didukung | Silakan upgrade ke 0.1.x |

Belum ada LTS. Setelah 1.0 stabil, tabel ini akan diperbarui dengan
kebijakan patch dan backport yang jelas.

## Cara melaporkan kerentanan (Reporting a Vulnerability)

**Jangan buka issue publik untuk kerentanan yang belum ditangani.**

Laporkan melalui salah satu jalur privat:

1. GitHub **Private vulnerability reporting** (bila diaktifkan di repo ini), atau
2. Email pemilik proyek (lihat profil GitHub pemilik repo),
   sertakan subjek `[PHPX SECURITY]`.

Sertakan dalam laporan:

- Deskripsi dampak dan skenario eksploitasi yang jelas.
- Versi/commit yang terpengaruh (`phpx --version`, OS, PHP build).
- Langkah reproduksi minimal (PoC) dan/atau log yang relevan.
- Kontak untuk tindak lanjut.

Komitmen kami:

- Konfirmasi penerimaan laporan maksimal **72 jam**.
- Penilaian awal dan rencana tindak lanjut maksimal **7 hari**.
- Koordinasi tanggal publikasi patch bila diperlukan.
- Kredit pelapor (bila diinginkan) pada advisory rilis.

## Cakupan (Scope)

Yang termasuk cakupan:

- Rust control plane / gateway (`crates/*`), IPC, worker supervision.
- Konfigurasi contoh, Dockerfile, dan systemd unit di repo ini.
- Path traversal, batasan body/header, timeout, dan penanganan secret
  yang didokumentasikan di `docs/operations/security.md`.

Yang di luar cakupan awal (tetap diterima sebagai laporan kualitas):

- Aplikasi PHP contoh (`examples/*`) yang sengaja minimal.
- Dependency pihak ketiga — laporkan ke upstream, beri tahu kami juga.
- Social engineering dan akses fisik.

## Penanganan secret

- Jangan pernah menaruh password, token, atau private key di issue,
  PR, log, atau file contoh yang di-commit.
- Gunakan environment variable atau secret manager; contoh konfigurasi
  memakai placeholder (`phpx.toml.example`).
- Output `phpx doctor` dirancang bebas secret — bila menemukan secret
  bocor di log, laporkan sebagai bug keamanan.

## Setelah rilis 1.0 (rencana)

- Kebijakan severity (CVSS) dan SLA patch yang eksplisit.
- Advisory terstruktur (GHSA) untuk setiap CVE yang relevan.
- Verifikasi tanda tangan artefak rilis (checksum + cosign/sigstore).
