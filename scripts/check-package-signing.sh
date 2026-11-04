#!/bin/sh
set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
command -v openssl >/dev/null 2>&1 || {
    printf 'openssl is required for package-signing checks\n' >&2
    exit 1
}
command -v sha256sum >/dev/null 2>&1 || {
    printf 'sha256sum is required for package-signing checks\n' >&2
    exit 1
}

work=$(mktemp -d "${TMPDIR:-/tmp}/goya-signing-check.XXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
archive=$work/goya-fixture.tar.gz
installer=$repo_root/scripts/install-local.sh
mkdir -p "$work/goya-fixture/bin"
printf 'studio fixture\n' > "$work/goya-fixture/bin/goya-studio"
printf 'scene fixture\n' > "$work/goya-fixture/bin/goya-scene"
tar -czf "$archive" -C "$work" goya-fixture
(cd "$work" && sha256sum goya-fixture.tar.gz > goya-fixture.tar.gz.sha256)

openssl genpkey -algorithm ED25519 -out "$work/publisher-private.pem" >/dev/null 2>&1
openssl pkey -in "$work/publisher-private.pem" -pubout -out "$work/publisher-public.pem" >/dev/null 2>&1
openssl genpkey -algorithm ED25519 -out "$work/other-private.pem" >/dev/null 2>&1
openssl pkey -in "$work/other-private.pem" -pubout -out "$work/other-public.pem" >/dev/null 2>&1
openssl pkeyutl -sign -rawin -inkey "$work/publisher-private.pem" \
    -in "$archive" -out "$archive.sig"

sh "$installer" --verify-only --trusted-key "$work/publisher-public.pem" "$archive" > "$work/verified"
grep -q 'Ed25519 publisher signature' "$work/verified"
sh "$installer" --verify-only "$archive" > "$work/integrity-only"
grep -q 'publisher authenticity unchecked' "$work/integrity-only"

if sh "$installer" --verify-only --trusted-key "$work/other-public.pem" "$archive" >/dev/null 2>&1; then
    printf 'wrong trusted key unexpectedly verified\n' >&2
    exit 1
fi

cp "$archive.sig" "$work/good.sig"
printf x >> "$archive.sig"
if sh "$installer" --verify-only --trusted-key "$work/publisher-public.pem" "$archive" >/dev/null 2>&1; then
    printf 'modified signature unexpectedly verified\n' >&2
    exit 1
fi
cp "$work/good.sig" "$archive.sig"

printf 'tamper\n' >> "$archive"
(cd "$work" && sha256sum goya-fixture.tar.gz > goya-fixture.tar.gz.sha256)
if sh "$installer" --verify-only --trusted-key "$work/publisher-public.pem" "$archive" >/dev/null 2>&1; then
    printf 'archive changed after signing unexpectedly verified\n' >&2
    exit 1
fi

printf 'package signing checks passed\n'
