#!/usr/bin/env bash
# Viuda qualification (docs/viuda/VIUDA-1.md §6).
#
# Without a host: tests and clippy for goya-viuda, then the static ARMv7
# runtime build, its size and SHA-256.
# With a host (a Developer Mode TV): a headless run on the TV, then the
# packaged app is installed and launched, and its log is read after 35 s.
#
# Usage: scripts/check-viuda.sh [tv-host] [scene.gscene]
set -euo pipefail

host=${1:-}
scene=${2:-apps/pairs.gscene}
target=armv7-unknown-linux-musleabihf
key=${WEBOS_KEY:-$HOME/.ssh/webos_tv_open}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

cargo test -q -p goya-viuda --features pack
cargo clippy -q -p goya-viuda --features pack --all-targets -- -D warnings
CARGO_PROFILE_RELEASE_STRIP=symbols CARGO_PROFILE_RELEASE_DEBUG=0 \
CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER=rust-lld \
  cargo build -q --release -p goya-viuda --bin viuda --target "$target"
cargo build -q -p goya-viuda --features pack --bin viuda-pack
bin=target/$target/release/viuda
echo "runtime: $(stat -c %s "$bin") bytes, sha256 $(sha256sum "$bin" | cut -d' ' -f1)"
[ -n "$host" ] || exit 0

ssh_tv() {
  ssh -p 9922 -i "$key" -o HostKeyAlgorithms=+ssh-rsa \
    -o PubkeyAcceptedAlgorithms=+ssh-rsa -o ConnectTimeout=8 -o BatchMode=yes \
    "prisoner@$host" "$@"
}
dir=/media/developer/temp/viuda-check
ssh_tv "mkdir -p $dir && cat > $dir/viuda && chmod +x $dir/viuda" < "$bin"
ssh_tv "cat > $dir/scene.gscene" < "$scene"
ssh_tv "cat /proc/loadavg; cd $dir && ./viuda scene.gscene --headless --keys ok,right,ok,ok,down,ok,right,ok,ok; rm -rf $dir"

out=$(mktemp -d)
./target/debug/viuda-pack --scene "$scene" --runtime "$bin" --id com.goya.viuda.check \
  --title "Viuda check" --out "$out" --deploy "$host" --key "$key" --launch
sleep 35
# shellcheck disable=SC2016 # expanded on the TV
ssh_tv 'f=$(ls -t /tmp/viuda-com.goya.viuda.check-*.log | head -1); echo "== $f"; cat "$f"'
