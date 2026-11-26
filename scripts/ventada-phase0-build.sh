#!/usr/bin/env bash
# Ventada Phase 0: cross-link the canonical resolver (goya-resolve) for
# Windows x64 and ARM64 from Linux, with a static CRT, and print the import
# closure. Nothing is launched: this host has no Windows.
#
# Needs cargo-xwin. XWIN_SDK_VERSION is pinned because xwin's default
# (Win11SDK_10.0.26100) failed to resolve headers on 2026-11-25.
set -euo pipefail
cd "$(dirname "$0")/.."

export XWIN_SDK_VERSION="${XWIN_SDK_VERSION:-10.0.22621}"
export RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/ventada-static}"

readobj=$(command -v llvm-readobj || find "$(rustc --print sysroot)" "$HOME/.rustup/toolchains" -name llvm-readobj -type f 2>/dev/null | head -1)

for triple in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
  cargo xwin build -q --release -p goya-resolver --bin goya-resolve --target "$triple"
  exe="$CARGO_TARGET_DIR/$triple/release/goya-resolve.exe"
  echo "== $triple"
  echo "bytes  $(stat -c %s "$exe")"
  echo "sha256 $(sha256sum "$exe" | cut -d' ' -f1)"
  if [ -n "$readobj" ]; then
    "$readobj" --coff-imports "$exe" | sed -n 's/^ *Name: \(.*\.dll\)$/import \1/p' | sort -u
  fi
done
