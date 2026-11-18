#!/usr/bin/env bash
# Emperor quality gate 13: packaging-signing-release
# Pipeline stage (13/13): packaging-signing-release
#
# Two honest parts:
#   packaging — a real end-to-end run: build the admitted counter to a fresh
#   directory, then `emperor package --format linux-archive`. PASS iff the
#   archive exists and is non-empty.
#   signing   — genuinely not implementable in compiler 0.0.1 (no signing
#   identity). Follows the emperor.yml blocked-evidence convention: the
#   sub-part is BLOCKED and the JSON carries no PASS string for it.
# Overall gate exit is 0 iff packaging passed; signing BLOCKED is not a
# failure. Top-level status is PASS only when packaging succeeded.

set -u

GATE_NUMBER=13
GATE_NAME="packaging-signing-release"
STAGE_DESC="packaging-signing-release"

WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
METRICS_DIR="${WORKSPACE_DIR}/quality-gate-metrics"
mkdir -p "${METRICS_DIR}"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

FIXTURE="crates/sash/fixtures/interactive-counter.gscene"
BUILD_DIR="$(mktemp -d)/counter-build"
trap 'rm -rf "$(dirname "${BUILD_DIR}")"' EXIT

PACKAGING_RESULT="FAILED"
PACKAGING_DETAIL="packaging did not complete"
ARCHIVE=""

echo "gate 13 (${GATE_NAME}): running: cargo run -p emperor --offline -- build ${FIXTURE} --out <tmp>/counter-build"
BUILD_EXIT=0
bash -c "cd '${WORKSPACE_DIR}' && cargo run -p emperor --offline -- build '${FIXTURE}' --out '${BUILD_DIR}'" >/dev/null 2>&1 || BUILD_EXIT=$?

if [[ "${BUILD_EXIT}" -eq 0 && -f "${BUILD_DIR}/App" ]]; then
    echo "gate 13 (${GATE_NAME}): running: emperor package <build-dir> --format linux-archive"
    PACKAGE_EXIT=0
    bash -c "cd '${WORKSPACE_DIR}' && cargo run -p emperor --offline -- package '${BUILD_DIR}' --format linux-archive" >/dev/null 2>&1 || PACKAGE_EXIT=$?
    ARCHIVE="$(dirname "${BUILD_DIR}")/$(basename "${BUILD_DIR}").tar.gz"
    if [[ "${PACKAGE_EXIT}" -eq 0 && -s "${ARCHIVE}" ]]; then
        PACKAGING_RESULT="PASS"
        PACKAGING_DETAIL="linux-archive produced: $(basename "${ARCHIVE}") ($(stat -c%s "${ARCHIVE}") bytes)"
    else
        PACKAGING_DETAIL="package exited ${PACKAGE_EXIT} or archive missing/empty"
    fi
else
    PACKAGING_DETAIL="build exited ${BUILD_EXIT} or build dir has no App"
fi

# Signing is genuinely unimplemented in compiler 0.0.1. Never claims PASS.
SIGNING_RESULT="BLOCKED"
SIGNING_DETAIL="signing is not implemented in compiler 0.0.1"

if [[ "${PACKAGING_RESULT}" == "PASS" ]]; then
    STATUS="PASS"
    EXIT_CODE=0
else
    STATUS="FAILED"
    EXIT_CODE=1
fi

JSON=$(printf '{"gate": %s, "stage": "%s", "status": "%s", "exit_code": %s, "commands": [], "parts": [{"part": "packaging", "result": "%s", "detail": "%s"}, {"part": "signing", "result": "%s", "detail": "%s"}], "timestamp": "%s"}' \
    "${GATE_NUMBER}" "${GATE_NAME}" "${STATUS}" "${EXIT_CODE}" \
    "${PACKAGING_RESULT}" "${PACKAGING_DETAIL}" \
    "${SIGNING_RESULT}" "${SIGNING_DETAIL}" \
    "${TIMESTAMP}")
echo "${JSON}"
printf '%s\n' "${JSON}" > "${METRICS_DIR}/gate${GATE_NUMBER}-${GATE_NAME}.json"

exit "${EXIT_CODE}"
