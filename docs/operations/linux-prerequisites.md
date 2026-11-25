# Prasyarat pengembangan Linux (M0)

Target: Ubuntu 22.04+ / Debian 12+ (x86_64), Docker opsional.

## 1. Rust (ter-pin)

- Instal via rustup: https://rustup.rs
- Toolchain ter-pin: lihat `rust-toolchain.toml` (saat ini 1.99.1).
- Komponen: `rustfmt`, `clippy`.
- Target untuk cek-silangan: `x86_64-unknown-linux-gnu` (default di Linux).

```bash
rustup show
rustup component add rustfmt clippy
```

## 2. PHP (8.3 / 8.4 / 8.5)

```bash
php --version
php -m
```

- Diperlukan untuk `phpx doctor`: `php` di PATH atau set `PHPX_PHP_BIN`.
- Ekstensi berguna untuk doctor M0: `json mbstring curl openssl pdo`.
- Matriks ekstensi penuh divalidasi per rilis (lihat SPEC 2.2).

## 3. Perkakas sistem

```bash
sudo apt update && sudo apt install -y build-essential pkg-config git curl
```

Docker (untuk paritas container, ADR-0005):

```bash
docker --version
```

## 4. Cek M0

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./target/debug/phpx doctor
./target/debug/phpx serve --config phpx.toml.example
```

Dev Windows/macOS boleh memakai WSL2 atau Docker Desktop; target produksi tetap Linux/Docker.
