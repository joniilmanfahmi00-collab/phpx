#!/usr/bin/env bash
# PHPX bench repeat harness (RC-track reproducibility record, NOT an SLA).
# Runs bench-lab.sh N times with identical settings, then aggregates the per-run
# summary.json files into reproducibility.json (mean/min/max/stdev/CV per metric)
# and optionally gates on variance. Same host + same config across runs is the
# comparability contract; CV gates catch methodology noise, never cross-machine SLA.
#
# Usage: bash scripts/bench-repeat.sh [--runs 3] [--requests 200] [--concurrency 10]
#   [--mode persistent] [--profile release] [--backend phpx] [--workers 4]
#   [--port 18082] [--out-dir data/bench/<auto>] [--max-failures 0] [--max-cv-pct 30]
#
# CV gate applies to rps + warm p50/p95 only (cold_ms is spawn noise by design
# and is reported without a gate). Each run also enforces --max-failures.
set -euo pipefail
RUNS=3
PORT=18082
REQS=200
CONC=10
MODE="persistent"
PROFILE="release"
BACKEND="phpx"
WORKERS=4
OUTDIR=""
MAXFAIL="0"
MAXCV="30"
while [ $# -gt 0 ]; do
  case "$1" in
    --runs) RUNS="$2"; shift 2;;
    --port) PORT="$2"; shift 2;;
    --requests) REQS="$2"; shift 2;;
    --concurrency) CONC="$2"; shift 2;;
    --mode) MODE="$2"; shift 2;;
    --profile) PROFILE="$2"; shift 2;;
    --backend) BACKEND="$2"; shift 2;;
    --workers) WORKERS="$2"; shift 2;;
    --out-dir) OUTDIR="$2"; shift 2;;
    --max-failures) MAXFAIL="$2"; shift 2;;
    --max-cv-pct) MAXCV="$2"; shift 2;;
    --help|-h)
      echo "Usage: bench-repeat.sh [--runs N] [--requests N] [--concurrency N] [--mode one-shot|persistent] [--profile debug|release] [--backend phpx|php-builtin] [--workers N] [--port N] [--out-dir DIR] [--max-failures N] [--max-cv-pct P]";
      exit 0;;
    *) echo "unknown arg $1"; exit 2;;
  esac
done
if [ "$RUNS" -lt 2 ] || [ "$RUNS" -gt 10 ]; then echo "runs must be 2-10"; exit 2; fi
if [ -z "$OUTDIR" ]; then
  OUTDIR="data/bench/$(date -u +%Y%m%dT%H%M%SZ)-repeat-${BACKEND}-${MODE}-${PROFILE}x${RUNS}"
fi
case "$OUTDIR" in /*) ;; *) OUTDIR="$(pwd)/$OUTDIR";; esac
mkdir -p "$OUTDIR"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
for i in $(seq 1 "$RUNS"); do
  echo "[bench-repeat] run $i/$RUNS ..."
  bash "$SCRIPT_DIR/bench-lab.sh" --port "$PORT" --requests "$REQS" --concurrency "$CONC" \
    --mode "$MODE" --profile "$PROFILE" --backend "$BACKEND" --workers "$WORKERS" \
    --max-failures "$MAXFAIL" --out-dir "$OUTDIR/run-$i"
done
export BL_PARENT="$OUTDIR" BL_RUNS="$RUNS" BL_MAXCV="$MAXCV" BL_OUT="$OUTDIR/reproducibility.json"
php -r '
$p = getenv("BL_PARENT"); $n = (int)getenv("BL_RUNS");
$get = function($i, $path) use ($p) {
  $s = json_decode(@file_get_contents("$p/run-$i/summary.json"), true);
  $v = $s;
  foreach (explode(".", $path) as $k) { $v = is_array($v) && array_key_exists($k, $v) ? $v[$k] : null; }
  return is_numeric($v) ? (float)$v : null;
};
$metrics = ["rps" => "rps", "cold_ms" => "cold_ms", "warm.mean_ms" => "warm_mean_ms",
  "warm.p50_ms" => "warm_p50_ms", "warm.p95_ms" => "warm_p95_ms", "warm.p99_ms" => "warm_p99_ms",
  "warm.max_ms" => "warm_max_ms"];
$fail = 0;
$rep = ["schema" => "phpx-bench-repeat/1", "runs" => $n, "metrics" => []];
foreach ($metrics as $path => $name) {
  $vals = [];
  for ($i = 1; $i <= $n; $i++) { $v = $get($i, $path); if ($v !== null) { $vals[] = $v; } }
  if (count($vals) < $n) { fwrite(STDERR, "missing $path in run summaries\n"); exit(2); }
  $mean = array_sum($vals) / $n;
  $var = 0.0;
  foreach ($vals as $v) { $var += ($v - $mean) * ($v - $mean); }
  $var /= $n;
  $sd = sqrt($var);
  $cv = $mean != 0 ? round($sd / abs($mean) * 100, 2) : 0.0;
  $rep["metrics"][$name] = ["n" => $n, "values" => $vals, "mean" => round($mean, 3),
    "min" => min($vals), "max" => max($vals), "stdev" => round($sd, 3), "cv_pct" => $cv];
}
$gated = ["warm_p50_ms", "warm_p95_ms", "rps"];
$maxcv = (float)getenv("BL_MAXCV");
$viol = [];
foreach ($gated as $g) {
  if ($rep["metrics"][$g]["cv_pct"] > $maxcv) { $viol[] = "$g cv=" . $rep["metrics"][$g]["cv_pct"] . "% exceeds --max-cv-pct $maxcv"; }
}
$rep["gates"] = ["max_cv_pct" => $maxcv, "gated_metrics" => $gated, "pass" => count($viol) === 0,
  "note" => "cold_ms reported without CV gate (spawn noise by design)"];
file_put_contents(getenv("BL_OUT"), json_encode($rep, JSON_PRETTY_PRINT) . "\n");
foreach ($rep["metrics"] as $name => $m) {
  echo sprintf("%-12s mean=%-10s min=%-10s max=%-10s cv=%s%%\n", $name, $m["mean"], $m["min"], $m["max"], $m["cv_pct"]);
}
if (count($viol) > 0) { fwrite(STDERR, "REPRODUCIBILITY VIOLATION: " . implode("; ", $viol) . "\n"); exit(1); }
echo "reproducibility: PASS (max CV $maxcv% on rps/p50/p95)\n";
'
echo "[bench-repeat] results=$OUTDIR/reproducibility.json (NOT an SLA)"
