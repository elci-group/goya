// Rebuild the Phase 1 fixtures from literals and refuse to print them
// unless decoding restores the same model. This file does not read the
// Rust encoder's output.

import { createHash } from "node:crypto";
import * as rcb from "./rcb.mjs";

function fail(message) {
  console.error(message);
  process.exit(1);
}

function id(tag) {
  const buf = Buffer.alloc(16);
  buf[15] = tag;
  return buf;
}

function field(rec, id) {
  const hit = rec.v.find(([key]) => key === id);
  if (!hit) fail(`missing field ${id}`);
  return hit[1];
}

function ints(value) {
  if (value.t !== "array") fail("expected int list");
  return value.v.map((item) => {
    if (item.t !== "int") fail("expected int");
    return item.v;
  });
}

// id, class, wire. Names stay in this comment: board, matched, selected,
// moves, score, phase, difficulty, reveal_pending, viewport, pointer, secret.
const PAIRS_FIELDS = [
  [1, 1, 6],
  [2, 6, 6],
  [3, 1, 6],
  [4, 6, 2],
  [5, 6, 2],
  [6, 6, 5],
  [7, 1, 5],
  [8, 7, 2],
  [9, 2, 2],
  [10, 4, 2],
  [11, 5, 4],
];

function intList(values) {
  return rcb.array(values.map((value) => rcb.int(value)));
}

function pairsBody(state) {
  return rcb.record([
    [1, intList(state.board)],
    [2, intList(state.matched)],
    [3, intList(state.selected)],
    [4, rcb.int(state.moves)],
    [5, rcb.int(state.score)],
    [6, rcb.enumeration(state.phase)],
    [7, rcb.enumeration(state.difficulty)],
    [8, rcb.int(state.reveal)],
  ]);
}

const deck = [0, 1, 2, 3, 3, 2, 1, 0];
const openState = {
  board: deck,
  matched: [],
  selected: [],
  moves: 0,
  score: 0,
  phase: 1,
  difficulty: 1,
  reveal: 0,
};
const matchedState = {
  board: deck,
  matched: [0, 7],
  selected: [],
  moves: 1,
  score: 1,
  phase: 1,
  difficulty: 1,
  reveal: 0,
};

const types = rcb.record([
  [
    1,
    rcb.array([
      rcb.bool(false),
      rcb.int(-9223372036854775808n),
      rcb.float(1.5),
      rcb.text("ñ"),
      rcb.enumeration(7),
    ]),
  ],
  [
    2,
    rcb.map([
      ["b", rcb.int(-2)],
      ["a", rcb.bool(true)],
    ]),
  ],
  [3, rcb.optional(rcb.ref(9n))],
  [4, rcb.optional(null)],
  [5, rcb.bytes([1, 2, 3])],
  [6, rcb.timestamp(-1n)],
]);

if (rcb.encodeDocument(rcb.bool(true)).toString("hex") !== "524342310101") {
  fail("bool true is not RCB1 01 01");
}
if (!rcb.encodeDocument(rcb.float(0)).equals(rcb.encodeDocument(rcb.float(-0)))) {
  fail("negative zero was not canonicalised");
}
const flipped = rcb.encodeDocument(
  rcb.record([
    [2, rcb.bool(true)],
    [1, rcb.bool(false)],
  ]),
);
const ordered = rcb.encodeDocument(
  rcb.record([
    [1, rcb.bool(false)],
    [2, rcb.bool(true)],
  ]),
);
if (!flipped.equals(ordered)) fail("record field order changed the bytes");

const schemaDoc = rcb.schemaDocument(PAIRS_FIELDS);
for (const name of ["board", "matched", "viewport", "secret", "reveal_pending"]) {
  if (schemaDoc.includes(Buffer.from(name))) fail(`schema document contains ${name}`);
}
const fingerprint = createHash("sha256").update(schemaDoc).digest();
if (!fingerprint.equals(rcb.schemaFingerprint(PAIRS_FIELDS))) {
  fail("schema fingerprint drifted from the schema document");
}

function snapshot(body, revision) {
  return rcb.encodeSnapshot({
    session: id(9),
    version: 1,
    fingerprint,
    epoch: 1n,
    revision,
    authority: id(1),
    body,
  });
}

const typesDoc = rcb.encodeDocument(types);
const openBody = rcb.encodeDocument(pairsBody(openState));
const matchedBody = rcb.encodeDocument(pairsBody(matchedState));
const openSnap = snapshot(openBody, 0n);
const matchedSnap = snapshot(matchedBody, 2n);

const decodedTypes = rcb.decodeDocument(typesDoc);
const showcase = field(decodedTypes, 1);
if (showcase.v.length !== 5) fail("type showcase length");
if (showcase.v[0].v !== false) fail("bool");
if (showcase.v[1].v !== -9223372036854775808n) fail("i64 min");
if (showcase.v[2].v !== 1.5) fail("float");
if (showcase.v[3].v !== "ñ") fail("text");
if (showcase.v[4].v !== 7) fail("enum");
const decodedMap = field(decodedTypes, 2);
if (decodedMap.v[0][0] !== "a" || decodedMap.v[0][1].v !== true) fail("map a");
if (decodedMap.v[1][0] !== "b" || decodedMap.v[1][1].v !== -2n) fail("map b");
if (field(decodedTypes, 3).v.t !== "ref" || field(decodedTypes, 3).v.v !== 9n) fail("ref");
if (field(decodedTypes, 4).v !== null) fail("optional none");
if (!field(decodedTypes, 5).v.equals(Buffer.from([1, 2, 3]))) fail("bytes");
if (field(decodedTypes, 6).v !== -1n) fail("timestamp");

function expectPairs(body, state) {
  const decoded = rcb.decodeDocument(body);
  const board = ints(field(decoded, 1)).map(Number);
  const matched = ints(field(decoded, 2)).map(Number);
  if (board.join(",") !== state.board.join(",")) fail("board");
  if (matched.join(",") !== state.matched.join(",")) fail("matched");
  if (ints(field(decoded, 3)).length !== state.selected.length) fail("selected");
  if (field(decoded, 4).v !== BigInt(state.moves)) fail("moves");
  if (field(decoded, 5).v !== BigInt(state.score)) fail("score");
  if (field(decoded, 6).v !== state.phase) fail("phase");
  if (field(decoded, 7).v !== state.difficulty) fail("difficulty");
  if (field(decoded, 8).v !== BigInt(state.reveal)) fail("reveal");
  if (decoded.v.some(([key]) => key > 8)) fail("non-portable field in snapshot body");
}

expectPairs(openBody, openState);
expectPairs(matchedBody, matchedState);

function expectSnap(bytes, body, revision) {
  const decoded = rcb.decodeSnapshot(bytes);
  if (decoded.session[15] !== 9 || decoded.authority[15] !== 1) fail("identity");
  if (decoded.version !== 1 || decoded.epoch !== 1n || decoded.revision !== revision) {
    fail("snapshot header");
  }
  if (!decoded.fingerprint.equals(fingerprint)) fail("schema fingerprint");
  if (!decoded.body.equals(body)) fail("snapshot body");
  if (bytes[24] !== fingerprint[0]) fail("fingerprint offset");
}

expectSnap(openSnap, openBody, 0n);
expectSnap(matchedSnap, matchedBody, 2n);

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("types", typesDoc);
line("open-body", openBody);
line("open-snap", openSnap);
line("matched-body", matchedBody);
line("matched-snap", matchedSnap);
console.log("equivalent");
