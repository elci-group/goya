#!/usr/bin/env bash
# Emperor quality gate 12: runtime-conformance
# Pipeline stage (12/13): runtime-conformance

set -u

GATE_NUMBER=12
GATE_NAME="runtime-conformance"
STAGE_DESC="runtime-conformance"
CMDS=(
  "cargo run -p emperor --offline -- test crates/sash/fixtures/interactive-counter.gscene --suite conformance --format json"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
