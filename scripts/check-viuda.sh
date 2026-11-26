#!/usr/bin/env bash
# Viuda qualification (docs/viuda/VIUDA-1.md §6, §9).
#
# Without a host: tests and clippy for goya-viuda (software and `gpu`), then
# both TV runtimes, with their sizes and SHA-256:
# * software: static armv7-unknown-linux-musleabihf, no libraries at all;
# * gpu: armv7-unknown-linux-gnueabi (the TV's soft-float ABI), linked
#   against glibc only; EGL, GLES2 and libwayland are loaded at run time.
# The GPU test must find an EGL device here unless VIUDA_REQUIRE_GPU is set
# to an empty value.
# With a host (a Developer Mode TV): both runtimes run headless on the TV,
# then the GPU runtime is packaged, installed and launched, and its log is
# read after 35 s.
#
# Usage: scripts/check-viuda.sh [tv-host] [scene.gscene]
set -euo pipefail

host=${1:-}
scene=${2:-apps/pairs.gscene}
key=${WEBOS_KEY:-$HOME/.ssh/webos_tv_open}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
export VIUDA_REQUIRE_GPU=${VIUDA_REQUIRE_GPU-1}

cargo test -q -p goya-viuda --features pack
cargo test -q -p goya-viuda --features gpu,pack
cargo clippy -q -p goya-viuda --features pack --all-targets -- -D warnings
cargo clippy -q -p goya-viuda --features gpu,pack --all-targets -- -D warnings

export CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=0
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER=rust-lld \
  cargo build -q --release -p goya-viuda --bin viuda --target armv7-unknown-linux-musleabihf
cc=$(scripts/viuda-armel-sysroot.sh)
# softfp: the soft-float calling convention with VFP instructions.
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABI_LINKER=$cc \
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABI_RUSTFLAGS="-C target-cpu=cortex-a53 -C target-feature=-soft-float" \
  cargo build -q --release -p goya-viuda --features gpu --bin viuda --target armv7-unknown-linux-gnueabi
cargo build -q -p goya-viuda --features pack --bin viuda-pack
soft=target/armv7-unknown-linux-musleabihf/release/viuda
gpu=target/armv7-unknown-linux-gnueabi/release/viuda
for bin in "$soft" "$gpu"; do
  echo "runtime $bin: $(stat -c %s "$bin") bytes, sha256 $(sha256sum "$bin" | cut -d' ' -f1)"
done
[ -n "$host" ] || exit 0

ssh_tv() {
  ssh -p 9922 -i "$key" -o HostKeyAlgorithms=+ssh-rsa \
    -o PubkeyAcceptedAlgorithms=+ssh-rsa -o ConnectTimeout=8 -o BatchMode=yes \
    "prisoner@$host" "$@"
}
dir=/media/developer/temp/viuda-check
keys=ok,right,ok,ok,down,ok,right,ok,ok
ssh_tv "mkdir -p $dir && cat > $dir/viuda-soft && chmod +x $dir/viuda-soft" < "$soft"
ssh_tv "cat > $dir/viuda-gpu && chmod +x $dir/viuda-gpu" < "$gpu"
ssh_tv "cat > $dir/scene.gscene" < "$scene"
ssh_tv "cat /proc/loadavg; cd $dir && ./viuda-soft scene.gscene --headless --keys $keys"
# Offscreen EGL may need the compositor's display; a failure here is
# reported, and the launched app below is the real test.
ssh_tv "cd $dir && XDG_RUNTIME_DIR=/tmp/xdg ./viuda-gpu scene.gscene --headless --gpu --keys $keys || echo 'headless gpu unavailable'; rm -rf $dir"

out=$(mktemp -d)
./target/debug/viuda-pack --scene "$scene" --runtime "$gpu" --id com.goya.viuda.check \
  --title "Viuda check" --out "$out" --deploy "$host" --key "$key" --launch
sleep 35
# shellcheck disable=SC2016 # expanded on the TV
ssh_tv 'f=$(ls -t /tmp/viuda-com.goya.viuda.check-*.log | head -1); echo "== $f"; cat "$f"'
