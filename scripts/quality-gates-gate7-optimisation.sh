#!/usr/bin/env bash
# Emperor quality gate 7: optimisation
# Pipeline stage (7/13): optimisation

set -u

GATE_NUMBER=7
GATE_NAME="optimisation"
STAGE_DESC="optimisation"
CMDS=(
  "cargo build -p emperor -p emperor-runtime --offline --release"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
