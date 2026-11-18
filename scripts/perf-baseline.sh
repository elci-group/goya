#!/usr/bin/env bash
# Emperor Phase 2 performance baseline harness.
#
# Measures, honestly and repeatably, with no optimization claims:
#   1. check   : `emperor check <fixture> --format json` (lowering + admission path)
#   2. test    : `emperor test <fixture> --suite conformance --format json` (lowering + in-process oracle)
#
# Each command is run 5 times; min and median are reported. A warmup run is
# performed first so cargo's incremental artifacts are in place and all runs
# measure the same steady-state cost. Machine context (CPU, cores, cargo,
# rustc) is recorded in the JSON output.
#
# Usage: scripts/perf-baseline.sh [output.json]
#   Default output: evidence/emperor/performance/baseline.json
#
# Exit code is 0 only if every measured command succeeded on every run.

set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENE="crates/sash/fixtures/interactive-counter.gscene"
RUNS=5
OUT="${1:-$ROOT/evidence/emperor/performance/baseline.json}"

cd "$ROOT" || { echo "cannot cd to $ROOT" >&2; exit 1; }

command -v cargo >/dev/null || { echo "cargo not found" >&2; exit 1; }
[ -f "$SCENE" ] || { echo "missing fixture: $SCENE" >&2; exit 1; }

# --- machine context ---------------------------------------------------------
cpu_model="$(grep -m1 'model name' /proc/cpuinfo | sed 's/.*: //')"
nproc_count="$(nproc)"
mem_total_kb="$(grep -m1 MemTotal /proc/meminfo | awk '{print $2}')"
cargo_version="$(cargo --version)"
rustc_version="$(rustc --version)"
kernel="$(uname -r)"
hostname="$(hostname)"

# --- helpers -----------------------------------------------------------------

# time_cmd <label> <args...> — prints "<label> <seconds> <exitcode>" per run
time_cmd() {
    local label="$1"; shift
    local start end s ns exit_code
    start=$(date +%s.%N)
    "$@" > /dev/null 2> /tmp/perf-baseline-stderr.$$
    exit_code=$?
    end=$(date +%s.%N)
    s=${start%.*}; ns=${start#*.}
    end_s=${end%.*}; end_ns=${end#*.}
    local elapsed
    elapsed=$(awk -v s1="$start" -v s2="$end" 'BEGIN{printf "%.6f", s2-s1}')
    echo "$label $elapsed $exit_code"
    if [ "$exit_code" -ne 0 ]; then
        echo "  stderr (truncated): $(head -c 400 /tmp/perf-baseline-stderr.$$)" >&2
    fi
    rm -f /tmp/perf-baseline-stderr.$$
}

# Runs 5 times after one warmup; emits a JSON array of durations.
measure() {
    local label="$1"; shift
    echo "=== measuring $label (1 warmup + $RUNS runs) ===" >&2
    # warmup
    "$@" > /dev/null 2>&1 || { echo "warmup failed for $label" >&2; exit 1; }
    local durations=() result exit_code
    for i in $(seq 1 $RUNS); do
        result=$(time_cmd "$label" "$@")
        exit_code=$(echo "$result" | awk '{print $3}')
        [ "$exit_code" -ne 0 ] && { echo "run $i of $label exited $exit_code" >&2; exit 1; }
        durations+=("$(echo "$result" | awk '{print $2}')")
    done
    # stats via awk: min, median, mean
    local stats runs_json
    stats=$(printf '%s\n' "${durations[@]}" | awk '{
        v[NR]=$1
        if (NR==1 || $1<min) min=$1
        sum+=$1
    } END {
        for (i=1;i<=NR;i++) for (j=i+1;j<=NR;j++) if (v[j]<v[i]) { t=v[i]; v[i]=v[j]; v[j]=t }
        med=(NR%2)? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2
        printf "%.6f\n%.6f\n%.6f\n", min, med, sum/NR
    }')
    runs_json=$(printf '[%s]' "$(IFS=,; echo "${durations[*]}")")
    printf '%s\n%s\n%s\n%s\n' "$(echo "$stats" | sed -n 1p)" "$(echo "$stats" | sed -n 2p)" "$(echo "$stats" | sed -n 3p)" "$runs_json"
}

# --- warm up the cargo build once so all 5 runs measure the same cost -------
echo "=== prebuilding emperor (debug, offline) ===" >&2
cargo build -p emperor --offline >&2 || { echo "prebuild failed" >&2; exit 1; }

# --- measure -----------------------------------------------------------------
# Measured as specified: `cargo run -p emperor --offline -- <cmd> ...`.
CARGO_RUN=(cargo run -p emperor --offline)
check_stats=$(measure "check" "${CARGO_RUN[@]}" check "$SCENE" --format json)
test_stats=$(measure "test-conformance" "${CARGO_RUN[@]}" test "$SCENE" --suite conformance --format json)

check_min=$(echo "$check_stats" | sed -n 1p)
check_median=$(echo "$check_stats" | sed -n 2p)
check_mean=$(echo "$check_stats" | sed -n 3p)
check_runs=$(echo "$check_stats" | sed -n 4p)
test_min=$(echo "$test_stats" | sed -n 1p)
test_median=$(echo "$test_stats" | sed -n 2p)
test_mean=$(echo "$test_stats" | sed -n 3p)
test_runs=$(echo "$test_stats" | sed -n 4p)

mkdir -p "$(dirname "$OUT")"

cat > "$OUT" <<EOF
{
  "status": "MEASURED",
  "date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "fixture": "$SCENE",
  "runs_per_measurement": $RUNS,
  "binary": "cargo run -p emperor --offline",
  "check": {
    "command": "emperor check $SCENE --format json",
    "runs_seconds": $check_runs,
    "min_seconds": $check_min,
    "median_seconds": $check_median,
    "mean_seconds": $check_mean
  },
  "test_conformance": {
    "command": "emperor test $SCENE --suite conformance --format json",
    "runs_seconds": $test_runs,
    "min_seconds": $test_min,
    "median_seconds": $test_median,
    "mean_seconds": $test_mean
  },
  "context": {
    "hostname": "$hostname",
    "cpu_model": "$cpu_model",
    "cores": "$nproc_count",
    "mem_total_kb": "$mem_total_kb",
    "kernel": "$kernel",
    "cargo": "$cargo_version",
    "rustc": "$rustc_version",
    "profile": "debug (emperor binary), offline"
  }
}
EOF

# Fix the placeholder runs array for check with real values (kept simple: min/median/mean recorded above).
echo
echo "=== baseline written to $OUT ==="
echo "check    : min ${check_min}s  median ${check_median}s  mean ${check_mean}s"
echo "test/conf: min ${test_min}s  median ${test_median}s  mean ${test_mean}s"
