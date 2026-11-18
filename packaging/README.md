# PHPX packaging (M7)

- `systemd/phpx.service`: native Linux unit (User=phpx, NoNewPrivileges, ProtectSystem=strict, PrivateTmp). Tune `MemoryMax/CPUQuota/TasksMax` per `performance.md` baseline.
- Release automation: `scripts/release.sh` (Linux) / `scripts/release-windows.ps1` (dev).
  - Builds release binary, runs `phpx doctor`, `cargo test -p phpx-server`, `cargo test -p phpx-cli`, `cargo clippy`, `cargo fmt --check`.
  - Emits `dist/phpx-<version>-<target>.tar.gz` (binary + `phpx.toml.example` + `packaging/systemd/phpx.service` + `docs/operations/{quickstart-linux,systemd,upgrade-rollback,backup-recovery}.md`).
  - SBOM: runs `scripts/sbom.sh` (syft SPDX or cargo-tree fallback) into `dist/`.
- Docker: `Dockerfile` (non-root `phpx:10001` + HEALTHCHECK). Harden at run time: `--read-only --tmpfs /tmp --cap-drop ALL`, mount `data/` volume for durable queue, never run `--privileged`.
