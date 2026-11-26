/* Node harness for the generated live-sky runtime (sky.js, `sky: local`).
   Used by scripts/check-arachnid.sh: node sky.js <built app>/sky.js
   It checks the astronomy against reference values, the time-zone lookup,
   the atmosphere model's colours, and that exposure follows the light. */
"use strict";
global.window = {};
require(require("path").resolve(process.argv[2]));
var G = window.GoyaSky, R = Math.PI / 180, fails = 0, passes = 0;
function check(name, ok, detail) {
  if (ok) passes++; else fails++;
  console.log((ok ? "PASS " : "FAIL ") + name + (detail === undefined ? "" : ": " + detail));
}
function deg(x) { return (x / R).toFixed(2); }

var london = G.locate("Europe/London", 0);
check("Europe/London resolves", london.source === "zone" && london.lat === 51.51 && london.lon === -0.13, JSON.stringify(london));
check("ICU legacy name Asia/Calcutta resolves", G.locate("Asia/Calcutta", -330).lat === 22.53);
var utc = G.locate("UTC", -60);
check("UTC falls back to the offset (UTC+1 is 15 E, 35 N)", utc.source === "offset" && utc.lon === 15 && utc.lat === 35, JSON.stringify(utc));
check("unknown zone falls back to the offset", G.locate("Mars/Olympus", 300).lon === -75);

/* NOAA reference: London, 21 June 2026 12:00 UTC, elevation 61.9, azimuth 178.9. */
var s = G.sun(Date.UTC(2026, 5, 21, 12, 0), london.lat, london.lon);
check("solstice noon elevation", Math.abs(s.alt / R - 61.92) < 0.2, deg(s.alt));
check("solstice noon azimuth", Math.abs(s.az / R - 178.85) < 0.5, deg(s.az));
/* London sunset on 26 November 2026 is 15:55 GMT (upper limb, refraction). */
s = G.sun(Date.UTC(2026, 10, 26, 15, 55), london.lat, london.lon);
check("geometric altitude at sunset", s.alt / R < 0 && s.alt / R > -1.2, deg(s.alt));
check("November sunset is in the south-west", s.az / R > 225 && s.az / R < 245, deg(s.az));
/* Equator, March equinox, 06:00 UTC: the Sun is just below the east point. */
s = G.sun(Date.UTC(2026, 2, 20, 6, 0), 0, 0);
check("equinox sunrise is due east", Math.abs(s.az / R - 90) < 1, deg(s.az));

/* Full Moon 24 November 2026, new Moon 9 November 2026. */
check("full Moon illuminated", G.moon(Date.UTC(2026, 10, 24, 14, 0), 51.5, 0).illum > 0.97);
check("new Moon dark", G.moon(Date.UTC(2026, 10, 9, 12, 0), 51.5, 0).illum < 0.03);

var noonSun = [0, Math.sin(40 * R), Math.cos(40 * R)];
var zen = G.radiance([0, 1, 0], noonSun);
check("daytime zenith is blue", zen[2] > zen[1] && zen[1] > zen[0], zen.map(function (x) { return x.toFixed(3); }).join(","));
var low = [0, Math.sin(1 * R), Math.cos(1 * R)];
var glow = G.radiance([0, Math.sin(3 * R), Math.cos(3 * R)], low);
check("sky towards a setting Sun is red", glow[0] > glow[1] && glow[1] > glow[2], glow.map(function (x) { return x.toFixed(3); }).join(","));
var dark = G.radiance([0, 1, 0], [0, Math.sin(-30 * R), Math.cos(-30 * R)]);
check("no sunlight with the Sun 30 degrees down", dark[0] + dark[1] + dark[2] < 1e-9);

var t = function (iso) { return G.lighting(Date.parse(iso), london); };
var noon = t("2026-11-26T12:00Z"), dusk = t("2026-11-26T16:40Z"), night = t("2026-11-26T22:00Z");
check("exposure rises from noon to dusk", dusk.exposure > noon.exposure * 50, noon.exposure.toPrecision(3) + " -> " + dusk.exposure.toPrecision(3));
check("the Sun lights the day, the Moon the night", noon.useSun && dusk.useSun && !night.useSun);
check("night factor", noon.night === 0 && night.night === 1);
var ms = night.ms;
check("night fill is blue", ms[2] > ms[1] && ms[1] > ms[0]);

console.log("sky: " + (fails ? "FAIL" : "PASS") + " (" + passes + " checks)");
process.exit(fails ? 1 : 0);
