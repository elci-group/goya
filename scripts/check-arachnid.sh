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
args=(--id com.goya.arachnid.media --version 0.1.0 --title "Goya Media Centre" --vendor Goya --target model:OLED65C3PUA
      --icon crates/arachnid/assets/icon-80.png)

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
  # List to files first: `grep -q` exits early, and under pipefail the
  # SIGPIPE it gives a still-writing tar would fail the check.
  tar tzf control.tar.gz > control.list
  tar tzf data.tar.gz > data.list
  grep -qx control control.list
  grep -qx "usr/palm/packages/com.goya.arachnid.media/packageinfo.json" data.list
  grep -qx "usr/palm/applications/com.goya.arachnid.media/appinfo.json" data.list
  grep -qx "usr/palm/applications/com.goya.arachnid.media/arachnid-splash.png" data.list
)
echo "ipk format: accepted by ar, gzip and tar"

# Enforced splash: the logo is packaged byte-for-byte, the page shows it and
# the manifest records it.
cmp "$work/a/app/arachnid-splash.png" assets/arachnid-logo.png
grep -q '<div id="arachnid-splash"' "$work/a/app/index.html"
grep -q '"splash": {"contract": "arachnid.splash/0.0.1"' "$work/a/reports/compilation-manifest.json"
echo "splash: enforced (logo packaged, shown, recorded)"

chrome="$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)"
if [ -z "$chrome" ]; then
  echo "remote runtime: BLOCKED (no Chrome/Chromium installed)"
  exit 0
fi
cp -r "$work/a/app" "$work/h"
cp crates/arachnid/tests/runtime/remote.js "$work/h/test.js"
sed -i 's#<script src="app.js"></script>#<script src="app.js"></script>\n<script src="test.js"></script>#' "$work/h/index.html"
results="$(timeout 60 "$chrome" --headless=new --disable-gpu --no-first-run \
  --user-data-dir="$work/profile" --virtual-time-budget=5000 --dump-dom "file://$work/h/index.html" 2>/dev/null |
  sed -n '/<pre id="results">/,/<\/pre>/p' | sed 's#<[^>]*>##g')"
echo "$results"
[ -n "$results" ] && ! grep -q '^FAIL' <<<"$results"
echo "remote runtime: PASS ($(grep -c '^PASS' <<<"$results") checks, $("$chrome" --version))"

# goya.lanlink/0: the generated link runtime must reproduce the golden
# vector pinned in tapestry-desktop (ring).
"$bin" build crates/arachnid/fixtures/lanlink.gscene --id com.goya.arachnid.lanlink --version 0.1.0 --title "Lanlink fixture" --target reported:7.5.8 --out "$work/l" >/dev/null
mkdir "$work/v"
cp "$work/l/app/lanlink.js" crates/arachnid/tests/runtime/lanlink-vectors.html "$work/v/"
vectors="$(timeout 60 "$chrome" --headless=new --disable-gpu --allow-file-access-from-files --virtual-time-budget=5000 --dump-dom "file://$work/v/lanlink-vectors.html" 2>/dev/null | sed -n '/<pre id="results">/,/<\/pre>/p')"
[ -n "$vectors" ] && ! grep -q 'FAIL' <<<"$vectors" && grep -q 'PASS sealed confirm' <<<"$vectors"
echo "lanlink vectors: PASS ($(grep -c 'PASS' <<<"$vectors") checks)"

# Goya Pairs (apps/pairs.gscene): GSSH/4-live with stacked focus slots, a
# GY-GVI/0 graphic, durable state and D9 announcements. Built twice for
# reproducibility, then played with the stock remote in headless Chrome and
# relaunched on the same profile to check the durable record.
pairs_args=(--id com.goya.arachnid.pairs --version 0.1.0 --title "Goya Pairs" --vendor Goya --target reported:7.5.8
            --icon crates/arachnid/assets/icon-80.png)
"$bin" build apps/pairs.gscene "${pairs_args[@]}" --out "$work/p1" >/dev/null
"$bin" build apps/pairs.gscene "${pairs_args[@]}" --out "$work/p2" >/dev/null
pairs_ipk="com.goya.arachnid.pairs_0.1.0_all.ipk"
cmp "$work/p1/$pairs_ipk" "$work/p2/$pairs_ipk"
grep -q '"name": "card"' "$work/p1/reports/compilation-manifest.json"
[ -s "$work/p1/app/graphics/card.gvi" ] && [ -s "$work/p1/app/gvi.js" ]
echo "pairs: reproducible ($(sha256sum "$work/p1/$pairs_ipk" | cut -d' ' -f1)), GVI inventory packaged"
cp -r "$work/p1/app" "$work/ph"
cp crates/arachnid/tests/runtime/pairs.js "$work/ph/test.js"
sed -i 's#<script src="app.js"></script>#<script src="app.js"></script>\n<script src="test.js"></script>#' "$work/ph/index.html"
for mode in "" "#persist"; do
  played="$(timeout 120 "$chrome" --headless=new --disable-gpu --no-first-run --user-data-dir="$work/pairs-profile" \
    --virtual-time-budget=60000 --dump-dom "file://$work/ph/index.html$mode" 2>/dev/null |
    sed -n '/<pre id="results">/,/<\/pre>/p' | sed 's#<[^>]*>##g')"
  grep '^FAIL' <<<"$played" || true
  [ -n "$played" ] && ! grep -q '^FAIL' <<<"$played"
  echo "pairs runtime${mode:+ ($mode)}: PASS ($(grep -c '^PASS' <<<"$played") checks)"
done
