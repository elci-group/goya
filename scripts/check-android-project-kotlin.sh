#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gradle_cache="${GRADLE_USER_HOME:-$HOME/.gradle}/caches/modules-2/files-2.1"
compiler_version=2.0.21
require_kotlin=false
if [[ "${1-}" == "--require-kotlin" ]]; then
    require_kotlin=true
elif [[ $# -gt 0 ]]; then
    echo "usage: $0 [--require-kotlin]" >&2
    exit 2
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cd "$repo_root"
printf '%s\n' '@ontology demo { @cell label { id: "label"; text: "Hello Android" } }' > "$tmp/qualification.gscene"
cargo run --offline --quiet --manifest-path crates/goya-sdk/Cargo.toml \
    --bin aragon-android-project -- "$tmp/qualification.gscene" "$tmp/project-a"
cargo run --offline --quiet --manifest-path crates/goya-sdk/Cargo.toml \
    --bin aragon-android-project -- "$tmp/qualification.gscene" "$tmp/project-b"

diff -r "$tmp/project-a" "$tmp/project-b" >/dev/null
for expected in \
    app/src/main/AndroidManifest.xml \
    app/src/main/java/dev/goya/preview/GoyaStaticTextPreview.kt \
    app/src/main/java/dev/goya/preview/MainActivity.kt \
    app/build.gradle.kts build.gradle.kts settings.gradle.kts gradle.properties; do
    [[ -s "$tmp/project-a/$expected" ]] || {
        echo "generated Android project is missing $expected" >&2
        exit 1
    }
done
echo 'Deterministic generated Android project structure check passed.'

find_jar() {
    local artifact="$1" version="$2"
    find "$gradle_cache" -type f \
        -path "*/$artifact/$version/*/$artifact-$version.jar" -print -quit 2>/dev/null || true
}

java_cmd="${JAVA_HOME:+$JAVA_HOME/bin/java}"
if [[ -z "$java_cmd" || ! -x "$java_cmd" ]]; then
    java_cmd="$(command -v java 2>/dev/null || true)"
fi
compiler="$(find_jar kotlin-compiler-embeddable "$compiler_version")"
stdlib="$(find_jar kotlin-stdlib 1.9.23)"
daemon="$(find_jar kotlin-daemon-embeddable "$compiler_version")"
trove="$(find_jar trove4j 1.0.20200330)"
coroutines="$(find_jar kotlinx-coroutines-core-jvm 1.6.4)"
annotations="$(find_jar annotations 13.0)"

missing=()
[[ -n "$java_cmd" ]] || missing+=("Java runtime")
[[ -n "$compiler" ]] || missing+=("cached Kotlin compiler $compiler_version")
[[ -n "$stdlib" ]] || missing+=("cached Kotlin stdlib 1.9.23")
[[ -n "$daemon" ]] || missing+=("cached Kotlin daemon $compiler_version")
[[ -n "$trove" ]] || missing+=("cached trove4j 1.0.20200330")
[[ -n "$coroutines" ]] || missing+=("cached coroutines 1.6.4")
[[ -n "$annotations" ]] || missing+=("cached annotations 13.0")

if (( ${#missing[@]} > 0 )); then
    if [[ "$require_kotlin" == true ]]; then
        printf 'Kotlin compilation unavailable: missing %s\n' "$(IFS=', '; echo "${missing[*]}")" >&2
        exit 2
    fi
    printf 'Generated Kotlin compilation skipped (missing %s).\n' "$(IFS=', '; echo "${missing[*]}")"
    echo 'This result does not qualify an Android SDK build.'
    exit 0
fi

compiler_cp="$compiler:$stdlib:$daemon:$trove:$coroutines:$annotations"
mapfile -t kotlin_sources < <(find "$tmp/project-a/app/src/main/java" -name '*.kt' -type f | sort)
java -cp "$compiler_cp" org.jetbrains.kotlin.cli.jvm.K2JVMCompiler \
    -no-stdlib -no-reflect -classpath "$stdlib" -d "$tmp/classes" \
    "$repo_root/crates/goya-sdk/tests/android-preview-stubs/android/content/Context.kt" \
    "$repo_root/crates/goya-sdk/tests/android-preview-stubs/android/widget/TextView.kt" \
    "$repo_root/crates/goya-sdk/tests/android-preview-stubs/android/os/Bundle.kt" \
    "$repo_root/crates/goya-sdk/tests/android-preview-stubs/android/app/Activity.kt" \
    "${kotlin_sources[@]}"

echo "Generated Android project Kotlin compilation passed (Kotlin $compiler_version; Android API stubs only)."
echo 'This result does not qualify Gradle, an Android SDK, APK packaging, or device behavior.'
