#!/usr/bin/env bash
# PHPX M7 SBOM (best-effort, reproducible).
# Prefers syft (SPDX), falls back to cargo tree + cargo metadata.
set -euo pipefail
OUT="${1:-sbom-spdx.json}"
if command -v syft >/dev/null 2>&1; then
  syft packages dir:. -o spdx-json --output "$OUT"
  echo "SBOM via syft -> $OUT"
else
  cargo tree --charset ascii > sbom-cargo-tree.txt
  cargo metadata --format-version 1 --no-deps > sbom-cargo-metadata.json
  echo "syft not found; wrote sbom-cargo-tree.txt + sbom-cargo-metadata.json (fallback, not SPDX)"
  echo "Install syft for SPDX: https://github.com/anchore/syft"
fi
