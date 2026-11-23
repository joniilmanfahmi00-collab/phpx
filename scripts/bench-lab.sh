#!/usr/bin/env bash
# PHPX P1-B benchmark laboratory (NOT an SLA): reproducible methodology with
# raw measurements. Dimensions covered here: throughput, latency distribution
# (p50/p95/p99/max), cold start vs warm path, gateway RSS trend.
# Reliability/stability/soak live in P1-A (tests) + scripts/soak*.sh.
#
# Usage: ./scripts/bench-lab.sh [--port 18082] [--requests 200] [--concurrency 10]
#   [--mode one-shot|persistent] [--profile debug|release] [--backend phpx|php-builtin]
#   [--workers 4] [--out-dir data/bench/<auto>]
# Comparability contract: same host, same PHP, same fixture, same mix.
set -euo pipefail
PORT=18082
REQS=200
CONC=10
MODE="one-shot"
PROFILE="debug"
BACKEND="phpx"
WORKERS=4
OUTDIR=""
while [ $# -gt 0 ]; do
  case "$1" in
    --port) PORT="$2"; shift 2;;
    --requests) REQS="$2"; shift 2;;
    --concurrency) CONC="$2"; shift 2;;
    --mode) MODE="$2"; shift 2;;
    --profile) PROFILE="$2"; shift 2;;
    --backend) BACKEND="$2"; shift 2;;
    --workers) WORKERS="$2"; shift 2;;
    --out-dir) OUTDIR="$2"; shift 2;;
    --help|-h)
      echo "Usage: bench-lab.sh [--port N] [--requests N] [--concurrency N] [--mode one-shot|persistent] [--profile debug|release] [--backend phpx|php-builtin] [--workers N] [--out-dir DIR]";
      exit 0;;
    *) echo "unknown arg $1"; exit 2;;
  esac
done
case "$MODE" in one-shot|persistent) ;; *) echo "mode must be one-shot or persistent"; exit 2;; esac
case "$PROFILE" in debug|release) ;; *) echo "profile must be debug or release"; exit 2;; esac
case "$BACKEND" in phpx|php-builtin) ;; *) echo "backend must be phpx or php-builtin"; exit 2;; esac

ROOT="$(pwd)/examples/vanilla-php"
# git-bash on Windows reports MSYS paths (/d/...); the native phpx/php
# binaries need a Windows path (D:/...). Real Linux CI is unaffected.
case "$(uname -s 2>/dev/null || echo unknown)" in
  MINGW*|MSYS*|CYGWIN*) ROOT="$(pwd -W)/examples/vanilla-php";;
esac
if [ -z "$OUTDIR" ]; then
  OUTDIR="data/bench/$(date -u +%Y%m%dT%H%M%SZ)-${BACKEND}-${MODE}-${PROFILE}"
