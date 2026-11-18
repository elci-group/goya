#!/usr/bin/env bash
# Aggregate the per-gate JSON emitted by the quality gate scripts into a
# single summary. Reads only files the gates actually wrote under
# quality-gate-metrics/ — no metric here is invented.

set -u

WORKSPACE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
METRICS_DIR="${WORKSPACE_DIR}/quality-gate-metrics"
SUMMARY_FILE="${METRICS_DIR}/summary.json"

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

python3 -I - "$METRICS_DIR" "${SUMMARY_FILE}" "${STAGES[@]}" <<'PYEOF'
import json, sys, datetime, pathlib

metrics_dir = pathlib.Path(sys.argv[1])
summary_file = pathlib.Path(sys.argv[2])
stages = sys.argv[3:]

gates = []
for i, stage in enumerate(stages, start=1):
    path = metrics_dir / f"gate{i}-{stage}.json"
    if path.is_file():
        try:
            data = json.loads(path.read_text())
        except json.JSONDecodeError as exc:
            data = {"gate": i, "stage": stage, "status": "INVALID_JSON", "error": str(exc)}
        data.setdefault("gate", i)
        data.setdefault("stage", stage)
        gates.append(data)
    else:
        gates.append({"gate": i, "stage": stage, "status": "NO_EVIDENCE"})

counts = {}
for g in gates:
    counts[g["status"]] = counts.get(g["status"], 0) + 1

summary = {
    "timestamp_utc": datetime.datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ"),
    "total_gates": len(gates),
    "status_counts": counts,
    "gates": gates,
}
summary_file.write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))
PYEOF
