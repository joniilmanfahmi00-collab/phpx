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
# Capture-then-match (never `cli | grep -q`): grep -q closes the pipe on the
# first match, and any later CLI write would hit EPIPE (Rust panics with 101
# instead of dying quietly). Bash pattern matching uses no pipe at all.
check_contains() {
  local needle="$1"; shift
  local out
  out=$("$@") || { echo "FAIL: $* exited $?"; exit 1; }
  case "$out" in
    *"$needle"*) ;;
    *) echo "FAIL: '$*' output missing '$needle'"; exit 1 ;;
  esac
}
"$BIN" doctor >/dev/null
check_contains "developer preview" "$BIN" version
check_contains "process" "$BIN" runtime list
check_contains "bounded pool" "$BIN" workers status
check_contains "healthz" "$BIN" metrics
check_contains "driver=" "$BIN" queue status
check_contains "scheduler" "$BIN" scheduler run
check_contains "dispatched" "$BIN" --json scheduler run
check_contains "cache" "$BIN" cache status
check_contains "entries" "$BIN" --json cache status
check_contains "stored" "$BIN" cache set --key smoke --value ok
check_contains "storage" "$BIN" storage status
check_contains "objects" "$BIN" --json storage status

echo "[smoke] ALL GREEN (M3+M5 developer preview: gateway + queue + scheduler + cache + storage)"
