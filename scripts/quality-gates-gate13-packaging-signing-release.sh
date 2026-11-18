#!/usr/bin/env bash
# Emperor quality gate 13: packaging-signing-release
# Pipeline stage (13/13): packaging-signing-release

set -u

GATE_NUMBER=13
GATE_NAME="packaging-signing-release"
STAGE_DESC="packaging-signing-release"
BLOCKED=1
BLOCKED_REASON="packaging and code signing require a release identity and artifact pipeline not available in CI"

source "$(dirname "${BASH_SOURCE[0]}")/quality-gates-common.sh"
