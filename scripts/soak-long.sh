#!/usr/bin/env bash
# PHPX RC soak-long: 60 menit default, sample RSS tiap 60s, slope KB/menit (NOT SLA).
# Usage: ./scripts/soak-long.sh [--port 18081] [--minutes 60] [--concurrency 8] [--mode one-shot|persistent] [--profile debug|release] [--out-dir DIR]
set -euo pipefail
PORT=18081
MINS=60
CONC=8
MODE="one-shot"
PROFILE="debug"
OUTDIR=""
while [ $# -gt 0 ]; do case "$1" in --port) PORT="$2"; shift 2;; --minutes) MINS="$2"; shift 2;; --concurrency) CONC="$2"; shift 2;; --mode) MODE="$2"; shift 2;; --profile) PROFILE="$2"; shift 2;; --out-dir) OUTDIR="$2"; shift 2;; *) echo "unknown $1"; exit 2;; esac; done
case "$MODE" in one-shot|persistent) ;; *) echo "mode must be one-shot or persistent"; exit 2;; esac
case "$PROFILE" in debug|release) ;; *) echo "profile must be debug or release"; exit 2;; esac
ROOT="$(pwd)/examples/vanilla-php"
if [ -z "$OUTDIR" ]; then OUTDIR="/tmp"; fi
case "$OUTDIR" in /*) ;; *) OUTDIR="$(pwd)/$OUTDIR";; esac
CFG="$OUTDIR/phpx-soaklong.toml"
if [ "$PROFILE" = "release" ]; then BIN="./target/release/phpx"; cargo build --release -p phpx-cli -q
else BIN="${PHPX_BIN:-./target/debug/phpx}"; cargo build -p phpx-cli -q; fi
LOG="$OUTDIR/phpx-soaklong.log"
CSV="$OUTDIR/phpx-soaklong-rss.csv"
cat > "$CFG" <<EOF
schema_version = 1
[server]
host = "127.0.0.1"
port = $PORT
tls = false
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
memory_soft_limit = "0"
memory_hard_limit = "0"
memory_sample_interval_seconds = 5
queue_wait_timeout_seconds = 2
shutdown_drain_timeout_seconds = 10
restart_max_retries = 5
restart_backoff_ms = 250
restart_backoff_max_ms = 30000
[runtime.embedded]
enabled = false
EOF
echo "t,rss_kb" > "$CSV"
RUST_LOG=warn "$BIN" serve --config "$CFG" > "$LOG" 2>&1 & 
SRV=$!
trap "kill $SRV 2>/dev/null || true" EXIT
for i in $(seq 1 30); do curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1 && break; sleep 0.5; done
END_AT=$(($(date +%s) + MINS*60))
while [ $(date +%s) -lt $END_AT ]; do
  seq 1 200 | xargs -P "$CONC" -I{} curl -fsS "http://127.0.0.1:$PORT/" >/dev/null 2>&1 || true
  RSS=$(ps -o rss= -p "$SRV" 2>/dev/null | tr -d " " || echo 0)
  echo "$(date -u +%FT%TZ),$RSS" >> "$CSV"
  sleep 50 || true
done
curl -fsS "http://127.0.0.1:$PORT/metrics" > "$OUTDIR/phpx-soaklong.out.metrics" || true
echo "[soak-long] out=$OUTDIR mode=$MODE profile=$PROFILE"
tail -3 "$CSV"
echo "[soak-long] hitung slope manual: (rss_akhir-rss_awal)/menit, gateway harus flat. PHP one-shot RSS direclaim OS."
kill $SRV 2>/dev/null || true
trap - EXIT
