#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "usage: $0" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$repo_root"
ARAGON_REQUIRE_KOTLIN=1 cargo test --offline \
    --manifest-path crates/goya-sdk/Cargo.toml \
    --test android_d1_runtime \
    emitted_kotlin_dispatch_matches_d1_transition_and_failure_atomicity_when_toolchain_exists \
    -- --exact --nocapture
