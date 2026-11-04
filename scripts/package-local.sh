#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
usage() {
    cat <<'EOF'
Usage: package-local.sh [--signing-key ED25519_PRIVATE_KEY.pem] [--out-dir DIRECTORY]
       package-local.sh [output-directory] [--signing-key ED25519_PRIVATE_KEY.pem]

Build a reproducible Linux tar.gz package and SHA-256 sidecar. If a signing
key is supplied, also create ARCHIVE.tar.gz.sig using OpenSSL Ed25519.
EOF
}
out_dir=
signing_key=
positional_out=no
while [ "$#" -gt 0 ]; do
    case "$1" in
        --signing-key)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            signing_key=$2
            shift 2
            ;;
        --out-dir)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            [ -z "$out_dir" ] || { usage >&2; exit 2; }
            out_dir=$2
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        -*|"")
            usage >&2
            exit 2
            ;;
        *)
            [ "$positional_out" = no ] && [ -z "$out_dir" ] || { usage >&2; exit 2; }
            out_dir=$1
            positional_out=yes
            shift
            ;;
    esac
done
out_dir=${out_dir:-"$repo_root/dist"}
if [ -n "$signing_key" ]; then
    [ -f "$signing_key" ] || { printf 'signing key not found: %s\n' "$signing_key" >&2; exit 1; }
    command -v openssl >/dev/null 2>&1 || { printf 'openssl is required for Ed25519 signing\n' >&2; exit 1; }
fi
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
installer="$out_dir/$package_name-install.sh"
tar --sort=name --mtime="@$epoch" --owner=0 --group=0 --numeric-owner \
    -C "$stage" -cf - "$package_name" | gzip -n > "$archive"
(cd "$out_dir" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
if [ -n "$signing_key" ]; then
    signature="$archive.sig"
    signature_tmp=$(mktemp "$out_dir/.${package_name}.sig.XXXXXX")
    if ! openssl pkeyutl -sign -rawin -inkey "$signing_key" -in "$archive" -out "$signature_tmp"; then
        rm -f "$signature_tmp"
        printf 'could not sign package archive\n' >&2
        exit 1
    fi
    mv -f "$signature_tmp" "$signature"
fi
cp "$repo_root/scripts/install-local.sh" "$installer"
chmod 755 "$installer"
printf 'package: %s\nchecksum: %s.sha256 (integrity only)\n' "$archive" "$archive"
if [ -n "$signing_key" ]; then
    printf 'signature: %s.sig (Ed25519)\n' "$archive"
fi
printf 'installer: %s\n' "$installer"
