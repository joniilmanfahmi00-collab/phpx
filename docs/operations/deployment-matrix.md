# PHPX deployment matrix (M7 RC)

| Target | Status | Base | User | Health | Notes |
|--------|--------|------|------|--------|-------|
| Docker `phpx:0.1.0-dev` | Supported (primary RC path) | `php:8.4-cli-bookworm` + rust builder | `phpx (10001)`, never root | `HEALTHCHECK /healthz` 15s/3s/3x | Run `--cpus=2 --memory=1g` for bound; mount `phpx.toml` + `data/` volume for durable queue |
| Native Linux (systemd) | Supported | `rust 1.99 + php-cli 8.3/8.4/8.5` | `User=phpx`, `NoNewPrivileges`, `ProtectSystem=strict` | `curl /readyz` in `ExecStartPost` or timer | See `packaging/systemd/phpx.service` + `quickstart-linux.md`; set `MemoryMax/CPUQuota` |
| Windows dev | Dev/test only | `cargo + php 8.3 ZTS` | current user | `Invoke-RestMethod /healthz` | No cgroup enforcement; use `queue_bound` + `workers` bounds + `bench-windows.ps1`. Native embed unavailable (needs Linux NTS link). |

Verified by: `m7_dockerfile_runs_non_root` (USER/HEALTHCHECK/uid), `m4_compat` (fixtures), `m7_resilience` (saturation 200/503/504 + recovery), `smoke-*.sh/ps1`.
