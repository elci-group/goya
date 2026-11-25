// Independent ChaCha20-Poly1305 frames for the Phase 2 channel.
// Node's crypto is OpenSSL. This file does not read Rust output.
// Key: HKDF-SHA256 (RFC 5869), info "GY-ROMANTICO/AEAD/1",
// salt = lower device id || higher device id, IKM = the 16-byte pairing code.
// Nonce: 12 bytes, direction at byte 0, sequence as little-endian u64 at byte 4.
// AAD is the 46-byte RTR1 header. The tag is appended to the ciphertext.

import { createHash, createCipheriv, createDecipheriv, hkdfSync } from "node:crypto";
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

const fingerprint = createHash("sha256").update(rcb.schemaDocument(PAIRS_FIELDS)).digest();
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
const snapshotPlain = rcb.encodeSnapshot({
  session: id(9),
  version: 1,
  fingerprint,
  epoch: 1n,
  revision: 0n,
  authority: id(1),
  body: openBody,
});

const helloPlain = rcb.encodeDocument(
  rcb.record([
    [1, rcb.text("0.1")],
    [2, rcb.text("goya.pairs")],
    [3, rcb.int(1)],
    [4, rcb.bytes(fingerprint)],
    [5, rcb.text("single_writer")],
    [
      6,
      rcb.array([
        rcb.text("snapshot"),
        rcb.text("mutation"),
        rcb.text("handoff"),
        rcb.text("reconnection"),
      ]),
    ],
  ]),
);

const code = Buffer.from("pairing-code-16b");
const sender = id(1);
const receiver = id(2);
const key = Buffer.from(
  hkdfSync("sha256", code, Buffer.concat([sender, receiver]), Buffer.from("GY-ROMANTICO/AEAD/1"), 32),
);

function header(sequence, direction, kind) {
  const out = Buffer.alloc(46);
  out.write("RTR1", 0, "ascii");
  out.writeBigUInt64LE(BigInt(sequence), 4);
  out[12] = direction;
  out[13] = kind;
  sender.copy(out, 14);
  receiver.copy(out, 30);
  return out;
}

function nonce(sequence, direction) {
  const out = Buffer.alloc(12);
  out[0] = direction;
  out.writeBigUInt64LE(BigInt(sequence), 4);
  return out;
}

function encrypt(sequence, kind, plaintext) {
  const head = header(sequence, 0, kind);
  const cipher = createCipheriv("chacha20-poly1305", key, nonce(sequence, 0), {
    authTagLength: 16,
  });
  cipher.setAAD(head);
  const body = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  return Buffer.concat([head, body, cipher.getAuthTag()]);
}

function decrypt(frame) {
  if (frame.length < 62 || frame.subarray(0, 4).toString("ascii") !== "RTR1") fail("frame");
  const head = frame.subarray(0, 46);
  const body = frame.subarray(46, frame.length - 16);
  const tag = frame.subarray(frame.length - 16);
  const sequence = head.readBigUInt64LE(4);
  const direction = head[12];
  const decipher = createDecipheriv("chacha20-poly1305", key, nonce(sequence, direction), {
    authTagLength: 16,
  });
  decipher.setAAD(head);
  decipher.setAuthTag(tag);
  return Buffer.concat([decipher.update(body), decipher.final()]);
}

const hello = encrypt(1, 1, helloPlain);
const sealed = encrypt(2, 2, snapshotPlain);
const openedHello = rcb.decodeDocument(decrypt(hello));
if (openedHello.v.find(([field]) => field === 2)[1].v !== "goya.pairs") fail("hello application");
const openedSnap = rcb.decodeSnapshot(decrypt(sealed));
const board = openedSnap.value.v.find(([field]) => field === 1)[1];
if (board.v.map((item) => Number(item.v)).join(",") !== "0,1,2,3,3,2,1,0") fail("board");
if (openedSnap.revision !== 0n || openedSnap.authority[15] !== 1) fail("snapshot header");
const tampered = Buffer.from(sealed);
tampered[tampered.length - 1] ^= 0xff;
try {
  decrypt(tampered);
  fail("tampered frame authenticated");
} catch (error) {
  if (error.message === "tampered frame authenticated") throw error;
}

function line(label, buf) {
  console.log(`${label} ${buf.toString("hex")}`);
}

line("hello-plain", helloPlain);
line("hello", hello);
line("snapshot-plain", snapshotPlain);
line("snapshot", sealed);
console.log("equivalent");
