#!/usr/bin/env bash
# Disparates quality gate 16: disparate-coverage-assessment
# GY-COVERAGE/1 representability assessment and capability-deficit tests.

set -u

GATE_NUMBER=16
GATE_NAME="disparate-coverage-assessment"
STAGE_DESC="disparate-coverage-assessment"
CMDS=(
  "cargo test -p goya-disparate-coverage --offline"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
