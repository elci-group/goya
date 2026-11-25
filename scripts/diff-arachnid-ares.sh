#!/usr/bin/env bash
# Structural differential: Arachnid's IPK writer vs LG's ares-package.
#
#   scripts/diff-arachnid-ares.sh <arachnid-build-dir>
#
# <arachnid-build-dir> is an `arachnid build --out` directory (it holds the
# .ipk and app/). ares-package packages the same app/ directory, and the two
# IPKs are compared on what the installer reads: ar members, control
# fields, data.tar paths, types, modes and file bytes, and packageinfo.json.
# Expected differences (mtimes, ownership, compression, Installed-Size,
# packager version) are reported but not counted as failures.
set -euo pipefail
build="$(cd "$1" && pwd)"
command -v ares-package >/dev/null || { echo "BLOCKED: ares-package not installed"; exit 2; }
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
ares-package --no-minify "$build/app" -o "$work/ares" >/dev/null
ours="$(ls "$build"/*.ipk)"; theirs="$(ls "$work"/ares/*.ipk)"
echo "ours:   $(basename "$ours")   theirs: $(basename "$theirs")"
for side in ours theirs; do
  mkdir "$work/$side"; ( cd "$work/$side"; ar x "${!side}"; ar t "${!side}" > members
    mkdir c d; tar xzf control.tar.gz -C c; tar xzf data.tar.gz -C d
    tar tzvf data.tar.gz | awk '{print $1, $NF}' | sed 's#^\(.\)\([^ ]*\) #\1\2 #' | sed 's#/$##' | sort -k2 > listing )
done
fail=0
check() { if diff -u "$work/ours/$1" "$work/theirs/$1" > "$work/d"; then echo "SAME   $2"; else echo "DIFF   $2"; sed 's/^/       /' "$work/d"; [ "${3:-hard}" = soft ] || fail=1; fi; }
check members "ar members and order"
grep -v -e '^Installed-Size' -e '^webOS-Packager-Version' "$work/ours/c/control" > "$work/ours/control.core"
grep -v -e '^Installed-Size' -e '^webOS-Packager-Version' "$work/theirs/c/control" > "$work/theirs/control.core"
check control.core "control fields (except Installed-Size, packager version)"
grep -h -e '^Installed-Size' "$work/ours/c/control" "$work/theirs/c/control" | paste -sd' ' | sed 's/^/INFO   Installed-Size ours vs theirs: /'
check listing "data.tar paths, types and modes"
id="$(sed -n 's/^Package: //p' "$work/theirs/c/control")"
(cd "$work/ours/d" && find . -type f -exec sha256sum {} + | sort -k2) > "$work/ours/sums"
(cd "$work/theirs/d" && find . -type f -exec sha256sum {} + | sort -k2) > "$work/theirs/sums"
check sums "file bytes inside data.tar (incl. packageinfo.json for $id)"
for side in ours theirs; do gzip -l "$work/$side/data.tar.gz" | tail -1 | awk -v s=$side '{print "INFO   data.tar.gz " s ": compressed " $1 ", raw " $2}'; done
[ $fail = 0 ] && echo "RESULT: structurally identical" || { echo "RESULT: differences above"; exit 1; }
