# PHPX M7 release automation (Windows dev).
$Ver = if ($args.Count -ge 1) { $args[0] } else { "0.1.0" }
$Target = (rustc -vV | Select-String "host:") -replace "host: ",""
$Dist = "dist/phpx-$Ver-$Target"
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p phpx-server --lib
cargo test -p phpx-server --test m7_security
cargo test -p phpx-server --test m7_resilience
cargo test -p phpx-cli
cargo build --release -p phpx-cli
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item target/release/phpx.exe "$Dist/" -Force
Copy-Item phpx.toml.example "$Dist/" -Force
Copy-Item packaging/systemd/phpx.service "$Dist/" -Force
powershell -File scripts/sbom-windows.ps1 "$Dist/sbom-spdx.json"
Write-Output "release ready: $Dist"
