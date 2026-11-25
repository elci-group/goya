// Independent RMU1 envelope and RTR1 mutation frame.
// Node's crypto is OpenSSL. This file does not read Rust output.
// The mutation id is the first 16 bytes of SHA-256 over
// "GY-ROMANTICO/MUT/1", the session, the origin, the epoch, the base
// revision, the revision, the schema version, and the canonical operations.
// The first Goya Pairs select sets field 3 to the one-element list [0].
// The frame is TV to phone, sequence 2, kind 4, after one hello each way
// and no snapshot in between.

import { createCipheriv, createDecipheriv, createHash, hkdfSync } from "node:crypto";
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

const session = id(9);
const origin = id(1);
const phone = id(2);
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

const intent = rcb.encodeDocument(
  rcb.record([
    [1, rcb.enumeration(1)],
    [2, rcb.int(0)],
    [3, rcb.bytes(phone)],
  ]),
);
const decodedIntent = rcb.decodeDocument(intent);
const intentFields = new Map(decodedIntent.v.map(([field, value]) => [field, value]));
if (intentFields.get(1).v !== 1 || intentFields.get(2).v !== 0n || intentFields.get(3).v[15] !== 2) {
  fail("intent");
}

const code = Buffer.from("pairing-code-16b");
const key = Buffer.from(
  hkdfSync("sha256", code, Buffer.concat([origin, phone]), Buffer.from("GY-ROMANTICO/AEAD/1"), 32),
);

function header(sequence, kind) {
  const out = Buffer.alloc(46);
  out.write("RTR1", 0, "ascii");
  out.writeBigUInt64LE(BigInt(sequence), 4);
  out[12] = 0;
  out[13] = kind;
  origin.copy(out, 14);
  phone.copy(out, 30);
  return out;
}

function nonce(sequence) {
  const out = Buffer.alloc(12);
  out[0] = 0;
  out.writeBigUInt64LE(BigInt(sequence), 4);
  return out;
}

function encrypt(sequence, kind, plaintext) {
  const head = header(sequence, kind);
  const cipher = createCipheriv("chacha20-poly1305", key, nonce(sequence), {
    authTagLength: 16,
  });
  cipher.setAAD(head);
  const body = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  return Buffer.concat([head, body, cipher.getAuthTag()]);
}

function decrypt(frame) {
  const head = frame.subarray(0, 46);
  const body = frame.subarray(46, frame.length - 16);
  const tag = frame.subarray(frame.length - 16);
  const decipher = createDecipheriv("chacha20-poly1305", key, nonce(head.readBigUInt64LE(4)), {
    authTagLength: 16,
  });
  decipher.setAAD(head);
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(body), decipher.final()]);
}

const frame = encrypt(2, 4, mutation);
if (!decrypt(frame).equals(mutation)) fail("mutation frame");
if (frame[13] !== 4 || frame.readBigUInt64LE(4) !== 2n) fail("mutation header");

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("mutation", mutation);
line("mutation-frame", frame);
line("intent", intent);
console.log("equivalent");
