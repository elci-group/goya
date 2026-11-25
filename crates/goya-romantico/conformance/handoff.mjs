// Independent prepare and grant records for an idle Goya Pairs session.
// Node's crypto is OpenSSL. This file does not read Rust output.
// Prepare names epoch 1 and revision 0. Grant names epoch 2, the same
// revision, and the SHA-256 of the open portable body. The grant frame is
// television to phone, sequence 3, kind 7, after one hello and one prepare.

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

function intList(values) {
  return rcb.array(values.map((value) => rcb.int(value)));
}

const session = id(9);
const from = id(1);
const to = id(2);
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
const fingerprint = createHash("sha256").update(openBody).digest();

function parties(epoch, revision, fingerprintBytes) {
  const fields = [
    [1, rcb.bytes(session)],
    [2, rcb.bytes(from)],
    [3, rcb.bytes(to)],
  ];
  if (epoch != null) {
    fields.push([4, rcb.int(epoch)], [5, rcb.int(revision)]);
  }
  if (fingerprintBytes) fields.push([6, rcb.bytes(fingerprintBytes)]);
  return rcb.encodeDocument(rcb.record(fields));
}

const prepare = parties(1, 0, null);
const grant = parties(2, 0, fingerprint);
const decoded = rcb.decodeDocument(grant);
const fields = new Map(decoded.v.map(([field, value]) => [field, value]));
if (fields.get(4).v !== 2n || fields.get(5).v !== 0n || !fields.get(6).v.equals(fingerprint)) {
  fail("grant");
}

const code = Buffer.from("pairing-code-16b");
const key = Buffer.from(
  hkdfSync("sha256", code, Buffer.concat([from, to]), Buffer.from("GY-ROMANTICO/AEAD/1"), 32),
);

function header(sequence, kind) {
  const out = Buffer.alloc(46);
  out.write("RTR1", 0, "ascii");
  out.writeBigUInt64LE(BigInt(sequence), 4);
  out[12] = 0;
  out[13] = kind;
  from.copy(out, 14);
  to.copy(out, 30);
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

encrypt(2, 6, prepare);
const frame = encrypt(3, 7, grant);
if (!decrypt(frame).equals(grant)) fail("grant frame");
if (frame[13] !== 7 || frame.readBigUInt64LE(4) !== 3n) fail("grant header");

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("prepare", prepare);
line("grant", grant);
line("grant-frame", frame);
console.log("equivalent");
