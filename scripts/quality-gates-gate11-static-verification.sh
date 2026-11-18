#!/usr/bin/env bash
# Emperor quality gate 11: static-verification
# Pipeline stage (11/13): static-verification

set -u

GATE_NUMBER=11
GATE_NAME="static-verification"
STAGE_DESC="static-verification"
CMDS=(
  "cargo clippy -p emperor -p emperor-runtime --all-targets --offline -- -D warnings"
  "cargo fmt -p emperor -p emperor-runtime -- --check"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
