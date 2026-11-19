#!/usr/bin/env bash
# PHPX M3 smoke test - native Linux (also runs in CI).
# Fails fast; prints each check. Reproducible, not a benchmark.
set -euo pipefail
ROOT="$(pwd)/examples/vanilla-php"
PORT="${PHPX_SMOKE_PORT:-18080}"
CFG="/tmp/phpx-smoke.toml"
BIN="${PHPX_BIN:-./target/debug/phpx}"

echo "[smoke] building phpx (debug)..."
cargo build -p phpx-cli -q

echo "[smoke] writing $CFG (root=$ROOT)"
cat > "$CFG" <<EOF
schema_version = 1
[server]
host = "127.0.0.1"
port = $PORT
tls = false
request_body_limit = "16MB"
[application]
root = "$ROOT"
document_root = "public"
entrypoint = "index.php"
[runtime]
mode = "process"
php_version = "8.4"
[runtime.process]
workers = 4
max_requests = 1000
request_timeout_seconds = 10
startup_timeout_seconds = 10
memory_limit = "256M"
[runtime.embedded]
enabled = false
EOF

echo "[smoke] starting gateway on 127.0.0.1:$PORT..."
RUST_LOG=info "$BIN" serve --config "$CFG" > /tmp/phpx-smoke.log 2>&1 &
SRV=$!
trap "kill $SRV 2>/dev/null || true" EXIT
for i in $(seq 1 30); do
  if curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1; then break; fi
  sleep 0.5
done

echo "[smoke] healthz..."
curl -fsS -i "http://127.0.0.1:$PORT/healthz" | head -5
echo "[smoke] readyz..."
curl -fsS "http://127.0.0.1:$PORT/readyz" | grep -q ready
echo "[smoke] front controller / ..."
curl -fsS "http://127.0.0.1:$PORT/" | grep -qi "phpx\|vanilla\|hello\|index"
echo "[smoke] static /style.css ..."
curl -fsS "http://127.0.0.1:$PORT/style.css" | head -3
echo "[smoke] php dispatch /hello.php (no source leak) ..."
BODY=$(curl -fsS "http://127.0.0.1:$PORT/hello.php")
echo "$BODY" | head -3
if echo "$BODY" | grep -q "<?php"; then echo "FAIL: php source leaked"; exit 1; fi
echo "[smoke] traversal blocked ..."
code=$(curl --path-as-is -o /dev/null -s -w "%{http_code}" "http://127.0.0.1:$PORT/%2e%2e/x" || true)
if [ "$code" != "400" ]; then echo "FAIL: traversal expected 400 got $code"; exit 1; fi
echo "[smoke] crash isolation (/crash.php -> 500, then / -> 200) ..."
code=$(curl -o /dev/null -s -w "%{http_code}" "http://127.0.0.1:$PORT/crash.php" || true)
if [ "$code" != "500" ]; then echo "FAIL: crash expected 500 got $code"; exit 1; fi
curl -fsS "http://127.0.0.1:$PORT/" >/dev/null
echo "[smoke] CLI contract ..."
"$BIN" doctor >/dev/null
"$BIN" version | grep -q "developer preview"
"$BIN" runtime list | grep -q process
"$BIN" workers status | grep -q "bounded pool"
"$BIN" metrics | grep -q "healthz"
"$BIN" queue status | grep -q "driver="
"$BIN" scheduler run | grep -qi "scheduler"
"$BIN" --json scheduler run | grep -q dispatched
"$BIN" cache status | grep -q "cache"
"$BIN" --json cache status | grep -q entries
"$BIN" cache set --key smoke --value ok | grep -qi "stored"
"$BIN" storage status | grep -q "storage"
"$BIN" --json storage status | grep -q objects

echo "[smoke] ALL GREEN (M3+M5 developer preview: gateway + queue + scheduler + cache + storage)"
