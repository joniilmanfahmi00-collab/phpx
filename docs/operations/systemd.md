# Panduan systemd PHPX (pratinjau developer M3)

File unit: `packaging/systemd/phpx.service`. Status: pratinjau developer, bukan SLA produksi. Lihat limitations.md.

## Instal (Ubuntu 22.04+ / Debian 12+)

```bash
# 1. user + direktori
sudo useradd -r -m -s /usr/sbin/nologin phpx
sudo mkdir -p /opt/phpx /etc/phpx /var/log/phpx /opt/phpx/data
sudo chown -R phpx:phpx /opt/phpx /var/log/phpx

# 2. binary (dari build rilis)
cargo build --release -p phpx-cli
sudo cp target/release/phpx /usr/local/bin/phpx
phpx doctor

# 3. app + config
# contoh: fixture vanilla
sudo cp -r examples/vanilla-php /opt/phpx/examples-vanilla
sudo cp phpx.toml.example /etc/phpx/phpx.toml
sudo chown -R phpx:phpx /etc/phpx
# edit /etc/phpx/phpx.toml:
#   [application] root = "/opt/phpx/examples-vanilla", document_root = "public"
#   [server] host = "127.0.0.1", port = 8080

# 4. unit
sudo cp packaging/systemd/phpx.service /etc/systemd/system/phpx.service
# edit WorkingDirectory + ExecStart --config sesuai path Anda
sudo systemctl daemon-reload
sudo systemctl enable --now phpx
systemctl status phpx
journalctl -u phpx -f
curl -i http://127.0.0.1:8080/healthz
```

## Perilaku

- `KillSignal=SIGINT` memicu drain graceful (sama seperti Ctrl-C): berhenti menerima, PHP in-flight dibatasi oleh `request_timeout_seconds`, lalu keluar.
- `TimeoutStopSec=40` sebaiknya melebihi `request_timeout_seconds` (default 30s).
- `Restart=on-failure` me-restart gateway yang crash; crash worker PHP TIDAK me-restart unit (dipetakan ke HTTP 500, gateway tetap menyala).
- `phpx workers reload` hanya memvalidasi config. Terapkan perubahan via `sudo systemctl restart phpx`. Reload SIGHUP live belum didukung (ditolak eksplisit di CLI).

## Catatan hardening

Unit menyetel `NoNewPrivileges=true`, `PrivateTmp=true`, `ProtectSystem=strict`, `ProtectHome=true`, `ReadWritePaths=/opt/phpx/data /var/log/phpx`.
Sesuaikan untuk distro Anda. M3 tidak mengklaim isolasi multi-tenant yang kuat; anggap kode aplikasi + ekstensi PHP sebagai tepercaya (SPEC 10).

## Log + health

```bash
journalctl -u phpx -f
# access log terstruktur: request_id, method, path, status, duration_ms
curl -i http://127.0.0.1:8080/healthz  # liveness
curl -i http://127.0.0.1:8080/readyz   # readiness (cek docroot + entrypoint)
phpx --config /etc/phpx/phpx.toml workers status
phpx --config /etc/phpx/phpx.toml metrics
```
