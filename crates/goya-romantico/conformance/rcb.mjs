// RCB/1 encoder and decoder.
//
// Independent of the Rust crate. Magic is "RCB1". Every integer is
// little-endian. Tags: 1 bool, 2 i64, 3 f64 bits, 4 utf-8, 5 enum u32,
// 6 array, 7 map, 8 record, 9 optional, 10 ref u64, 11 bytes, 12 timestamp
// i64 microseconds. Map keys and record ids are strictly increasing.
// Negative zero is stored as positive zero. NaN and infinities are rejected.
// A map key is a length-prefixed blob with no type tag.

import { createHash } from "node:crypto";

const MAX_DEPTH = 16;
const MAX_BYTES = 1024 * 1024;
const MAX_ITEMS = 4096;
const MAX_BLOB = 65536;
const I64_MIN = -9223372036854775808n;
const I64_MAX = 9223372036854775807n;
const U64_MAX = 18446744073709551615n;
const NEGATIVE_ZERO = 0x8000000000000000n;

export function bool(v) {
  if (typeof v !== "boolean") throw new Error("bool");
  return { t: "bool", v };
}

export function int(v) {
  const n = BigInt(v);
  if (n < I64_MIN || n > I64_MAX) throw new Error("int");
  return { t: "int", v: n };
}

export function float(v) {
  if (typeof v !== "number" || !Number.isFinite(v)) throw new Error("float");
  return { t: "float", v: v === 0 ? 0 : v };
}

export function text(v) {
  if (typeof v !== "string") throw new Error("text");
  return { t: "text", v };
}

export function enumeration(v) {
  if (!Number.isInteger(v) || v < 0 || v > 0xffffffff) throw new Error("enum");
  return { t: "enum", v };
}

export function array(items) {
  return { t: "array", v: items };
}

export function map(entries) {
  return { t: "map", v: entries };
}

export function record(fields) {
  return { t: "record", v: fields };
}

export function optional(inner) {
  return { t: "optional", v: inner };
}

export function ref(v) {
  const n = BigInt(v);
  if (n < 0n || n > U64_MAX) throw new Error("ref");
  return { t: "ref", v: n };
}

export function bytes(v) {
  return { t: "bytes", v: Buffer.from(v) };
}

export function timestamp(v) {
  const n = BigInt(v);
  if (n < I64_MIN || n > I64_MAX) throw new Error("timestamp");
  return { t: "timestamp", v: n };
}

class Cursor {
  constructor(buf) {
    this.buf = buf;
    this.i = 0;
  }

  take(n) {
    if (this.i + n > this.buf.length) throw new Error("truncated");
    const slice = this.buf.subarray(this.i, this.i + n);
    this.i += n;
    return slice;
  }

  u8() {
    return this.take(1)[0];
  }

  u32() {
    return this.take(4).readUInt32LE(0);
  }

  u64() {
    return this.take(8).readBigUInt64LE(0);
  }

  i64() {
    return this.take(8).readBigInt64LE(0);
  }
}

function pushCount(chunks, tag, count) {
  if (count > MAX_ITEMS) throw new Error("items");
  const head = Buffer.alloc(5);
  head[0] = tag;
  head.writeUInt32LE(count, 1);
  chunks.push(head);
}

function pushBlob(chunks, tag, payload) {
  if (payload.length > MAX_BLOB) throw new Error("blob");
  const parts = [];
  if (tag !== 0) parts.push(Buffer.from([tag]));
  const len = Buffer.alloc(4);
  len.writeUInt32LE(payload.length, 0);
  parts.push(len, payload);
  chunks.push(Buffer.concat(parts));
}

