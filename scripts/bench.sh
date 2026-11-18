#!/usr/bin/env bash
# PHPX M3 reproducible baseline benchmark (NOT a performance claim).
# Uses wrk if available, else curl loop. Single-node, vanilla fixture.
set -euo pipefail
PORT="${PHPX_BENCH_PORT:-18082}"
REQS="${1:-200}"
CONC="${2:-10}"
ROOT="$(pwd)/examples/vanilla-php"
CFG="/tmp/phpx-bench.toml"
BIN="${PHPX_BIN:-./target/debug/phpx}"

cargo build -p phpx-cli -q
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
request_timeout_seconds = 30
startup_timeout_seconds = 10
memory_limit = "256M"
[runtime.embedded]
enabled = false
EOF

RUST_LOG=warn "$BIN" serve --config "$CFG" > /tmp/phpx-bench.log 2>&1 &
SRV=$!
trap "kill $SRV 2>/dev/null || true" EXIT
for i in $(seq 1 30); do curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1 && break; sleep 0.5; done

COLD_START=$(python3 -c "import time,urllib.request; s=time.time(); urllib.request.urlopen(\"http://127.0.0.1:$PORT/\").read(); print(int((time.time()-s)*1000))" 2>/dev/null || echo 0)
echo "[bench] cold_ms=$COLD_START (request 200 pertama saatos healthz, termasuk spawn PHP pertama)"
echo "[bench] M3 baseline: $REQS requests, concurrency $CONC, vanilla / (one-shot PHP, workers=4)"
echo "[bench] date=$(date -u +%Y-%m-%dT%H:%M:%SZ) php=$(php -r "echo PHP_VERSION;" 2>/dev/null || echo unknown) rust=$(rustc --version)"
if command -v wrk >/dev/null 2>&1; then
  wrk -t"$CONC" -c"$CONC" -d15s "http://127.0.0.1:$PORT/" || true
else
  echo "[bench] wrk not found, using curl loop (rough timing)..."
  START=$(date +%s)
  seq 1 "$REQS" | xargs -P "$CONC" -I{} curl -fsS "http://127.0.0.1:$PORT/" >/dev/null
  END=$(date +%s)
  ELAPSED=$((END-START > 0 ? END-START : 1))
  echo "[bench] completed $REQS requests in ${ELAPSED}s (~$((REQS/ELAPSED)) req/s, developer machine baseline only)"
fi
echo "[bench] done. See /tmp/phpx-bench.log for access logs. Do not use as SLA."
