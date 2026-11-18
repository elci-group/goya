#!/usr/bin/env bash
# Emperor quality gate 9: machine-code-generation
# Pipeline stage (9/13): machine-code-generation

set -u

GATE_NUMBER=9
GATE_NAME="machine-code-generation"
STAGE_DESC="machine-code-generation"
CMDS=(
  "cargo run -p emperor --offline -- check crates/sash/fixtures/interactive-counter.gscene --target x86_64-unknown-linux-gnu --format json"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
