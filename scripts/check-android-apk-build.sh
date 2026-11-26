#!/usr/bin/env bash
# Aragon ARG-G03/ARG-G04 qualification: build the generated static-text,
# root-button and compiled-scene projects into unsigned release APKs with the locked toolchain
# (docs/aragon/android-toolchain-lock.md), twice each from fresh copies at
# different paths, offline, with strict Gradle dependency verification.
# Every APK must be byte-identical to its twin and to the recorded hash.
#
# Exit status: 0 qualified, 1 failed, 2 toolchain missing or not the locked one.
#   --allow-network   permit Gradle to download into an empty cache (still
#                     verified against gradle/verification-metadata.xml)
set -euo pipefail

offline=(--offline)
case $# in
    0) ;;
    1) [[ "$1" == --allow-network ]] && offline=() || { echo "usage: $0 [--allow-network]" >&2; exit 2; } ;;
    *) echo "usage: $0 [--allow-network]" >&2; exit 2 ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Expected unsigned release APK SHA-256 per qualification scene.
expected_static=0b100c527fd01473800c09765437a4a769ef7bd727a30dda79b4e815ecedf280
expected_button=53f482a25861780f733f30e00292dd1018adfea4f1b2508d7e9c3dab208bacf5
expected_scene=6e2ccc85d8cea8413335460a482c048026cc6cd92b3781daa33f9057b7227b1a

fail_toolchain() { echo "toolchain: $*" >&2; exit 2; }

java_cmd="${JAVA_HOME:+$JAVA_HOME/bin/java}"
[[ -n "$java_cmd" && -x "$java_cmd" ]] || fail_toolchain "set JAVA_HOME to the locked JDK"
java_version="$("$java_cmd" -XshowSettings:properties -version 2>&1 | sed -n 's/^ *java.runtime.version = //p')"
[[ "$java_version" == "21.0.13+9-LTS" ]] || fail_toolchain "JDK is $java_version, lock requires Temurin 21.0.13+9"

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
[[ -n "$sdk" && -d "$sdk" ]] || fail_toolchain "set ANDROID_HOME to the locked SDK"
revision() { sed -n 's/^Pkg.Revision=//p' "$sdk/$1/source.properties" 2>/dev/null; }
[[ "$(revision platforms/android-35)" == 2 ]] || fail_toolchain "platforms;android-35 must be revision 2"
[[ "$(revision build-tools/34.0.0)" == 34.0.0 ]] || fail_toolchain "build-tools;34.0.0 missing"
[[ "$(revision platform-tools)" == 37.0.1 ]] || fail_toolchain "platform-tools must be 37.0.1"
export ANDROID_HOME="$sdk"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp:?}"' EXIT

printf '%s\n' '@ontology demo { @cell label { id: "label"; text: "Hello Android" } }' \
    > "$tmp/static.gscene"
printf '%s\n' '@ontology demo { @button increment { id: "button.increment"; text: "Increment" } @interaction increment { id: "action.increment"; trigger: button.increment.activate; class: D1; state { write count: int; } input { } set count = 1; } }' \
    > "$tmp/button.gscene"
cp "$repo_root/crates/goya-sdk/tests/android-device/counter.gscene" "$tmp/scene.gscene"

cd "$repo_root"
cargo build --offline --quiet --manifest-path crates/goya-sdk/Cargo.toml --bin aragon-android-project
generator="$repo_root/target/debug/aragon-android-project"

build() { # scene output [generator args...]
    local scene="$1" out="$2"; shift 2
    "$generator" "$scene" "$out" "$@"
    (cd "$out" && ./gradlew --no-daemon --quiet --console=plain "${offline[@]}" \
        --dependency-verification strict assembleRelease)
    sha256sum "$out/app/build/outputs/apk/release/app-release-unsigned.apk" | cut -d' ' -f1
}

status=0
check() { # name expected scene [generator args...]
    local name="$1" expected="$2" scene="$3"; shift 3
    local a b
    a="$(build "$scene" "$tmp/$name-a" "$@")"
    b="$(build "$scene" "$tmp/second copy/$name-b" "$@")"
    echo "$name: $a"
    if [[ "$a" != "$b" ]]; then
        echo "$name: APKs from two fresh builds differ ($b)" >&2
        status=1
    elif [[ "$a" != "$expected" ]]; then
        echo "$name: APK differs from the recorded hash $expected" >&2
        status=1
    fi
}

mkdir -p "$tmp/second copy"
check static "$expected_static" "$tmp/static.gscene"
check root-button "$expected_button" "$tmp/button.gscene" --root-button --initial count=int:0
check scene "$expected_scene" "$tmp/scene.gscene" --scene

if (( status == 0 )); then
    echo 'Locked-toolchain release APK builds are reproducible and match the recorded hashes.'
    echo 'Unsigned APKs only: no emulator, device, signing or packaging qualification.'
fi
exit $status
