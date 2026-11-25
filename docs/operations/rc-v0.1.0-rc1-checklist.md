# Checklist PHPX v0.1.0-rc1 (dibangun di Windows, target prod Linux)

> Status: tag `v0.1.0-rc1` adalah penanda developer-preview dan mendahului
> penilaian gerbang (lihat `release-gates.md`). Checklist ini historis untuk
> build rc1; untuk status kini lihat `release-gates.md` + `performance.md`.

## Build

- version: 0.1.0-rc1 (binary melaporkan 0.1.0, tag rc = git tag v0.1.0-rc1, tanpa ubah kode)
- host: x86_64-pc-windows-msvc, rustc 1.99.1, php 8.3.30 ZTS cli
- binary: target/release/phpx.exe (~10 MB, doctor hijau 16/16)
- fmt: cargo fmt --check bersih, clippy bersih

## Test (wajib hijau sebelum tag)

- lib 5/5, m4_compat 5/5, m7_security 4/4, m7_resilience 2/2, cli 6/6, queue 6/6
- bench-windows 10 req cold_ms tercatat, soak-short opsional

## Dist (dist/phpx-0.1.0-rc1-x86_64-pc-windows-msvc/)

- phpx.exe + phpx.toml.example + packaging/systemd/phpx.service
- docs: quickstart-linux.md, upgrade-rollback.md, backup-recovery.md, baseline.md, performance.md, limitations.md
- sbom: sbom-cargo-tree.txt + sbom-cargo-metadata.json (syft SPDX bila ada di CI Linux)
- VERSION.txt + CHECKSUMS.sha256

## Tag (manual)

- git tag -a v0.1.0-rc1 -m "PHPX v0.1.0-rc1: M7 exit 6/6, OTel-ready, bounded RC"
- git push origin v0.1.0-rc1
- GitHub Release: upload dist tar.gz + sbom, tempel checklist ini + delta RC limitations.md

## Post-tag (CI Linux)

- rebuild Linux x86_64-unknown-linux-gnu + docker build + smoke-docker.sh
- syft SPDX asli, simpan 30 hari bareng log bench
