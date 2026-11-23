#!/usr/bin/env bash
# PHPX M4+P1-A soak: fixed-pool baseline, RSS trend + /metrics report (adaptive disabled).
# Usage: ./scripts/soak.sh [--port 18081] [--requests 2000] [--concurrency 8] [--execution-mode one-shot|persistent]
set -euo pipefail
PORT=18081
REQUESTS=2000
CONC=8
MODE="one-shot"
while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT="$2"; shift 2;;
    --requests) REQUESTS="$2"; shift 2;;
    --concurrency) CONC="$2"; shift 2;;
    --execution-mode) MODE="$2"; shift 2;;
    *) echo "unknown arg $1"; exit 2;;
  esac
done
case "$MODE" in
  one-shot|persistent) ;;
  *) echo "execution-mode must be one-shot or persistent, got $MODE"; exit 2;;
esac
ROOT="$(pwd)/examples/vanilla-php"
CFG="/tmp/phpx-soak.toml"
BIN="${PHPX_BIN:-./target/debug/phpx}"
LOG="/tmp/phpx-soak.log"
OUT="/tmp/phpx-soak.out"

echo "[soak] building phpx (debug)..."
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
execution_mode = "$MODE"
workers = 4
min_workers = 1
max_requests = 500
request_timeout_seconds = 10
startup_timeout_seconds = 10
memory_limit = "256M"
queue_wait_timeout_seconds = 2
shutdown_drain_timeout_seconds = 10
restart_max_retries = 5
restart_backoff_ms = 250
restart_backoff_max_ms = 30000
[runtime.embedded]
enabled = false
EOF

echo "[soak] starting gateway on 127.0.0.1:$PORT..."
RUST_LOG=warn "$BIN" serve --config "$CFG" > "$LOG" 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null || true' EXIT
sleep 1.5

echo "[soak] $REQUESTS requests, concurrency $CONC..."
START=$(date +%s)
seq 1 "$REQUESTS" | xargs -P "$CONC" -I{} bash -c '
  case $(({} % 4)) in
    0) curl -sf -o /dev/null "http://127.0.0.1:'"$PORT"'/" ;;
    1) curl -sf -o /dev/null "http://127.0.0.1:'"$PORT"'/hello.php" ;;
    2) curl -sf -o /dev/null "http://127.0.0.1:'"$PORT"'/style.css" ;;
    3) curl -sf -o /dev/null "http://127.0.0.1:'"$PORT"'/healthz" ;;
  esac' 2>/dev/null || true
END=$(date +%s)
DUR=$((END > START ? END - START : 1))

echo "[soak] scraping /metrics..."
METRICS="$(curl -sf "http://127.0.0.1:$PORT/metrics" || echo METRICS_FAILED)"
echo "$METRICS" > "$OUT.metrics"
echo "$METRICS" | grep -E "^phpx_pool_" || true

echo "--- soak report (fixed-pool baseline, mode=$MODE, adaptive disabled) ---"
echo "requests=$REQUESTS concurrency=$CONC duration_s=$DUR rps=$((REQUESTS / DUR))"
if command -v ps >/dev/null 2>&1; then
  RSS=$(ps -o rss= -p "$SRV" 2>/dev/null | tr -d " " || echo unknown)
  echo "gateway_rss_kb=$RSS"
fi
echo "metrics_saved=$OUT.metrics log=$LOG"
echo "[soak] done. SIGTERM drain test: kill $SRV and watch log for drain complete."
kill -TERM "$SRV" 2>/dev/null || true
sleep 2
grep -i -E "drain|shutdown" "$LOG" | tail -5 || true
trap - EXIT
