# Matriks deployment PHPX (M7 RC)

| Target | Status | Basis | User | Health | Catatan |
|--------|--------|------|------|--------|-------|
| Docker `phpx:0.1.0-dev` | Didukung (jalur RC primer) | `php:8.4-cli-bookworm` + builder rust | `phpx (10001)`, tidak pernah root | `HEALTHCHECK /healthz` 15s/3s/3x | Jalankan `--cpus=2 --memory=1g` agar terbatas; mount volume `phpx.toml` + `data/` untuk queue tahan-lama |
| Linux native (systemd) | Didukung | `rust 1.99 + php-cli 8.3/8.4/8.5` | `User=phpx`, `NoNewPrivileges`, `ProtectSystem=strict` | `curl /readyz` di `ExecStartPost` atau timer | Lihat `packaging/systemd/phpx.service` + `quickstart-linux.md`; set `MemoryMax/CPUQuota` |
| Windows dev | Hanya dev/pengujian | `cargo + php 8.3 ZTS` | user saat ini | `Invoke-RestMethod /healthz` | Tanpa penegakan cgroup; pakai batas `queue_bound` + `workers` + `bench-windows.ps1`. Embed native tidak tersedia (butuh link NTS Linux). |

Diverifikasi oleh: `m7_dockerfile_runs_non_root` (USER/HEALTHCHECK/uid), `m4_compat` (fixture), `m7_resilience` (saturasi 200/503/504 + pemulihan), `smoke-*.sh/ps1`.
