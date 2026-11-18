#!/usr/bin/env bash
# PHPX M7 release automation (Linux).
set -euo pipefail
VER="${1:-0.1.0}"
TARGET="$(rustc -vV | sed -n 's/host: //p')"
DIST="dist/phpx-${VER}-${TARGET}"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p phpx-server --lib
cargo test -p phpx-server --test m7_security
cargo test -p phpx-server --test m7_resilience
cargo test -p phpx-cli
cargo build --release -p phpx-cli
mkdir -p "$DIST"
cp target/release/phpx "$DIST/"
cp phpx.toml.example "$DIST/"
cp packaging/systemd/phpx.service "$DIST/"
./scripts/sbom.sh "$DIST/sbom-spdx.json" || true
tar -czf "$DIST.tar.gz" -C dist "$(basename "$DIST")"
echo "release ready: $DIST.tar.gz"
