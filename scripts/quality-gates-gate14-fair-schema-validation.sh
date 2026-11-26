#!/usr/bin/env bash
# Disparates quality gate 14: fair-schema-validation
# GY-FAIR/1 schema, validation and canonical serialisation tests.

set -u

GATE_NUMBER=14
GATE_NAME="fair-schema-validation"
STAGE_DESC="fair-schema-validation"
CMDS=(
  "cargo test -p goya-fair --offline"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
