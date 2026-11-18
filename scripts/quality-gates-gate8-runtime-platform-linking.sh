#!/usr/bin/env bash
# Emperor quality gate 8: runtime-platform-linking
# Pipeline stage (8/13): runtime-platform-linking

set -u

GATE_NUMBER=8
GATE_NAME="runtime-platform-linking"
STAGE_DESC="runtime-platform-linking"
CMDS=(
  "cargo build -p emperor -p emperor-runtime --offline"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
