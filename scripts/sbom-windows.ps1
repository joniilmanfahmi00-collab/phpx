# PHPX M7 SBOM (Windows fallback).
# Prefers syft (SPDX), falls back to cargo tree + cargo metadata.
$Out = if ($args.Count -ge 1) { $args[0] } else { "sbom-spdx.json" }
if (Get-Command syft -ErrorAction SilentlyContinue) {
  syft packages dir:. -o spdx-json --output $Out
  Write-Output "SBOM via syft -> $Out"
} else {
  cargo tree --charset ascii | Out-File -Encoding utf8 sbom-cargo-tree.txt
  cargo metadata --format-version 1 --no-deps | Out-File -Encoding utf8 sbom-cargo-metadata.json
  Write-Output "syft not found; wrote sbom-cargo-tree.txt + sbom-cargo-metadata.json (fallback)"
}