function encodeValue(value, depth, chunks) {
  if (depth === 0) throw new Error("depth");
  switch (value.t) {
    case "bool":
      chunks.push(Buffer.from([1, value.v ? 1 : 0]));
      return;
    case "int": {
      const raw = Buffer.alloc(9);
      raw[0] = 2;
      raw.writeBigInt64LE(value.v, 1);
      chunks.push(raw);
      return;
    }
    case "float": {
      const raw = Buffer.alloc(9);
      raw[0] = 3;
      canonicalFloat(value.v).copy(raw, 1);
      chunks.push(raw);
      return;
    }
    case "text":
      pushBlob(chunks, 4, Buffer.from(value.v, "utf8"));
      return;
    case "enum": {
      const raw = Buffer.alloc(5);
      raw[0] = 5;
      raw.writeUInt32LE(value.v, 1);
      chunks.push(raw);
      return;
    }
    case "array":
      pushCount(chunks, 6, value.v.length);
      for (const item of value.v) encodeValue(item, depth - 1, chunks);
      return;
    case "map": {
      const entries = value.v.map(([key, item]) => [Buffer.from(key, "utf8"), item]);
      entries.sort((left, right) => Buffer.compare(left[0], right[0]));
      for (let i = 1; i < entries.length; i += 1) {
        if (Buffer.compare(entries[i - 1][0], entries[i][0]) >= 0) {
          throw new Error("map key");
        }
      }
      pushCount(chunks, 7, entries.length);
      for (const [key, item] of entries) {
        pushBlob(chunks, 0, key);
        encodeValue(item, depth - 1, chunks);
      }
      return;
    }
    case "record": {
      const fields = value.v.map(([id, item]) => [id, item]);
      fields.sort((left, right) => left[0] - right[0]);
      for (let i = 1; i < fields.length; i += 1) {
        if (fields[i - 1][0] >= fields[i][0]) throw new Error("record field");
      }
      pushCount(chunks, 8, fields.length);
      for (const [id, item] of fields) {
        if (!Number.isInteger(id) || id < 0 || id > 0xffffffff) throw new Error("field id");
        const raw = Buffer.alloc(4);
        raw.writeUInt32LE(id, 0);
        chunks.push(raw);
        encodeValue(item, depth - 1, chunks);
      }
      return;
    }
    case "optional":
      if (value.v == null) {
        chunks.push(Buffer.from([9, 0]));
        return;
      }
      chunks.push(Buffer.from([9, 1]));
      encodeValue(value.v, depth - 1, chunks);
      return;
    case "ref": {
      const raw = Buffer.alloc(9);
      raw[0] = 10;
      raw.writeBigUInt64LE(value.v, 1);
      chunks.push(raw);
      return;
    }
    case "bytes":
      pushBlob(chunks, 11, value.v);
      return;
    case "timestamp": {
      const raw = Buffer.alloc(9);
      raw[0] = 12;
      raw.writeBigInt64LE(value.v, 1);
      chunks.push(raw);
      return;
    }
    default:
      throw new Error("value");
  }
}

function canonicalFloat(value) {
  const raw = Buffer.alloc(8);
  raw.writeDoubleLE(value, 0);
  if (raw.readBigUInt64LE(0) === NEGATIVE_ZERO) raw.fill(0);
  return raw;
}

function finish(chunks) {
  const out = Buffer.concat(chunks);
  if (out.length > MAX_BYTES) throw new Error("bytes");
  return out;
}

export function encodeDocument(value) {
  const chunks = [Buffer.from("RCB1")];
  encodeValue(value, MAX_DEPTH, chunks);
  return finish(chunks);
}

export function encodeBare(value) {
  const chunks = [];
  encodeValue(value, MAX_DEPTH, chunks);
  return finish(chunks);
}

function utf8Strict(payload) {
  const text = payload.toString("utf8");
  if (!Buffer.from(text, "utf8").equals(payload)) throw new Error("utf8");
  return text;
}

