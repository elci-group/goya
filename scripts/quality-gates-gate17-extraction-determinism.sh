#!/usr/bin/env bash
# Disparates quality gate 17: extraction-determinism
#
# End-to-end determinism evidence for GY-FAIR/1 extraction: build the
# disparate-extract binary if missing (cargo build -p goya-disparate-electron
# --offline), run it twice over the in-repo fixture Electron application
# (crates/goya-disparate-electron/fixtures/sample-electron-app — never foreign
# application code), and cmp the two canonical FAIR outputs. The gate PASSes
# only if both runs exit 0 and the two canonical documents are byte-identical.
# The emitted JSON records only what actually ran and its actual exit status.

set -u

GATE_NUMBER=17
GATE_NAME="extraction-determinism"
STAGE_DESC="extraction-determinism"

WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${WORKSPACE_DIR}"
METRICS_DIR="${WORKSPACE_DIR}/quality-gate-metrics"
mkdir -p "${METRICS_DIR}"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

FIXTURE_APP="crates/goya-disparate-electron/fixtures/sample-electron-app"
BIN="target/debug/disparate-extract"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT
OUT1="${TMP_DIR}/run1.fair.json"
OUT2="${TMP_DIR}/run2.fair.json"

RUN_CMDS=()
RUN_EXITS=()
FAILED=0

record() {
    echo "gate ${GATE_NUMBER} (${GATE_NAME}): running: ${1}"
    (exit 0)
    bash -c "${1}"
    RC=$?
    RUN_CMDS+=("${1}")
    RUN_EXITS+=("${RC}")
    if [[ "${RC}" -ne 0 ]]; then
        FAILED=1
    fi
}

# Build the extractor first if the binary is missing.
record "test -x ${BIN} || cargo build -p goya-disparate-electron --offline"

# Run the extractor twice over the in-repo fixture app.
record "${BIN} ${FIXTURE_APP} ${OUT1} >/dev/null"
record "${BIN} ${FIXTURE_APP} ${OUT2} >/dev/null"

# Byte-identical canonical output or the gate fails.
record "cmp ${OUT1} ${OUT2}"

DETAILS="["
first=1
for i in "${!RUN_CMDS[@]}"; do
    if [[ ${first} -eq 0 ]]; then DETAILS+=", "; fi
    first=0
    DETAILS+=$(printf '{"command": "%s", "exit_code": %s}' "${RUN_CMDS[$i]}" "${RUN_EXITS[$i]}")
done
DETAILS+="]"

STATUS="PASS"
if [[ "${FAILED}" -eq 1 ]]; then
    STATUS="FAILED"
fi

JSON=$(printf '{"gate": %s, "stage": "%s", "status": "%s", "failed": %s, "commands": %s, "timestamp": "%s"}' \
    "${GATE_NUMBER}" "${GATE_NAME}" "${STATUS}" "${FAILED}" "${DETAILS}" "${TIMESTAMP}")
echo "${JSON}"
printf '%s\n' "${JSON}" > "${METRICS_DIR}/gate${GATE_NUMBER}-${GATE_NAME}.json"

if [[ "${FAILED}" -eq 1 ]]; then
    exit 1
fi
exit 0
