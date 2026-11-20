#!/usr/bin/env bash
# Arachnid (webOS TV) offline qualification.
#
#   1. Unit tests: goya-arachnid and the webOS caprichios.
#   2. Build the media-centre fixture twice; the IPKs must be byte-identical.
#   3. Independent format check: GNU ar, gzip -t and tar must accept the IPK.
#   4. Remote runtime: headless Chrome drives 5-way, OK, pointer and
#      modality switching against the generated app (skipped, and reported
#      as BLOCKED, when no Chrome is installed).
#
# Nothing here touches a TV. Device install and launch are a separate,
# manual step: `arachnid getkey` + `arachnid deploy`.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cargo test -q -p goya-arachnid
cargo test -q -p goya-caprichios --lib webos
cargo build -q -p goya-arachnid
bin="$root/target/debug/arachnid"
fixture="crates/arachnid/fixtures/media-centre.gscene"
args=(--id com.goya.arachnid.media --version 0.1.0 --title "Goya Media Centre" --vendor Goya --target model:OLED65C3PUA)

"$bin" build "$fixture" "${args[@]}" --out "$work/a" >/dev/null
"$bin" build "$fixture" "${args[@]}" --out "$work/b" >/dev/null
ipk="com.goya.arachnid.media_0.1.0_all.ipk"
cmp "$work/a/$ipk" "$work/b/$ipk"
echo "reproducible: $(sha256sum "$work/a/$ipk" | cut -d' ' -f1)"

mkdir "$work/x"
(
  cd "$work/x"
  [ "$(ar t "../a/$ipk" | tr '\n' ' ')" = "debian-binary control.tar.gz data.tar.gz " ]
  ar x "../a/$ipk"
  [ "$(cat debian-binary)" = "2.0" ]
  gzip -t control.tar.gz data.tar.gz
  tar tzf control.tar.gz | grep -qx control
  tar tzf data.tar.gz | grep -qx "usr/palm/packages/com.goya.arachnid.media/packageinfo.json"
  tar tzf data.tar.gz | grep -qx "usr/palm/applications/com.goya.arachnid.media/appinfo.json"
)
echo "ipk format: accepted by ar, gzip and tar"

chrome="$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)"
if [ -z "$chrome" ]; then
  echo "remote runtime: BLOCKED (no Chrome/Chromium installed)"
  exit 0
fi
cp -r "$work/a/app" "$work/h"
cp crates/arachnid/tests/runtime/remote.js "$work/h/test.js"
sed -i 's#<script src="app.js"></script>#<script src="app.js"></script>\n<script src="test.js"></script>#' "$work/h/index.html"
results="$(timeout 60 "$chrome" --headless=new --disable-gpu --no-first-run \
  --user-data-dir="$work/profile" --dump-dom "file://$work/h/index.html" 2>/dev/null |
  sed -n '/<pre id="results">/,/<\/pre>/p' | sed 's#<[^>]*>##g')"
echo "$results"
[ -n "$results" ] && ! grep -q '^FAIL' <<<"$results"
echo "remote runtime: PASS ($(grep -c '^PASS' <<<"$results") checks, $("$chrome" --version))"
