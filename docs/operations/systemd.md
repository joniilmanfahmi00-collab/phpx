# PHPX systemd guidance (M3 developer preview)

Unit file: `packaging/systemd/phpx.service`. Status: developer preview, not a production SLA. See limitations.md.

## Install (Ubuntu 22.04+ / Debian 12+)

```bash
# 1. user + dirs
sudo useradd -r -m -s /usr/sbin/nologin phpx
sudo mkdir -p /opt/phpx /etc/phpx /var/log/phpx /opt/phpx/data
sudo chown -R phpx:phpx /opt/phpx /var/log/phpx

# 2. binary (from release build)
cargo build --release -p phpx-cli
sudo cp target/release/phpx /usr/local/bin/phpx
phpx doctor

# 3. app + config
# example: vanilla fixture
sudo cp -r examples/vanilla-php /opt/phpx/examples-vanilla
sudo cp phpx.toml.example /etc/phpx/phpx.toml
sudo chown -R phpx:phpx /etc/phpx
# edit /etc/phpx/phpx.toml:
#   [application] root = "/opt/phpx/examples-vanilla", document_root = "public"
#   [server] host = "127.0.0.1", port = 8080

# 4. unit
sudo cp packaging/systemd/phpx.service /etc/systemd/system/phpx.service
# edit WorkingDirectory + ExecStart --config to match your paths
sudo systemctl daemon-reload
sudo systemctl enable --now phpx
systemctl status phpx
journalctl -u phpx -f
curl -i http://127.0.0.1:8080/healthz
```

## Behavior

- `KillSignal=SIGINT` triggers graceful drain (same as Ctrl-C): stops accepting, in-flight PHP bounded by `request_timeout_seconds`, then exits.
- `TimeoutStopSec=40` should exceed `request_timeout_seconds` (default 30s).
- `Restart=on-failure` restarts crashed gateway; PHP worker crashes do NOT restart the unit (they map to HTTP 500, gateway stays up).
- `phpx workers reload` only validates config in M3. Apply changes via `sudo systemctl restart phpx`. Live SIGHUP reload is M4.

## Hardening notes

Unit sets `NoNewPrivileges=true`, `PrivateTmp=true`, `ProtectSystem=strict`, `ProtectHome=true`, `ReadWritePaths=/opt/phpx/data /var/log/phpx`.
Tune for your distro. M3 does not claim strong multi-tenant isolation; treat app code + PHP extensions as trusted (SPEC 10).

## Logs + health

```bash
journalctl -u phpx -f
# structured access logs: request_id, method, path, status, duration_ms
curl -i http://127.0.0.1:8080/healthz  # liveness
curl -i http://127.0.0.1:8080/readyz   # readiness (docroot + entrypoint check)
phpx --config /etc/phpx/phpx.toml workers status
phpx --config /etc/phpx/phpx.toml metrics
```
