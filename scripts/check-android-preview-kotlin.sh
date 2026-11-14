#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gradle_cache="${GRADLE_USER_HOME:-$HOME/.gradle}/caches/modules-2/files-2.1"
compiler_version=2.0.21

find_jar() {
    local artifact="$1" version="$2" found
    found="$(find "$gradle_cache" -type f -path "*/$artifact/$version/*/$artifact-$version.jar" -print -quit 2>/dev/null || true)"
    if [[ -z "$found" ]]; then
        echo "missing cached Kotlin compiler dependency: $artifact:$version" >&2
        echo "This check is offline; populate the Gradle cache separately, then retry." >&2
        exit 2
    fi
    printf '%s' "$found"
}

compiler="$(find_jar kotlin-compiler-embeddable "$compiler_version")"
stdlib="$(find_jar kotlin-stdlib 1.9.23)"
daemon="$(find_jar kotlin-daemon-embeddable "$compiler_version")"
trove="$(find_jar trove4j 1.0.20200330)"
coroutines="$(find_jar kotlinx-coroutines-core-jvm 1.6.4)"
annotations="$(find_jar annotations 13.0)"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cd "$repo_root"
cargo run --offline --quiet --manifest-path crates/goya-sdk/Cargo.toml \
    --example emit_android_preview -- "$tmp/GoyaStaticTextPreview.kt"

compiler_cp="$compiler:$stdlib:$daemon:$trove:$coroutines:$annotations"
java -cp "$compiler_cp" org.jetbrains.kotlin.cli.jvm.K2JVMCompiler \
    -no-stdlib -no-reflect -classpath "$stdlib" -d "$tmp/classes" \
    crates/goya-sdk/tests/android-preview-stubs/android/content/Context.kt \
    crates/goya-sdk/tests/android-preview-stubs/android/widget/TextView.kt \
    "$tmp/GoyaStaticTextPreview.kt"

echo "Offline Kotlin syntax/type check passed for generated Android static-text preview (Kotlin $compiler_version; Android API stubs only)."
