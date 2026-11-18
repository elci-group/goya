#!/usr/bin/env bash
# Emperor quality gate 4: effects-capabilities
# Pipeline stage (4/13): effects-capabilities

set -u

GATE_NUMBER=4
GATE_NAME="effects-capabilities"
STAGE_DESC="effects-capabilities"
CMDS=(
  "cargo test -p emperor -p sash -p sash-ir --offline --lib --tests"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
