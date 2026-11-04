#!/bin/sh
set -eu

usage() {
    cat <<'EOF'
Usage: install-local.sh [--dest DIRECTORY] [--force] [--verify-only]
       [--trusted-key ED25519_PUBLIC_KEY.pem] ARCHIVE.tar.gz

Verify ARCHIVE.tar.gz against ARCHIVE.tar.gz.sha256, validate its member
paths, and install goya-studio and goya-scene into DIRECTORY (default:
~/.local/bin). SHA-256 checks integrity only. Supplying --trusted-key also
requires and verifies ARCHIVE.tar.gz.sig as an Ed25519 publisher signature.
--force replaces existing regular files. --verify-only performs checks
without installing anything.
EOF
}

fail() {
    printf 'install-local.sh: %s\n' "$*" >&2
    exit 1
}

dest=
force=no
verify_only=no
archive=
trusted_key=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --dest)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            dest=$2
            shift 2
            ;;
        --force)
            force=yes
            shift
            ;;
        --verify-only)
            verify_only=yes
            shift
            ;;
        --trusted-key)
            [ "$#" -ge 2 ] || { usage >&2; exit 2; }
            [ -z "$trusted_key" ] || { usage >&2; exit 2; }
            trusted_key=$2
            shift 2
            ;;
        --)
            shift
            [ "$#" -eq 1 ] || { usage >&2; exit 2; }
            archive=$1
            shift
            ;;
        -*|"")
            usage >&2
            exit 2
            ;;
        *)
            [ -z "$archive" ] || { usage >&2; exit 2; }
            archive=$1
            shift
            ;;
    esac
done
[ -n "$archive" ] || { usage >&2; exit 2; }

command -v tar >/dev/null 2>&1 || fail 'tar is required'
command -v sha256sum >/dev/null 2>&1 || fail 'sha256sum is required'

case "$archive" in
    /*) ;;
    *) archive=$(pwd)/$archive ;;
esac
[ -f "$archive" ] || fail "archive not found: $archive"
checksum_file=$archive.sha256
[ -f "$checksum_file" ] || fail "checksum sidecar not found: $checksum_file"

# The producer emits the standard sha256sum format with the archive basename.
archive_name=${archive##*/}
expected=
listed_name=
extra=
IFS=' ' read -r expected listed_name extra < "$checksum_file" || :
[ -n "$expected" ] && [ "$listed_name" = "$archive_name" ] && [ -z "$extra" ] ||
    fail 'checksum sidecar must name this archive in sha256sum format'
[ "${#expected}" -eq 64 ] || fail 'checksum must contain 64 hexadecimal digits'
case "$expected" in
    *[!0-9a-fA-F]*) fail 'checksum contains a non-hexadecimal character' ;;
esac
actual=$(sha256sum "$archive" | awk '{ print $1 }')
[ "$actual" = "$expected" ] || fail 'SHA-256 checksum mismatch'

if [ -n "$trusted_key" ]; then
    [ -f "$trusted_key" ] || fail "trusted public key not found: $trusted_key"
    command -v openssl >/dev/null 2>&1 || fail 'openssl is required for Ed25519 signature verification'
    signature=$archive.sig
    [ -f "$signature" ] || fail "signature sidecar not found: $signature"
    openssl pkeyutl -verify -pubin -inkey "$trusted_key" -rawin \
        -in "$archive" -sigfile "$signature" >/dev/null 2>&1 ||
        fail 'Ed25519 signature verification failed'
    authenticity=verified
else
    authenticity=unchecked
fi

tmp_root=${TMPDIR:-/tmp}
work=$(mktemp -d "$tmp_root/goya-install.XXXXXX") || fail 'could not create temporary directory'
cleanup() { rm -rf "$work"; }
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

LC_ALL=C
export LC_ALL
tar -tzf "$archive" > "$work/members" || fail 'could not read archive member list'
[ -s "$work/members" ] || fail 'archive is empty'

