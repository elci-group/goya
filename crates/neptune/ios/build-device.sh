#!/bin/sh
set -eu

if [ "$(uname -s)" != Darwin ]; then
    printf '%s\n' 'BLOCKED: this project requires a Mac with Xcode and the iPhoneOS SDK.' >&2
    exit 3
fi
if ! xcrun --sdk iphoneos --show-sdk-path >/dev/null; then
    printf '%s\n' 'BLOCKED: Xcode iPhoneOS SDK is unavailable.' >&2
    exit 3
fi

cd -- "$(dirname -- "$0")"
build_dir="$PWD/.build"
xcodebuild -project NeptuneScene.xcodeproj -target NeptuneScene \
    -configuration Release -sdk iphoneos \
    "CONFIGURATION_BUILD_DIR=$build_dir" "OBJROOT=$build_dir/obj" \
    "SYMROOT=$build_dir/sym" CODE_SIGNING_ALLOWED=NO build

app="$build_dir/NeptuneScene.app"
if [ ! -f "$app/NeptuneScene" ] || [ ! -f "$app/Web/index.html" ]; then
    printf '%s\n' 'Xcode did not produce the executable and bundled scene.' >&2
    exit 1
fi
printf 'Unsigned device app: %s\n' "$app"
