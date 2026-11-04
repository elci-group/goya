#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
version=$(tr -d '\r\n' < "$repo_root/VERSION")
workspace_version=$(awk '
    /^\[workspace\.package\]/ { in_workspace_package=1; next }
    /^\[/ { in_workspace_package=0 }
    in_workspace_package && /^version = / { gsub(/[" ]/, "", $3); print $3; exit }
' "$repo_root/Cargo.toml")
if [ "$version" != "$workspace_version" ]; then
    printf 'version mismatch: VERSION=%s Cargo workspace=%s\n' "$version" "$workspace_version" >&2
    exit 1
fi
epoch=${SOURCE_DATE_EPOCH:-0}
out_dir=${1:-"$repo_root/dist"}
arch=$(uname -m)
package_name="goya-${version}-linux-${arch}"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT HUP INT TERM

mkdir -p "$stage/$package_name/bin" "$stage/$package_name/share/doc/goya"
(cd "$repo_root" && cargo build --release -p goya-studio -p goya-hello --bin goya-studio --bin scene)
cp "$repo_root/target/release/goya-studio" "$stage/$package_name/bin/goya-studio"
cp "$repo_root/target/release/scene" "$stage/$package_name/bin/goya-scene"
cp "$repo_root/README.md" "$stage/$package_name/share/doc/goya/README.md"
cp "$repo_root/docs/sdk-studio.md" "$stage/$package_name/share/doc/goya/SDK-STUDIO.md"
cp "$repo_root/LICENSE-MIT" "$stage/$package_name/share/doc/goya/"
cp "$repo_root/LICENSE-APACHE" "$stage/$package_name/share/doc/goya/"

mkdir -p "$out_dir"
archive="$out_dir/$package_name.tar.gz"
tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -C "$stage" -cf - "$package_name" | gzip -n > "$archive"
(cd "$out_dir" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
printf 'package: %s\nchecksum: %s.sha256\n' "$archive" "$archive"
