#!/usr/bin/env bash
# Emperor quality gate 3: canonical-goya-ir
# Pipeline stage (3/13): canonical-goya-ir

set -u

GATE_NUMBER=3
GATE_NAME="canonical-goya-ir"
STAGE_DESC="canonical-goya-ir"
CMDS=(
  "cargo run -p emperor --offline -- check crates/sash/fixtures/interactive-counter.gscene --format json"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
