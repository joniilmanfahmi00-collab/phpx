# Matriks versi PHP yang didukung PHPX (pratinjau developer M3)

`phpx doctor` melaporkan `php --version` aktual dan menandai `8.3 / 8.4 / 8.5` sebagai target.
`phpx runtime list|inspect` menampilkan `php_target` (dari config) vs `php_actual` (dari PATH atau `PHPX_PHP_BIN`).

## Matriks uji M3

| PHP | Status (M3) | Catatan |
|-----|-------------|-------|
| 8.3 | Didukung, teruji-CI bila tersedia | `phpx doctor` -> yes. Fixture vanilla lolos. |
| 8.4 | Didukung, default `php_version = "8.4"` | Basis runtime Docker `php:8.4-cli-bookworm`. Fixture vanilla lolos. |
| 8.5 | Best-effort | `doctor` menandai yes bila string `8.5` ditemukan; matriks CI penuh adalah hardening M4. |
| < 8.3 (8.1, 8.2, 7.x) | Tak terverifikasi | `doctor` mencetak `unverified`. Mungkin jalan untuk fixture vanilla tetapi tidak diklaim. Ajukan isu beserta output `phpx doctor --json`. |
| Fork PHP / build kustom | Tak terverifikasi | Wajib menyediakan CLI `php` di PATH atau set `PHPX_PHP_BIN`. NTS/ZTS keduanya ditoleransi di M3 (CLI one-shot); audit SAPI embed adalah M4/M6. |

## Ekstensi (M3)

Cek `phpx doctor` (peringatan, tidak fatal kecuali `php` sendiri hilang):

- `json`, `mbstring`, `curl`, `openssl`, `pdo` -> `found` atau `missing (warning)`.

Fixture vanilla hanya butuh inti. Fixture framework (Composer/Laravel/WordPress) adalah pekerjaan kompatibilitas M4 dan akan memperluas tabel ini dengan ekstensi wajib per app.

## Cara memilih PHP

```bash
php --version
PHPX_PHP_BIN=/usr/bin/php8.4 phpx doctor
PHPX_PHP_BIN=/usr/bin/php8.4 phpx serve --config phpx.toml
```

Di Docker, image mem-pin PHP via `php:8.4-cli-bookworm`. Cek dengan:

```bash
docker run --rm phpx:0.1.0-dev php --version
```

## Prospek M4 (bukan M3)

- Publikasikan hasil CI per-versi (8.3/8.4/8.5 x ekstensi).
- Dokumentasikan gap SAPI/ekstensi untuk Laravel/WordPress.
- Tanpa peringkat performa per versi PHP sampai benchmark baseline reprodusibel ada (lihat scripts/bench.sh).
