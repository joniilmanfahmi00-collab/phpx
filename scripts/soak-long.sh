#!/usr/bin/env bash
# PHPX RC soak-long: 60 menit default, sample RSS tiap 60s, slope KB/menit (NOT SLA).
# Usage: ./scripts/soak-long.sh [--port 18081] [--minutes 60] [--concurrency 8]
set -euo pipefail
PORT=18081
MINS=60
CONC=8
while [ $# -gt 0 ]; do case "$1" in --port) PORT="$2"; shift 2;; --minutes) MINS="$2"; shift 2;; --concurrency) CONC="$2"; shift 2;; *) echo "unknown $1"; exit 2;; esac; done
ROOT="$(pwd)/examples/vanilla-php"
CFG="/tmp/phpx-soaklong.toml"
BIN="${PHPX_BIN:-./target/debug/phpx}"
LOG="/tmp/phpx-soaklong.log"
CSV="/tmp/phpx-soaklong-rss.csv"
cargo build -p phpx-cli -q
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
workers = 4
max_requests = 500
request_timeout_seconds = 10
startup_timeout_seconds = 10
memory_limit = "256M"
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
curl -fsS "http://127.0.0.1:$PORT/metrics" > /tmp/phpx-soaklong.out.metrics || true
echo "[soak-long] csv=$CSV log=$LOG metrics=/tmp/phpx-soaklong.out.metrics"
tail -3 "$CSV"
echo "[soak-long] hitung slope manual: (rss_akhir-rss_awal)/menit, gateway harus flat. PHP one-shot RSS direclaim OS."
kill $SRV 2>/dev/null || true
trap - EXIT
