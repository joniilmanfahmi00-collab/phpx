# PHPX native Linux quickstart (M3 developer preview)

Target: Ubuntu 22.04+ / Debian 12+ (x86_64). Windows/macOS use WSL2 or Docker (see quickstart-docker.md).
Status: v0.1 developer preview, not a production SLA. See limitations.md.

## 1. Prerequisites

```bash
sudo apt update && sudo apt install -y build-essential pkg-config git curl php-cli php-json php-mbstring php-curl php-sqlite3
rustup show   # toolchain pinned in rust-toolchain.toml (1.99.1)
php --version # 8.3 / 8.4 / 8.5, see php-version-matrix.md
```

Full list: linux-prerequisites.md.

## 2. Build + doctor

```bash
git clone <phpx-repo> && cd phpx
cargo build -p phpx-cli
./target/debug/phpx doctor
# expect: phpx doctor (M3 developer preview), all OK
```

## 3. Serve vanilla fixture

```bash
cp phpx.toml.example phpx.toml
# edit phpx.toml if needed: root="examples/vanilla-php", document_root="public"
./target/debug/phpx serve --config phpx.toml
# or: ./target/debug/phpx serve --config examples/vanilla-php-toml? Use:
# ./target/debug/phpx serve --host 127.0.0.1 --port 8080 --config phpx.toml
```

In another terminal:

```bash
curl -i http://127.0.0.1:8080/healthz   # -> 200 ok (phpx v0.1 developer preview)
curl -i http://127.0.0.1:8080/readyz    # -> 200 ready
curl -i http://127.0.0.1:8080/          # -> 200 vanilla fixture (front controller)
curl -i http://127.0.0.1:8080/style.css # -> 200 static, correct MIME
curl -i http://127.0.0.1:8080/hello.php # -> 200 dispatched, never raw source
curl -i http://127.0.0.1:8080/%2e%2e/x  # -> 400 bad request (traversal blocked)
```

Logs show structured access lines with `request_id, method, path, status, duration_ms` and `x-request-id` response header.

## 4. Status commands

```bash
./target/debug/phpx runtime list
./target/debug/phpx runtime inspect
./target/debug/phpx workers status
./target/debug/phpx workers reload   # validates config; restart serve to apply
./target/debug/phpx metrics
```

`queue status|work` and `scheduler run` exit 2 with deferred-to-M5 note (expected in M3).

## 5. Systemd (optional, dev machine)

See systemd.md + packaging/systemd/phpx.service:

```bash
sudo cp packaging/systemd/phpx.service /etc/systemd/system/phpx.service
sudo systemctl daemon-reload
sudo systemctl restart phpx
systemctl status phpx
journalctl -u phpx -f
```

## 6. Smoke + bench

```bash
bash scripts/smoke-linux.sh
bash scripts/bench.sh --requests 200 --concurrency 10
```

Expected: smoke passes (healthz, front-controller, static, traversal 400, crash 500 then 200, backpressure 503 under load).
Benchmark output is reproducible baseline only, not a performance claim.

## Troubleshooting

- `config error ... (path: ...)` -> copy example, check TOML, see configuration.md.
- `cannot bind ...` -> port in use; change `server.port` or `phpx serve --port 9001`.
- `php not found` -> install PHP or set `PHPX_PHP_BIN=/usr/bin/php8.4`.
- `tls=true requires ...` -> set `tls=false` for local dev or provide PEM cert+key.
