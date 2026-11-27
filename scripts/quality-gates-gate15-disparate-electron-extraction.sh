#!/usr/bin/env bash
# Disparates quality gate 15: disparate-electron-extraction
# GY-FAIR/1 semantic extraction of foreign (Electron) applications.

set -u

GATE_NUMBER=15
GATE_NAME="disparate-electron-extraction"
STAGE_DESC="disparate-electron-extraction"
CMDS=(
  "cargo test -p goya-disparate-electron --offline"
)
source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
