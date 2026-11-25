// Independent capability record for a controller surface.
// This file does not read Rust output. The record is four booleans:
// display, interactive input, touch, and large display. Touch is absent.
// There is no resolution, sensor, or operating-system name. The card intent
// is a separate record and is not part of this advertisement.

import * as rcb from "./rcb.mjs";

function fail(message) {
  console.error(message);
  process.exit(1);
}

const capabilities = rcb.encodeDocument(
  rcb.record([
    [1, rcb.bool(false)],
    [2, rcb.bool(true)],
    [3, rcb.bool(false)],
    [4, rcb.bool(false)],
  ]),
);

const decoded = rcb.decodeDocument(capabilities);
if (decoded.t !== "record" || decoded.v.length !== 4) fail("record");
const fields = new Map(decoded.v.map(([field, value]) => [field, value]));
if (fields.get(1).v !== false || fields.get(2).v !== true) fail("input");
if (fields.get(3).v !== false || fields.get(4).v !== false) fail("preference");

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("capabilities", capabilities);
console.log("equivalent");
