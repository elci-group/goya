#!/usr/bin/env bash
# Emperor quality gate 10: elf-assembly-linking
# Pipeline stage (10/13): elf-assembly-linking

set -u

GATE_NUMBER=10
GATE_NAME="elf-assembly-linking"
STAGE_DESC="elf-assembly-linking"
CMDS=(
  "cargo build -p emperor-runtime --offline --release"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
