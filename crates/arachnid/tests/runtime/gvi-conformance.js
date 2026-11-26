/* Node conformance check for the generated GY-GVI/0 runtime (gvi.js).

   Usage: node gvi-conformance.js GVI_JS INVENTORY.gvi RADICES SHA256 REFERENCE.json

   REFERENCE.json is what the reference decoder (goya_gvi::Inventory, via
   arachnid::graphics::reference_json) reports for every state: per item,
   the source item, the path segments, the style, the affine transform and
   the opacity. The runtime must reproduce it exactly, state by state. */
"use strict";
var fs = require("fs");
var api = require(require("path").resolve(process.argv[2]));
var bytes = new Uint8Array(fs.readFileSync(process.argv[3]));
var radices = process.argv[4].split(",").map(Number);
var sha = process.argv[5];
var reference = JSON.parse(fs.readFileSync(process.argv[6], "utf8"));

if (api.sha256(bytes) !== sha) {
  console.log("FAIL sha256: runtime " + api.sha256(bytes) + ", reference " + sha);
  process.exit(1);
}
var inv = api.openBytes(bytes, radices);
var fails = 0;
reference.forEach(function (entry) {
  var key = entry[0], want = entry[1], state = [], rest = key;
  for (var d = radices.length - 1; d >= 0; d--) { state[d] = rest % radices[d]; rest = Math.floor(rest / radices[d]); }
  var got = inv.resolve(state).map(function (it) {
    var st = inv.styles[it.style];
    return [it.item, inv.paths[it.geometry], [st.fill, st.stroke], it.t, it.opacity];
  });
  if (JSON.stringify(got) !== JSON.stringify(want)) {
    fails++;
    if (fails <= 3) console.log("FAIL state " + key + ":\n  got  " + JSON.stringify(got).slice(0, 400) + "\n  want " + JSON.stringify(want).slice(0, 400));
  }
});
console.log(fails ? "FAIL " + fails + " of " + reference.length + " states differ" : "PASS " + reference.length + " states match the reference decoder");
process.exit(fails ? 1 : 0);
