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
expected_static=cd3e6f15892278b35490a3d46cb90109d069e870d9b20ec7e506c6d584f9b06b
expected_button=47af8d6c0932c4aa6e19b0ee7576d8887eda269f056a8e40934e71e1efbab89e
expected_scene=ddabde96440df056f3a638f1d740df9fb4ee81362f9f320ba775afe00feb1754

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
cat > "$tmp/scene.gscene" <<'SCENE'
@ontology counter {
  @state { total: int = 0 session; note: text = "none" session; }
  @cell root { id: "root"; fill: #f4f1ea;
    @cell title { id: "title"; position: (16, 16, 0); extent: (300, 40, 0); text: "Goya counter" }
    @input amount { id: "field.amount"; position: (16, 64, 0); extent: (200, 48, 0); }
    @input memo { id: "field.memo"; position_p: (0.05, 0.3, 0); extent_p: (0.9, 0.08, 0); }
    @button add { id: "button.add"; position: (232, 64, 0); extent: (120, 48, 0); text: "Add" }
    @cell list { id: "list"; position_p: (0.05, 0.45, 0); extent_p: (0.9, 0.5, 0); scroll: true; fill: #ffffff;
      @cell para { id: "para"; extent: (280, 900, 0); wrap: 28; text: "Scroll cells clip their children and let them overflow; this paragraph is wrapped by the reference wrapper." }
    }
  }
  @interaction add { id: "action.add"; trigger: button.add.activate; class: D1;
    state { read total: int; write total: int; write note: text; }
    input { amount: int from "field.amount"; memo: text from "field.memo"; }
    set total = state.total + input.amount; set note = input.memo; }
}
SCENE

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
