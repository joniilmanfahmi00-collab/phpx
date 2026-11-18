# Linux development prerequisites (M0)

Target: Ubuntu 22.04+ / Debian 12+ (x86_64), Docker optional.

## 1. Rust (pinned)

- Install via rustup: https://rustup.rs
- Pinned toolchain: see `rust-toolchain.toml` (currently 1.99.1).
- Components: `rustfmt`, `clippy`.
- Target for cross-checks: `x86_64-unknown-linux-gnu` (default on Linux).

```bash
rustup show
rustup component add rustfmt clippy
```

## 2. PHP (8.3 / 8.4 / 8.5)

```bash
php --version
php -m
```

- Required for `phpx doctor`: `php` in PATH or set `PHPX_PHP_BIN`.
- Useful extensions for M0 doctor: `json mbstring curl openssl pdo`.
- Full extension matrix is validated per release (see SPEC 2.2).

## 3. System tools

```bash
sudo apt update && sudo apt install -y build-essential pkg-config git curl
```

Docker (for container parity, ADR-0005):

```bash
docker --version
```

## 4. M0 checks

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./target/debug/phpx doctor
./target/debug/phpx serve --config phpx.toml.example
```

Windows/macOS dev may use WSL2 or Docker Desktop; production target stays Linux/Docker.