fi
# Absolutize: background jobs/subshells must not depend on cwd.
case "$OUTDIR" in /*) ;; *) OUTDIR="$(pwd)/$OUTDIR";; esac
mkdir -p "$OUTDIR"
command -v php >/dev/null || { echo "php not found (see linux-prerequisites.md)"; exit 2; }
command -v curl >/dev/null || { echo "curl not found"; exit 2; }

if [ "$BACKEND" = "phpx" ]; then
  if [ "$PROFILE" = "release" ]; then BIN="./target/release/phpx"; cargo build --release -p phpx-cli -q
  else BIN="./target/debug/phpx"; cargo build -p phpx-cli -q; fi
  CFG="$OUTDIR/phpx-bench.toml"
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
workers = $WORKERS
min_workers = 1
max_requests = 1000000
request_timeout_seconds = 30
startup_timeout_seconds = 15
memory_limit = "256M"
memory_soft_limit = "0"
memory_hard_limit = "0"
memory_sample_interval_seconds = 5
queue_wait_timeout_seconds = 5
shutdown_drain_timeout_seconds = 10
restart_max_retries = 5
restart_backoff_ms = 250
restart_backoff_max_ms = 30000
[runtime.embedded]
enabled = false
EOF
  RUST_LOG=warn "$BIN" serve --config "$CFG" > "$OUTDIR/gateway.log" 2>&1 &
  SRV=$!
  # shellcheck disable=SC2064
  trap "kill $SRV 2>/dev/null || true" EXIT
else
  BIN="php-builtin"
  php -S "127.0.0.1:$PORT" -t "$ROOT/public" > "$OUTDIR/php-builtin.log" 2>&1 &
  SRV=$!
  # shellcheck disable=SC2064
  trap "kill $SRV 2>/dev/null || true" EXIT
fi
for i in $(seq 1 60); do curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1 && break; sleep 0.5; done
if [ "$BACKEND" = "php-builtin" ]; then
  for i in $(seq 1 60); do curl -fsS "http://127.0.0.1:$PORT/" >/dev/null 2>&1 && break; sleep 0.5; done
fi
# Fail loudly when the gateway never came up (otherwise the whole run would
# record zeros and still exit 0). In-run flakes are recorded, not fatal.
if [ "$BACKEND" = "phpx" ]; then
  curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1 || { echo "[bench-lab] gateway failed to start on :$PORT (see $OUTDIR/gateway.log)"; exit 3; }
else
  curl -fsS "http://127.0.0.1:$PORT/" >/dev/null 2>&1 || { echo "[bench-lab] php-builtin failed to start on :$PORT"; exit 3; }
fi

# Gateway RSS sampler (max over the run; ps resident KB).
# Needs procps `ps -o` (real Linux CI). Under git-bash, ps lacks -o and the
# sampler records 0 — use bench-lab.ps1 on Windows for working RSS capture.
RSS_MAX=0
RSS_START=""
if [ "$BACKEND" = "phpx" ]; then
  RSS_START="$(ps -o rss= -p "$SRV" 2>/dev/null | tr -d ' ' || echo 0)"
  RSS_MAX="$RSS_START"
  ( while kill -0 "$SRV" 2>/dev/null; do
      R="$(ps -o rss= -p "$SRV" 2>/dev/null | tr -d ' ' || echo 0)"
      echo "$R" >> "$OUTDIR/rss.log"
      sleep 1
    done ) &
  RSSMON=$!
fi

# Cold start: first request after healthz (includes first PHP spawn/boot).
COLD_MS=$(curl -s -o /dev/null -w "%{time_total}" "http://127.0.0.1:$PORT/" || echo 0)
COLD_MS=$(BL_COLD_IN="$COLD_MS" php -r 'echo (int)round((float)getenv("BL_COLD_IN")*1000);')
echo "[bench-lab] cold_ms=$COLD_MS"

# Warm path: per-request timings, one file per request (append-safe).
TD="$OUTDIR/times.d"
mkdir -p "$TD"
START_NS=$(date +%s%N)
# In-run flakes are recorded as FAIL (see failures= in the summary), never fatal.
seq 1 "$REQS" | xargs -P "$CONC" -I{} sh -c 'curl -s -o /dev/null -w "%{time_total}\n" "http://127.0.0.1:$0/" > "$1/{}" || echo "FAIL" > "$1/{}"' "$PORT" "$TD" || true
END_NS=$(date +%s%N)
ELAPSED_S=$(BL_E0="$END_NS" BL_E1="$START_NS" php -r '$e=((int)getenv("BL_E0")-(int)getenv("BL_E1"))/1e9; echo $e>0?$e:0.001;')
FAILURES=$(grep -l FAIL "$TD"/* 2>/dev/null || true | wc -l | tr -d ' ')
cat "$TD"/* 2>/dev/null | grep -v FAIL > "$OUTDIR/times.txt" || true
rm -rf "$TD"

if [ -n "${RSSMON:-}" ]; then kill "$RSSMON" 2>/dev/null || true; fi
if [ "$BACKEND" = "phpx" ]; then
  RSS_END="$(ps -o rss= -p "$SRV" 2>/dev/null | tr -d ' ' || echo 0)"
  if [ -f "$OUTDIR/rss.log" ]; then RSS_MAX=$(sort -n "$OUTDIR/rss.log" | tail -1); fi
else
  RSS_END=0
fi

PHPV=$(php -r 'echo PHP_VERSION;')
RUSTV=$(rustc --version 2>/dev/null || echo unknown)
PHPXV=$([ "$BACKEND" = "phpx" ] && ("$BIN" version 2>/dev/null || echo unknown) || echo n/a)
OSV=$(uname -srm 2>/dev/null || echo unknown)
DATEV=$(date -u +%Y-%m-%dT%H:%M:%SZ)
RPS=$(BL_R="$REQS" BL_E="$ELAPSED_S" php -r '$r=(int)getenv("BL_R"); $e=(float)getenv("BL_E"); echo $e>0?round($r/$e,2):0;')
export BL_DATE="$DATEV" BL_OS="$OSV" BL_PHP="$PHPV" BL_RUST="$RUSTV" BL_PHPX="$PHPXV"
export BL_BACKEND="$BACKEND" BL_MODE="$MODE" BL_PROFILE="$PROFILE" BL_WORKERS="$WORKERS"
export BL_REQS="$REQS" BL_CONC="$CONC" BL_FAIL="$FAILURES" BL_COLD="$COLD_MS"
export BL_ELAPSED="$ELAPSED_S" BL_RPS="$RPS" BL_RSS0="$RSS_START" BL_RSSMAX="$RSS_MAX" BL_RSS1="$RSS_END"
export BL_TIMES="$OUTDIR/times.txt" BL_OUT="$OUTDIR/summary.json"
SUMMARY=$(php -r '
$t=array_map("floatval", array_filter(array_map("trim", @file(getenv("BL_TIMES")) ?: []), fn($x)=>$x!=="" && $x!=="FAIL"));
$d=["n"=>0];
if ($t) { sort($t); $n=count($t);
  $q=function($p) use ($t,$n){ $i=(int)ceil($p*$n)-1; $i=max(0,min($n-1,$i)); return $t[$i]; };
  $d=["n"=>$n,"mean_ms"=>round(array_sum($t)/$n*1000,3),"p50_ms"=>round($q(0.50)*1000,3),"p95_ms"=>round($q(0.95)*1000,3),"p99_ms"=>round($q(0.99)*1000,3),"max_ms"=>round(max($t)*1000,3)]; }
$o=["schema"=>"phpx-bench-lab/1","date"=>getenv("BL_DATE"),"os"=>getenv("BL_OS"),"php_version"=>getenv("BL_PHP"),"rustc"=>getenv("BL_RUST"),"phpx_version"=>getenv("BL_PHPX"),"backend"=>getenv("BL_BACKEND"),"execution_mode"=>(getenv("BL_BACKEND")==="phpx"?getenv("BL_MODE"):"n/a"),"profile"=>getenv("BL_PROFILE"),"workers"=>(int)getenv("BL_WORKERS"),"requests"=>(int)getenv("BL_REQS"),"concurrency"=>(int)getenv("BL_CONC"),"failures"=>(int)getenv("BL_FAIL"),"cold_ms"=>(int)getenv("BL_COLD"),"elapsed_s"=>(float)getenv("BL_ELAPSED"),"rps"=>(float)getenv("BL_RPS"),"warm"=>$d,"rss_kb"=>["start"=>(int)getenv("BL_RSS0"),"max"=>(int)getenv("BL_RSSMAX"),"end"=>(int)getenv("BL_RSS1")]];
file_put_contents(getenv("BL_OUT"), json_encode($o, JSON_PRETTY_PRINT)."\n"); echo json_encode($o, JSON_PRETTY_PRINT),"\n";')
echo "$SUMMARY"

echo "[bench-lab] failures=$FAILURES rps=$RPS results=$OUTDIR/summary.json (NOT an SLA)"
if [ "$BACKEND" = "phpx" ]; then
  curl -sf "http://127.0.0.1:$PORT/metrics" > "$OUTDIR/metrics.txt" 2>/dev/null || true
fi
kill "$SRV" 2>/dev/null || true
trap - EXIT