function decodeValue(cur, depth) {
  if (depth === 0) throw new Error("depth");
  switch (cur.u8()) {
    case 1: {
      const flag = cur.u8();
      if (flag > 1) throw new Error("bool");
      return bool(flag === 1);
    }
    case 2:
      return int(cur.i64());
    case 3: {
      const bits = cur.u64();
      if (bits === NEGATIVE_ZERO) throw new Error("negative zero");
      const raw = Buffer.alloc(8);
      raw.writeBigUInt64LE(bits, 0);
      const value = raw.readDoubleLE(0);
      if (!Number.isFinite(value)) throw new Error("float");
      return float(value);
    }
    case 4:
      return text(utf8Strict(readBlob(cur)));
    case 5:
      return enumeration(cur.u32());
    case 6: {
      const count = readCount(cur);
      const items = [];
      for (let i = 0; i < count; i += 1) items.push(decodeValue(cur, depth - 1));
      return array(items);
    }
    case 7: {
      const count = readCount(cur);
      const entries = [];
      let previous = null;
      for (let i = 0; i < count; i += 1) {
        const keyBytes = readBlob(cur);
        if (previous && Buffer.compare(keyBytes, previous) <= 0) throw new Error("map key");
        previous = keyBytes;
        entries.push([utf8Strict(keyBytes), decodeValue(cur, depth - 1)]);
      }
      return map(entries);
    }
    case 8: {
      const count = readCount(cur);
      const fields = [];
      let previous = null;
      for (let i = 0; i < count; i += 1) {
        const id = cur.u32();
        if (previous !== null && id <= previous) throw new Error("record field");
        previous = id;
        fields.push([id, decodeValue(cur, depth - 1)]);
      }
      return record(fields);
    }
    case 9: {
      const flag = cur.u8();
      if (flag === 0) return optional(null);
      if (flag === 1) return optional(decodeValue(cur, depth - 1));
      throw new Error("optional");
    }
    case 10:
      return ref(cur.u64());
    case 11:
      return bytes(readBlob(cur));
    case 12:
      return timestamp(cur.i64());
    default:
      throw new Error("tag");
  }
}

function readCount(cur) {
  const count = cur.u32();
  if (count > MAX_ITEMS) throw new Error("items");
  return count;
}

function readBlob(cur) {
  const len = cur.u32();
  if (len > MAX_BLOB) throw new Error("blob");
  return Buffer.from(cur.take(len));
}

export function decodeDocument(buf) {
  const cur = new Cursor(buf);
  const magic = cur.take(4).toString("ascii");
  if (magic !== "RCB1") throw new Error("magic");
  const value = decodeValue(cur, MAX_DEPTH);
  if (cur.i !== buf.length) throw new Error("trailing");
  return value;
}

// Schema identity is an array of records {1: id, 2: class, 3: wire type}.
// The source name is not an input.
export function schemaDocument(fields) {
  return encodeDocument(
    array(
      fields.map(([id, cls, wire]) =>
        record([
          [1, int(id)],
          [2, enumeration(cls)],
          [3, enumeration(wire)],
        ]),
      ),
    ),
  );
}

export function schemaFingerprint(fields) {
  return createHash("sha256").update(schemaDocument(fields)).digest();
}

export function encodeSnapshot({ session, version, fingerprint, epoch, revision, authority, body }) {
  if (session.length !== 16 || authority.length !== 16 || fingerprint.length !== 32) {
    throw new Error("snapshot header");
  }
  const head = Buffer.alloc(92);
  head.write("RSN1", 0, "ascii");
  session.copy(head, 4);
  head.writeUInt32LE(version >>> 0, 20);
  fingerprint.copy(head, 24);
  head.writeBigUInt64LE(BigInt(epoch), 56);
  head.writeBigUInt64LE(BigInt(revision), 64);
  authority.copy(head, 72);
  head.writeUInt32LE(body.length, 88);
  return finish([head, body]);
}

export function decodeSnapshot(buf) {
  if (buf.length < 92 || buf.subarray(0, 4).toString("ascii") !== "RSN1") {
    throw new Error("snapshot");
  }
  const len = buf.readUInt32LE(88);
  if (92 + len !== buf.length) throw new Error("snapshot length");
  const body = Buffer.from(buf.subarray(92));
  return {
    session: Buffer.from(buf.subarray(4, 20)),
    version: buf.readUInt32LE(20),
    fingerprint: Buffer.from(buf.subarray(24, 56)),
    epoch: buf.readBigUInt64LE(56),
    revision: buf.readBigUInt64LE(64),
    authority: Buffer.from(buf.subarray(72, 88)),
    body,
    value: decodeDocument(body),
  };
}
