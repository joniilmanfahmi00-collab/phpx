#!/usr/bin/env bash
# PHPX RC compare skeleton (NOT SLA): phpx vs php-builtin, workload sami persis.
# Usage: ./scripts/bench-compare.sh [--port 18082] [--requests 200] [--concurrency 10]
set -euo pipefail
PORT=18082
REQS=200
CONC=10
while [ $# -gt 0 ]; do case "$1" in --port) PORT="$2"; shift 2;; --requests) REQS="$2"; shift 2;; --concurrency) CONC="$2"; shift 2;; *) echo "unknown $1"; exit 2;; esac; done
ROOT="$(pwd)/examples/vanilla-php/public"
BIN="${PHPX_BIN:-./target/debug/phpx}"
echo "[compare] baseline lock: $REQS req, conc $CONC, fixture vanilla-php/public"
echo "[compare] versions: phpx=$($BIN --version 2>/dev/null || echo unknown) php=$(php -r 'echo PHP_VERSION;' 2>/dev/null || echo unknown) rust=$(rustc --version 2>/dev/null || echo unknown)"
echo "[compare] 1) phpx..."
PHPX_BENCH_PORT=$PORT ./scripts/bench.sh "$REQS" "$CONC" 2>&1 | tail -5 || true
echo "[compare] 2) php-builtin (php -S, same docroot, no gateway)..."
php -S "127.0.0.1:$((PORT+10))" -t "$ROOT" >/tmp/phpx-phpbuiltin.log 2>&1 & 
PB=$!
trap "kill $PB 2>/dev/null || true" EXIT
sleep 1
START=$(date +%s)
seq 1 "$REQS" | xargs -P "$CONC" -I{} curl -fsS "http://127.0.0.1:$((PORT+10))/" >/dev/null 2>&1 || true
END=$(date +%s)
DUR=$((END > START ? END - START : 1))
echo "[compare] php-builtin: $REQS req in ${DUR}s (~$((REQS/DUR)) req/s, mesin dev saja)"
kill $PB 2>/dev/null || true
trap - EXIT
echo "[compare] done. Klaim = delta vs baseline checked-in, bukan SLA. php-fpm/frankenphp/rr: ganti langkah 2 dgn backend sama + workload sama."
