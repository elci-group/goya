#!/usr/bin/env bash
# Emperor Phase 3 performance baseline harness for `emperor build`.
#
# Measures the end-to-end wall time of a full build (lowering + cargo build
# of emperor-app into a fresh per-run target dir under /tmp) because cargo
# build is the dominant cost of the build path. Each measurement is run 5
# times after 1 warmup, each run into its OWN fresh out dir (so every run
# measures the same cold-target cost, matching per-build isolation).
#
# Machine context (CPU, cores, cargo, rustc) is recorded, matching
# scripts/perf-baseline.sh. The produced App binary size is recorded.
#
# Usage: scripts/perf-build-baseline.sh [output.json]
#   Default output: evidence/emperor/performance/build-baseline.json
#
# Exit code is 0 only if every measured build succeeded on every run.

set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCENE="crates/sash/fixtures/interactive-counter.gscene"
RUNS=5
TMPBASE="$(mktemp -d /tmp/emperor-perf-build.XXXXXX)"
OUT="${1:-$ROOT/evidence/emperor/performance/build-baseline.json}"

cleanup() { rm -rf "$TMPBASE"; }
trap cleanup EXIT

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
time_run() {
    # $1 = out dir; times `cargo run -p emperor --offline -- build ...`
    local outdir="$1"
    local start end exit_code elapsed
    start=$(date +%s.%N)
    cargo run -p emperor --offline -- build "$SCENE" --out "$outdir" \
        > /dev/null 2> /tmp/perf-build-stderr.$$
    exit_code=$?
    end=$(date +%s.%N)
    elapsed=$(awk -v s1="$start" -v s2="$end" 'BEGIN{printf "%.6f", s2-s1}')
    echo "$elapsed $exit_code"
    if [ "$exit_code" -ne 0 ]; then
        echo "  stderr (truncated): $(head -c 400 /tmp/perf-build-stderr.$$)" >&2
    fi
    rm -f /tmp/perf-build-stderr.$$
}

measure() {
    # Warmup run (fresh out dir, discarded).
    local warmdir="$TMPBASE/warmup"
    echo "=== warmup build into $warmdir ===" >&2
    local r
    r=$(time_run "$warmdir")
    [ "$(echo "$r" | awk '{print $2}')" -ne 0 ] && { echo "warmup build failed" >&2; exit 1; }
    local durations=() result elapsed exit_code
    for i in $(seq 1 $RUNS); do
        local outdir="$TMPBASE/run$i"
        echo "=== measuring build run $i into $outdir ===" >&2
        result=$(time_run "$outdir")
        exit_code=$(echo "$result" | awk '{print $2}')
        [ "$exit_code" -ne 0 ] && { echo "run $i exited $exit_code" >&2; exit 1; }
        durations+=("$(echo "$result" | awk '{print $1}')")
    done
    local stats
    stats=$(printf '%s\n' "${durations[@]}" | awk '{
        v[NR]=$1
        if (NR==1 || $1<min) min=$1
        sum+=$1
    } END {
        for (i=1;i<=NR;i++) for (j=i+1;j<=NR;j++) if (v[j]<v[i]) { t=v[i]; v[i]=v[j]; v[j]=t }
        med=(NR%2)? v[(NR+1)/2] : (v[NR/2]+v[NR/2+1])/2
        printf "%.6f\n%.6f\n%.6f\n", min, med, sum/NR
    }')
    local runs_json
    runs_json=$(printf '[%s]' "$(IFS=,; echo "${durations[*]}")")
    printf '%s\n%s\n%s\n%s\n' "$(echo "$stats" | sed -n 1p)" "$(echo "$stats" | sed -n 2p)" "$(echo "$stats" | sed -n 3p)" "$runs_json"
}

# --- prebuild the emperor compiler binary itself ------------------------------
# The measurement is `cargo run`, so the emperor binary must be prebuilt once
# (as scripts/perf-baseline.sh does) so runs measure build orchestration cost,
# not emperor's own compilation.
echo "=== prebuilding emperor (debug, offline) ===" >&2
cargo build -p emperor --offline >&2 || { echo "prebuild failed" >&2; exit 1; }

# --- measure -----------------------------------------------------------------
stats=$(measure)
min=$(echo "$stats" | sed -n 1p)
median=$(echo "$stats" | sed -n 2p)
mean=$(echo "$stats" | sed -n 3p)
runs_json=$(echo "$stats" | sed -n 4p)

# --- artifact size (bytes) from the last run's output ------------------------
app="$TMPBASE/run$RUNS/App"
if [ -f "$app" ]; then
    app_bytes=$(stat -c '%s' "$app")
    app_mib=$(awk -v b="$app_bytes" 'BEGIN{printf "%.2f", b/1048576}')
else
    echo "warning: $app not found; recording null size" >&2
    app_bytes="null"
    app_mib="null"
fi

mkdir -p "$(dirname "$OUT")"

cat > "$OUT" <<EOF
{
  "status": "MEASURED",
  "date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "fixture": "$SCENE",
  "command": "cargo run -p emperor --offline -- build $SCENE --out <fresh /tmp dir>",
  "runs_per_measurement": $RUNS,
  "out_isolation": "fresh out dir per run (cold target dir, matching per-build isolation)",
  "build": {
    "runs_seconds": $runs_json,
    "min_seconds": $min,
    "median_seconds": $median,
    "mean_seconds": $mean
  },
  "artifact": {
    "path": "App",
    "size_bytes": $app_bytes,
    "size_mib": $app_mib
  },
  "context": {
    "hostname": "$hostname",
    "cpu_model": "$cpu_model",
    "cores": "$nproc_count",
    "mem_total_kb": "$mem_total_kb",
    "kernel": "$kernel",
    "cargo": "$cargo_version",
    "rustc": "$rustc_version",
    "profile": "debug (emperor binary), offline, debug cargo build for emperor-app"
  }
}
EOF

echo
echo "=== build baseline written to $OUT ==="
echo "build  : min ${min}s  median ${median}s  mean ${mean}s"
echo "App    : ${app_bytes} bytes (${app_mib} MiB)"
