#!/usr/bin/env bash
# Viuda Phase 0 probe (docs/viuda/VIUDA-0.md §3).
#
# Cross-builds the canonical resolver (`goya-resolve`, GY-RESOLVER/0.1) as a
# static ARMv7 binary, copies it and one scene to the TV's Developer Mode temp
# directory, admits and runs the scene there, and removes both afterwards.
# The timings are indicative (5 runs, no load control). They are not a
# VIU-G06 benchmark.
#
# Usage: scripts/viuda-phase0-probe.sh <tv-host> <scene.gscene> [action]
# Needs: rustup target armv7-unknown-linux-musleabihf, the Developer Mode key
# at ~/.ssh/webos_tv_open (see crates/arachnid deploy).
set -euo pipefail

host=${1:?tv host}
scene=${2:?scene.gscene}
action=${3:-}
target=armv7-unknown-linux-musleabihf
key=${WEBOS_KEY:-$HOME/.ssh/webos_tv_open}
dir=/media/developer/temp/viuda-p0

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=0 \
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER=rust-lld \
  cargo build -q --release -p goya-resolver --bin goya-resolve --target "$target"
bin=target/$target/release/goya-resolve

ssh_tv() {
  ssh -p 9922 -i "$key" -o HostKeyAlgorithms=+ssh-rsa \
    -o PubkeyAcceptedAlgorithms=+ssh-rsa -o ConnectTimeout=8 -o BatchMode=yes \
    "prisoner@$host" "$@"
}

echo "binary: $(stat -c %s "$bin") bytes, sha256 $(sha256sum "$bin" | cut -d' ' -f1)"
ssh_tv "mkdir -p $dir && cat > $dir/goya-resolve && chmod +x $dir/goya-resolve" < "$bin"
ssh_tv "cat > $dir/scene.gscene" < "$scene"
trap 'ssh_tv "rm -rf $dir" || true' EXIT

ssh_tv "uname -m; cat /proc/loadavg; cd $dir && ./goya-resolve scene.gscene --validate-only"
ssh_tv "cd $dir && for i in 1 2 3 4 5; do timeout 10 time -v ./goya-resolve scene.gscene --validate-only 2>&1 >/dev/null \
  | grep -E 'Elapsed|Maximum resident' | tr '\n' ' '; echo; done"
if [ -n "$action" ]; then
  ssh_tv "cd $dir && ./goya-resolve scene.gscene --viewport 1920x1080 --dispatch $(printf %q "$action") --trace" \
    | grep -v '^action '
fi
