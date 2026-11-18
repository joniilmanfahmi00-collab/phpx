# PHPX v0.1.0-rc1 checklist (Windows built, Linux prod target)

## Build

- version: 0.1.0-rc1 (binary reports 0.1.0, rc tag = git tag v0.1.0-rc1, no code change)
- host: x86_64-pc-windows-msvc, rustc 1.99.1, php 8.3.30 ZTS cli
- binary: target/release/phpx.exe (9.8 MB, doctor green 16/16)
- fmt: cargo fmt --check clean, clippy clean

## Tests (wajib hijau sateuacan tag)

- lib 5/5, m4_compat 5/5, m7_security 4/4, m7_resilience 2/2, cli 6/6, queue 6/6
- bench-windows 10 req cold_ms tercatat, soak-short opsional

## Dist (dist/phpx-0.1.0-rc1-x86_64-pc-windows-msvc/)

- phpx.exe + phpx.toml.example + packaging/systemd/phpx.service
- docs: quickstart-linux.md, upgrade-rollback.md, backup-recovery.md, baseline.md, performance.md, limitations.md
- sbom: sbom-cargo-tree.txt + sbom-cargo-metadata.json (syft SPDX lamun aya di CI Linux)
- VERSION.txt + CHECKSUMS.sha256

## Tag (manual, Neng teu commit/tag otomatis)

- git tag -a v0.1.0-rc1 -m "PHPX v0.1.0-rc1: M7 exit 6/6, OTel-ready, bounded RC"
- git push origin v0.1.0-rc1
- GitHub Release: upload dist tar.gz + sbom, tempel checklist ieu + limitations.md RC delta

## Post-tag (Linux CI)

- rebuild Linux x86_64-unknown-linux-gnu + docker build + smoke-docker.sh
- syft SPDX asli, simpen 30 hari bareng bench log
