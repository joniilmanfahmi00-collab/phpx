# Quickstart Linux native PHPX (pratinjau developer M3)

Target: Ubuntu 22.04+ / Debian 12+ (x86_64). Windows/macOS memakai WSL2 atau Docker (lihat quickstart-docker.md).
Status: pratinjau developer v0.1, bukan SLA produksi. Lihat limitations.md.

## 1. Prasyarat

```bash
sudo apt update && sudo apt install -y build-essential pkg-config git curl php-cli php-json php-mbstring php-curl php-sqlite3
rustup show   # toolchain ter-pin di rust-toolchain.toml (1.99.1)
php --version # 8.3 / 8.4 / 8.5, lihat php-version-matrix.md
```

Daftar lengkap: linux-prerequisites.md.

## 2. Build + doctor

```bash
git clone <phpx-repo> && cd phpx
cargo build -p phpx-cli
./target/debug/phpx doctor
# harapkan: phpx doctor (pratinjau developer M3), semua OK
```

## 3. Sajikan fixture vanilla

```bash
cp phpx.toml.example phpx.toml
# edit phpx.toml bila perlu: root="examples/vanilla-php", document_root="public"
./target/debug/phpx serve --config phpx.toml
# atau: ./target/debug/phpx serve --config examples/vanilla-php-toml? Pakai:
# ./target/debug/phpx serve --host 127.0.0.1 --port 8080 --config phpx.toml
```

Di terminal lain:

```bash
curl -i http://127.0.0.1:8080/healthz   # -> 200 ok (phpx v0.1 developer preview)
curl -i http://127.0.0.1:8080/readyz    # -> 200 ready
curl -i http://127.0.0.1:8080/          # -> 200 fixture vanilla (front controller)
curl -i http://127.0.0.1:8080/style.css # -> 200 statis, MIME benar
curl -i http://127.0.0.1:8080/hello.php # -> 200 didispatch, tidak pernah source mentah
curl -i http://127.0.0.1:8080/%2e%2e/x  # -> 400 bad request (traversal diblokir)
```

Log menampilkan baris akses terstruktur berisi `request_id, method, path, status, duration_ms` dan header respons `x-request-id`.

## 4. Perintah status

```bash
./target/debug/phpx runtime list
./target/debug/phpx runtime inspect
./target/debug/phpx workers status
./target/debug/phpx workers reload   # memvalidasi config; restart serve untuk menerapkan
./target/debug/phpx metrics
```

`queue status|work` dan `scheduler run` berjalan sejak M5 (exit 0 + catatan).

## 5. Systemd (opsional, mesin dev)

Lihat systemd.md + packaging/systemd/phpx.service:

```bash
sudo cp packaging/systemd/phpx.service /etc/systemd/system/phpx.service
sudo systemctl daemon-reload
sudo systemctl restart phpx
systemctl status phpx
journalctl -u phpx -f
```

## 6. Smoke + bench

```bash
bash scripts/smoke-linux.sh
bash scripts/bench.sh --requests 200 --concurrency 10
```

Harapan: smoke lolos (healthz, front-controller, statis, traversal 400, crash 500 lalu 200, backpressure 503 saat beban).
Output benchmark hanya baseline reprodusibel, bukan klaim performa.

## Troubleshooting

- `config error ... (path: ...)` -> salin contoh, cek TOML, lihat configuration.md.
- `cannot bind ...` -> port dipakai; ubah `server.port` atau `phpx serve --port 9001`.
- `php not found` -> instal PHP atau set `PHPX_PHP_BIN=/usr/bin/php8.4`.
- `tls=true requires ...` -> set `tls=false` untuk dev lokal atau sediakan cert+key PEM.
