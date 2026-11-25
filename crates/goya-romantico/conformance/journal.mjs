// Independent RJN1 journal for an idle Goya Pairs session plus the first
// select. This file does not read Rust output and does not import the other
// conformance scripts. The base is the open RSN1. The tail is one RMU1 that
// sets field 3 to the one-element list [0]. The mutation id is the first 16
// bytes of SHA-256 over "GY-ROMANTICO/MUT/1", the session, the origin, the
// epoch, the base revision, the revision, the schema version, and the
// canonical operations.

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

const session = id(9);
const origin = id(1);
const fingerprint = rcb.schemaFingerprint(PAIRS_FIELDS);
const openBody = rcb.encodeDocument(
  rcb.record([
    [1, intList([0, 1, 2, 3, 3, 2, 1, 0])],
    [2, intList([])],
    [3, intList([])],
    [4, rcb.int(0)],
    [5, rcb.int(0)],
    [6, rcb.enumeration(1)],
    [7, rcb.enumeration(1)],
    [8, rcb.int(0)],
  ]),
);
const snapshot = rcb.encodeSnapshot({
  session,
  version: 1,
  fingerprint,
  epoch: 1n,
  revision: 0n,
  authority: origin,
  body: openBody,
});

const selected = rcb.encodeBare(rcb.array([rcb.int(0)]));
const operations = Buffer.concat([u32(1), Buffer.from([1]), u32(3), selected]);
const preimage = Buffer.concat([
  Buffer.from("GY-ROMANTICO/MUT/1"),
  session,
  origin,
  u64(1),
  u64(0),
  u64(1),
  u32(1),
  operations,
]);
const mutationId = createHash("sha256").update(preimage).digest().subarray(0, 16);
const mutation = Buffer.concat([
  Buffer.from("RMU1"),
  session,
  mutationId,
  origin,
  u64(1),
  u64(0),
  u64(1),
  u32(1),
  operations,
]);

const journal = Buffer.concat([
  Buffer.from("RJN1"),
  u32(snapshot.length),
  snapshot,
  u32(1),
  u32(mutation.length),
  mutation,
]);

if (snapshot.subarray(0, 4).toString("ascii") !== "RSN1") fail("snapshot");
if (mutation.subarray(0, 4).toString("ascii") !== "RMU1") fail("mutation");
if (journal.subarray(0, 4).toString("ascii") !== "RJN1") fail("journal");
if (journal.length !== 16 + snapshot.length + mutation.length) fail("length");

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("journal", journal);
console.log("equivalent");
