#!/usr/bin/env bash
# Emperor quality gate 1: lexical-syntactic-admission
# Pipeline stage (1/13): lexical-syntactic-admission

set -u

GATE_NUMBER=1
GATE_NAME="lexical-syntactic-admission"
STAGE_DESC="lexical-syntactic-admission"
CMDS=(
  "cargo test -p emperor -p sash -p sash-ir --offline --lib"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