root=
while IFS= read -r member || [ -n "$member" ]; do
    [ -n "$member" ] || fail 'archive contains an empty member name'
    case "$member" in
        /*) fail "archive contains an absolute path: $member" ;;
        *[!A-Za-z0-9._/-]*) fail 'archive member names contain unsupported characters' ;;
    esac
    case "/$member/" in
        */../*) fail "archive contains a parent traversal path: $member" ;;
    esac
    case "$member" in
        .|./*|*/./*|*/.) fail "archive contains a dot path component: $member" ;;
        *//*) fail "archive contains an empty path component: $member" ;;
    esac
    case "$member" in
        */*) member_root=${member%%/*} ;;
        *) member_root=$member ;;
    esac
    [ -n "$member_root" ] || fail 'archive contains an empty top-level path'
    if [ -z "$root" ]; then
        root=$member_root
    elif [ "$member_root" != "$root" ]; then
        fail 'archive must contain exactly one top-level directory'
    fi
    case "$member" in
        "$root") fail 'archive top-level entry must be a directory' ;;
        "$root/"|"$root/"*) ;;
        *) fail 'archive member escapes its top-level directory' ;;
    esac
done < "$work/members"

# Only regular files and directories are expected in this bundle. This also
# excludes symlinks and hard links that could redirect extraction or install.
tar -tvzf "$archive" > "$work/details" || fail 'could not inspect archive entry types'
while IFS= read -r detail || [ -n "$detail" ]; do
    entry_type=${detail%${detail#?}}
    case "$entry_type" in
        -|d) ;;
        *) fail 'archive may contain only regular files and directories' ;;
    esac
done < "$work/details"

[ "$verify_only" = no ] || {
    if [ "$authenticity" = verified ]; then
        printf 'verified: %s (SHA-256 integrity and Ed25519 publisher signature)\n' "$archive"
    else
        printf 'integrity verified: %s (SHA-256 only; publisher authenticity unchecked)\n' "$archive"
    fi
    exit 0
}

[ -n "$dest" ] || {
    [ -n "${HOME:-}" ] || fail 'HOME is unset; pass --dest DIRECTORY'
    dest=$HOME/.local/bin
}
mkdir -p "$dest" || fail "could not create destination: $dest"
[ -d "$dest" ] || fail "destination is not a directory: $dest"

tar -xzf "$archive" -C "$work" || fail 'archive extraction failed'
package_dir=$work/$root
studio=$package_dir/bin/goya-studio
scene=$package_dir/bin/goya-scene
[ -f "$studio" ] && [ ! -L "$studio" ] || fail 'package is missing regular bin/goya-studio'
[ -f "$scene" ] && [ ! -L "$scene" ] || fail 'package is missing regular bin/goya-scene'

check_target() {
    target=$1
    if [ -e "$target" ] || [ -L "$target" ]; then
        [ "$force" = yes ] || fail "refusing to overwrite $target (pass --force to replace it)"
        [ -f "$target" ] && [ ! -L "$target" ] || fail "refusing to replace non-regular file: $target"
    fi
}
check_target "$dest/goya-studio"
check_target "$dest/goya-scene"

install_one() {
    source=$1
    target=$2
    staged=$(mktemp "$dest/.goya-install.XXXXXX") || fail "could not stage $target"
    if ! cp "$source" "$staged" || ! chmod 755 "$staged"; then
        rm -f "$staged"
        fail "could not prepare $target"
    fi
    if [ "$force" = yes ]; then
        mv -f "$staged" "$target" || { rm -f "$staged"; fail "could not install $target"; }
    else
        # Linking a staged file gives a no-clobber final step even if another
        # process creates the target after the preflight check.
        ln "$staged" "$target" || { rm -f "$staged"; fail "refusing to overwrite $target"; }
        rm -f "$staged"
    fi
}

install_one "$studio" "$dest/goya-studio"
install_one "$scene" "$dest/goya-scene"
if [ "$authenticity" = verified ]; then
    printf 'installed goya-studio and goya-scene to %s (Ed25519 signature verified)\n' "$dest"
else
    printf 'installed goya-studio and goya-scene to %s (SHA-256 integrity only; publisher authenticity unchecked)\n' "$dest"
fi
