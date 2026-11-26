#!/usr/bin/env bash
# Link sysroot for Viuda's GPU runtime (docs/viuda/VIUDA-1.md §9).
#
# The TV's userland uses the ARM soft-float calling convention
# (`/lib/ld-linux.so.3`), which is Debian's armel. The GPU runtime is a
# dynamically linked `armv7-unknown-linux-gnueabi` binary that loads EGL,
# GLES2 and libwayland at run time, so it links against glibc only. These
# Debian stretch packages (glibc 2.24) supply the link-time stubs and
# start-up objects; on the TV the binary binds to the TV's own libraries.
#
# Writes target/armel-sysroot and target/armel-sysroot/armel-cc, the linker
# for CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABI_LINKER. Idempotent.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
out=$root/target/armel-sysroot
debs=$root/target/armel-debs
base=http://archive.debian.org/debian/pool/main

# SHA-256 as listed in Debian's stretch binary-armel Packages index (checked
# 26 November 2026; the index signature was not verified).
packages=(
  "g/glibc/libc6_2.24-11+deb9u4_armel.deb 601950cfcf0e9f705d9a2bff65351dd6e86759cb6573d5ca6ba6cbe98ffaf5d8"
  "g/glibc/libc6-dev_2.24-11+deb9u4_armel.deb cd747f52bf0e39a7fd4f7e41508239c779079c40bc39a466372f5bba10c0933f"
  "g/gcc-6/libgcc1_6.3.0-18+deb9u1_armel.deb 387f00835349688e6ca6f50610de202be10b498855a6f7e39a90204701448763"
  "g/gcc-6/libgcc-6-dev_6.3.0-18+deb9u1_armel.deb bc5f84f85f75652ab9b28adae6c2e8fc8ab437458463cde3624bb2b6ac09922f"
)

if [ -x "$out/armel-cc" ]; then
  echo "$out/armel-cc"
  exit 0
fi
mkdir -p "$debs"
rm -rf "$out.tmp"
mkdir -p "$out.tmp"
for p in "${packages[@]}"; do
  path=${p% *}
  sum=${p#* }
  file=$debs/$(basename "$path")
  [ -f "$file" ] || curl -sfo "$file" "$base/$path"
  echo "$sum  $file" | sha256sum -c --quiet -
  dpkg-deb -x "$file" "$out.tmp"
done

# Absolute symlinks (libpthread.so -> /lib/...) would resolve on the build
# host; lld would then fall back to the static archives.
(cd "$out.tmp" && find . -type l -lname '/*' | while read -r l; do
  t=$(readlink "$l")
  ln -sfn "$(realpath -m --relative-to="$(dirname "$l")" ".$t")" "$l"
done)

lld=$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld
mkdir -p "$out.tmp/bin"
ln -sf "$lld" "$out.tmp/bin/ld.lld"
cat > "$out.tmp/armel-cc" <<EOF
#!/bin/sh
PATH=$out/bin:\$PATH exec clang --target=arm-linux-gnueabi --sysroot=$out -fuse-ld=lld "\$@"
EOF
chmod +x "$out.tmp/armel-cc"
rm -rf "$out"
mv "$out.tmp" "$out"
echo "$out/armel-cc"
