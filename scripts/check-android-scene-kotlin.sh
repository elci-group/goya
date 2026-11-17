#!/usr/bin/env bash
# Aragon Phase 3 differential check: compile the generated scene
# specification and D1 runtimes with kotlinc, run them on the JVM, and
# compare every result with the Rust references (goya_lang::layout,
# Frame::scroll_limits, handoff::convert_field, the canonical D1 executor).
# Requires kotlinc and java on PATH; a missing toolchain fails.
set -euo pipefail

if [[ $# -ne 0 ]]; then
    echo "usage: $0" >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
ARAGON_REQUIRE_KOTLIN=1 cargo test --offline \
    --manifest-path crates/goya-sdk/Cargo.toml \
    --test android_scene \
    generated_kotlin_matches_the_rust_references_on_the_jvm_when_toolchain_exists \
    -- --exact --nocapture
