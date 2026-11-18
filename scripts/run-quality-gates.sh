#!/usr/bin/env bash
# Emperor Quality Gates master controller.
# Runs all 13 gate scripts (one per stage in crates/emperor/src/stage.rs::NAMES),
# collects the honest JSON each gate emits, and exits 1 only if a non-BLOCKED
# gate's command actually failed. Blocked gates (status BLOCKED) exit 0 by
# convention, matching .github/workflows/emperor.yml blocked-evidence jobs.

set -u

WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE_SCRIPTS_DIR="${WORKSPACE_DIR}/scripts"
METRICS_DIR="${WORKSPACE_DIR}/quality-gate-metrics"
mkdir -p "${METRICS_DIR}"

STAGES=(
    "lexical-syntactic-admission"
    "type-semantic-validation"
    "canonical-goya-ir"
    "effects-capabilities"
    "target-negotiation"
    "native-linux-lowering"
    "optimisation"
    "runtime-platform-linking"
    "machine-code-generation"
    "elf-assembly-linking"
    "static-verification"
    "runtime-conformance"
    "packaging-signing-release"
)

echo "=== Emperor Quality Gates ==="
echo "Timestamp: $(date -u +%Y-%m-%dT%H:%M:%SZ)"

passed=0
failed=0
blocked=0
missing=0
results=()

x=0
while [ "${x}" -lt "${#STAGES[@]}" ]; do
    gate_number=$((x + 1))
    gate_name="${STAGES[$x]}"
    script_path="${GATE_SCRIPTS_DIR}/quality-gates-gate${gate_number}-${gate_name}.sh"

    echo "--- Gate ${gate_number}: ${gate_name} ---"

    if [ ! -f "${script_path}" ]; then
        echo "ERROR: gate script not found: ${script_path}"
        missing=$((missing + 1))
        results+=("${gate_number}:${gate_name}:MISSING")
        x=$((x + 1))
        continue
    fi

    if bash "${script_path}"; then
        if grep -q '"status": "BLOCKED"' "${METRICS_DIR}/gate${gate_number}-${gate_name}.json" 2>/dev/null; then
            blocked=$((blocked + 1))
            results+=("${gate_number}:${gate_name}:BLOCKED")
        else
            passed=$((passed + 1))
            results+=("${gate_number}:${gate_name}:PASS")
        fi
    else
        failed=$((failed + 1))
        results+=("${gate_number}:${gate_name}:FAILED")
    fi

    x=$((x + 1))
done

echo ""
echo "=== Quality Gates Summary ==="
echo "Results:"
for r in "${results[@]}"; do
    gate_id="${r%%:*}"
    rest="${r#*:}"
    gate_stage="${rest%%:*}"
    gate_status="${rest#*:}"
    echo "  ${gate_id} ${gate_stage} -> ${gate_status}"
done
echo "Passed: ${passed}  Failed: ${failed}  Blocked: ${blocked}  Missing: ${missing}"

if [ "${failed}" -gt 0 ] || [ "${missing}" -gt 0 ]; then
    echo "=== Quality Gates Execution: FAILURE ==="
    exit 1
fi

echo "=== Quality Gates Execution: SUCCESS (blocked gates are honest evidence, not passes) ==="
exit 0
