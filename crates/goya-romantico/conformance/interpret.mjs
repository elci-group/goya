// Independent interpreter of one Goya Pairs session.
// This file does not read Rust output and does not import the other
// conformance scripts. The compiled writer selects card 0. Authority then
// moves to this interpreter: epoch 2, origin the phone, revision still 1.
// The interpreter selects card 7. The continuity contract is nine fields.
// It has no execution mode, no target name, and no action alias.

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

function u32(value) {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(value, 0);
  return buf;
}

function u64(value) {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(BigInt(value), 0);
  return buf;
}

function intList(values) {
  return rcb.array(values.map((value) => rcb.int(value)));
}

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

const deck = [0, 1, 2, 3, 3, 2, 1, 0];
const state = {
  selected: [],
  matched: [],
  moves: 0,
  score: 0,
  phase: 1,
};

function selectCard(card) {
  if (state.phase !== 1 || card >= deck.length) fail("illegal");
  if (state.matched.includes(card) || state.selected.includes(card)) fail("illegal");
  state.selected = state.selected.concat([card]);
  if (state.selected.length === 1) {
    return [{ field: 3, bare: rcb.encodeBare(intList(state.selected)) }];
  }
  if (state.selected.length !== 2) fail("illegal");
  state.moves += 1;
  const left = state.selected[0];
  const right = state.selected[1];
  if (deck[left] !== deck[right]) fail("mismatch");
  state.matched = state.matched.concat(state.selected).sort((a, b) => a - b);
  state.score += 1;
  state.phase = state.score === deck.length / 2 ? 3 : 1;
  const matched = state.matched.slice();
  const score = state.score;
  const moves = state.moves;
  const phase = state.phase;
  state.selected = [];
  return [
    { field: 3, bare: rcb.encodeBare(intList([])) },
    { field: 2, bare: rcb.encodeBare(intList(matched)) },
    { field: 5, bare: rcb.encodeBare(rcb.int(score)) },
    { field: 4, bare: rcb.encodeBare(rcb.int(moves)) },
    { field: 6, bare: rcb.encodeBare(rcb.enumeration(phase)) },
    { field: 8, bare: rcb.encodeBare(rcb.int(0)) },
  ];
}

function encodeOps(ops) {
  const parts = [u32(ops.length)];
  for (const op of ops) parts.push(Buffer.from([1]), u32(op.field), op.bare);
  return Buffer.concat(parts);
}

function envelope(origin, epoch, base, revision, ops) {
  const operations = encodeOps(ops);
  const preimage = Buffer.concat([
    Buffer.from("GY-ROMANTICO/MUT/1"),
    id(9),
    origin,
    u64(epoch),
    u64(base),
    u64(revision),
    u32(1),
    operations,
  ]);
  const mutationId = createHash("sha256").update(preimage).digest().subarray(0, 16);
  return Buffer.concat([
    Buffer.from("RMU1"),
    id(9),
    mutationId,
    origin,
    u64(epoch),
    u64(base),
    u64(revision),
    u32(1),
    operations,
  ]);
}

const first = selectCard(0);
if (first.length !== 1 || first[0].field !== 3) fail("first");
const second = selectCard(7);
if (second.length !== 6 || state.score !== 1 || state.phase !== 1) fail("match");
if (state.matched.length !== 2 || state.matched[0] !== 0 || state.matched[1] !== 7) fail("matched");
if (state.selected.length !== 0) fail("selected");
if (state.phase === 1 && !state.matched.includes(0)) fail("replay");

const fingerprint = rcb.schemaFingerprint(PAIRS_FIELDS);
const contract = rcb.encodeDocument(
  rcb.record([
    [1, rcb.text("goya.pairs")],
    [2, rcb.text("0.1")],
    [3, rcb.int(1)],
    [4, rcb.bytes(fingerprint)],
    [5, rcb.text("single_writer")],
    [6, rcb.array([rcb.enumeration(1), rcb.enumeration(2)])],
    [7, rcb.enumeration(1)],
    [8, rcb.enumeration(1)],
    [9, rcb.int(0)],
  ]),
);
const decoded = rcb.decodeDocument(contract);
if (decoded.t !== "record" || decoded.v.length !== 9) fail("contract");

const mutation = envelope(id(2), 2, 1, 2, second);

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("mutation", mutation);
line("contract", contract);
console.log("equivalent");
