#!/usr/bin/env bash
# Emperor quality gate 5: target-negotiation
# Pipeline stage (5/13): target-negotiation

set -u

GATE_NUMBER=5
GATE_NAME="target-negotiation"
STAGE_DESC="target-negotiation"
CMDS=(
  "cargo build -p emperor -p emperor-runtime --offline"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
