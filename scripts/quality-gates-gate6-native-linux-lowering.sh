#!/usr/bin/env bash
# Emperor quality gate 6: native-linux-lowering
# Pipeline stage (6/13): native-linux-lowering

set -u

GATE_NUMBER=6
GATE_NAME="native-linux-lowering"
STAGE_DESC="native-linux-lowering"
CMDS=(
  "cargo build -p emperor -p emperor-runtime --offline"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
