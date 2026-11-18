#!/usr/bin/env bash
# Emperor quality gate 2: type-semantic-validation
# Pipeline stage (2/13): type-semantic-validation

set -u

GATE_NUMBER=2
GATE_NAME="type-semantic-validation"
STAGE_DESC="type-semantic-validation"
CMDS=(
  "cargo test -p sash -p sash-ir --offline --tests"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
