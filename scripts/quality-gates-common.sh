#!/usr/bin/env bash
# Shared runner for Emperor quality gate scripts.
# A gate script sets GATE_NUMBER, GATE_NAME, STAGE_DESC and (unless BLOCKED=1)
# the CMDS array, then sources this file. Each command is executed for real;
# the emitted JSON records only what actually ran and its actual exit status.
# No metric in the JSON is invented: every number comes from the command exit.

set -u

WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")/.." && pwd)"
METRICS_DIR="${WORKSPACE_DIR}/quality-gate-metrics"
mkdir -p "${METRICS_DIR}"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

if [[ "${BLOCKED:-0}" == "1" ]]; then
    # Genuinely not implementable in CI (no signing identity / release
    # pipeline). Follows the emperor.yml blocked-evidence convention: status
    # BLOCKED, exit 0, JSON must not say PASS.
    JSON=$(printf '{"gate": %s, "stage": "%s", "status": "BLOCKED", "exit_code": 0, "commands": [], "reason": "%s", "timestamp": "%s"}' \
        "${GATE_NUMBER}" "${GATE_NAME}" "${BLOCKED_REASON:-not implementable in CI}" "${TIMESTAMP}")
    echo "${JSON}"
    printf '%s\n' "${JSON}" > "${METRICS_DIR}/gate${GATE_NUMBER}-${GATE_NAME}.json"
    exit 0
fi

RUN_CMDS=()
RUN_EXITS=()
FAILED=0
for CMD in "${CMDS[@]}"; do
    echo "gate ${GATE_NUMBER} (${GATE_NAME}): running: ${CMD}"
    (exit 0)
    bash -c "${CMD}"
    RC=$?
    RUN_CMDS+=("${CMD}")
    RUN_EXITS+=("${RC}")
    if [[ "${RC}" -ne 0 ]]; then
        FAILED=1
    fi
done

STATUS="PASS"
if [[ "${FAILED}" -eq 1 ]]; then
    STATUS="FAILED"
fi

# Build a compact JSON array of {command, exit_code} from what actually ran.
DETAILS="["
first=1
for i in "${!RUN_CMDS[@]}"; do
    if [[ ${first} -eq 0 ]]; then DETAILS+=", "; fi
    first=0
    DETAILS+=$(printf '{"command": "%s", "exit_code": %s}' "${RUN_CMDS[$i]}" "${RUN_EXITS[$i]}")
done
DETAILS+="]"

JSON=$(printf '{"gate": %s, "stage": "%s", "status": "%s", "failed": %s, "commands": %s, "timestamp": "%s"}' \
    "${GATE_NUMBER}" "${GATE_NAME}" "${STATUS}" "${FAILED}" "${DETAILS}" "${TIMESTAMP}")
echo "${JSON}"
printf '%s\n' "${JSON}" > "${METRICS_DIR}/gate${GATE_NUMBER}-${GATE_NAME}.json"

if [[ "${FAILED}" -eq 1 ]]; then
    exit 1
fi
exit 0
