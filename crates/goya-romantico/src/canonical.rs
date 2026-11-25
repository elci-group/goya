//! RCB/1 — romántico canonical bytes.
//!
//! Equivalent values encode to equivalent bytes. Maps and records are sorted.
//! Negative zero becomes positive zero. NaN and infinities are rejected.
//! Decoders reject truncated input, trailing bytes, unsorted keys, and values
//! nested deeper than [`MAX_DEPTH`].

use crate::Error;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAX_DEPTH: usize = 16;
pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_ITEMS: usize = 4096;
pub const MAX_BLOB: usize = 65_536;

const TAG_BOOL: u8 = 1;
const TAG_INT: u8 = 2;
const TAG_FLOAT: u8 = 3;
const TAG_TEXT: u8 = 4;
const TAG_ENUM: u8 = 5;
const TAG_ARRAY: u8 = 6;
const TAG_MAP: u8 = 7;
const TAG_RECORD: u8 = 8;
const TAG_OPTIONAL: u8 = 9;
const TAG_REF: u8 = 10;
const TAG_BYTES: u8 = 11;
const TAG_TIMESTAMP: u8 = 12;

/// Finite floating-point value with a single canonical bit pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderedF64 {
    bits: u64,
}

impl OrderedF64 {
    pub fn try_new(value: f64) -> Result<Self, Error> {
        if !value.is_finite() {
            return Err(Error::new(
                "RMT-E2001",
                "non-finite floating point is rejected",
            ));
        }
        let mut bits = value.to_bits();
        if bits == (-0.0f64).to_bits() {
            bits = 0;
        }
        Ok(Self { bits })
    }

    pub fn to_f64(self) -> f64 {
        f64::from_bits(self.bits)
    }
}

/// Portable semantic value. Field identity lives in the surrounding record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(OrderedF64),
    Text(String),
    Enum(u32),
    Array(Vec<Value>),
    Map(BTreeMap<String, Value>),
    Record(BTreeMap<u32, Value>),
    Optional(Option<Box<Value>>),
    Ref(u64),
    Bytes(Vec<u8>),
    /// Microseconds since the Unix epoch. This crate never reads a clock.
    TimestampUs(i64),
}

pub(crate) struct Sink<'a> {
    out: &'a mut Vec<u8>,
    left: usize,
}

impl<'a> Sink<'a> {
    pub(crate) fn new(out: &'a mut Vec<u8>) -> Self {
        Self {
            out,
            left: MAX_BYTES,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > self.left {
            return Err(Error::new(
                "RMT-E8001",
                "canonical encoding exceeded the byte budget",
            ));
        }
        self.left -= bytes.len();
        self.out.extend_from_slice(bytes);
        Ok(())
    }
}

struct Cur<'a> {
    buf: &'a [u8],
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.buf.len() < n {
            return Err(Error::new("RMT-E2001", "truncated canonical value"));
        }
        let (head, tail) = self.buf.split_at(n);
        self.buf = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, Error> {
        let bytes = self.take(4)?;
        let mut raw = [0u8; 4];
        raw.copy_from_slice(bytes);
        Ok(u32::from_le_bytes(raw))
    }

    fn u64(&mut self) -> Result<u64, Error> {
        let bytes = self.take(8)?;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(raw))
    }
}

pub fn fingerprint_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn encode(value: &Value) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut sink = Sink::new(&mut out);
    sink.push(b"RCB1")?;
    encode_value(&mut sink, value, MAX_DEPTH)?;
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<Value, Error> {
    let mut cur = Cur { buf: bytes };
    let magic = cur.take(4)?;
    if magic != b"RCB1" {
        return Err(Error::new(
            "RMT-E2001",
            "canonical document magic is not RCB1",
        ));
    }
    let value = decode_value(&mut cur, MAX_DEPTH)?;
    if !cur.buf.is_empty() {
        return Err(Error::new(
            "RMT-E2001",
            "trailing bytes after canonical value",
        ));
    }
    Ok(value)
}

pub(crate) fn encode_value(sink: &mut Sink<'_>, value: &Value, depth: usize) -> Result<(), Error> {
    if depth == 0 {
        return Err(Error::new(
            "RMT-E2001",
            "canonical value is nested too deeply",
        ));
    }
    match value {
        Value::Bool(flag) => {
            sink.push(&[TAG_BOOL, u8::from(*flag)])?;
        }
        Value::Int(number) => {
            sink.push(&[TAG_INT])?;
            sink.push(&number.to_le_bytes())?;
        }
        Value::Float(number) => {
            sink.push(&[TAG_FLOAT])?;
            sink.push(&number.bits.to_le_bytes())?;
        }
        Value::Text(text) => write_blob(sink, TAG_TEXT, text.as_bytes())?,
        Value::Enum(discriminant) => {
            sink.push(&[TAG_ENUM])?;
            sink.push(&discriminant.to_le_bytes())?;
        }
        Value::Array(items) => {
            write_count(sink, TAG_ARRAY, items.len())?;
            for item in items {
                encode_value(sink, item, depth - 1)?;
            }
        }
        Value::Map(map) => {
            write_count(sink, TAG_MAP, map.len())?;
            for (key, item) in map {
                write_blob(sink, 0, key.as_bytes())?;
                encode_value(sink, item, depth - 1)?;
            }
        }
        Value::Record(fields) => {
            write_count(sink, TAG_RECORD, fields.len())?;
            for (id, item) in fields {
                sink.push(&id.to_le_bytes())?;
                encode_value(sink, item, depth - 1)?;
            }
        }
        Value::Optional(None) => sink.push(&[TAG_OPTIONAL, 0])?,
        Value::Optional(Some(inner)) => {
            sink.push(&[TAG_OPTIONAL, 1])?;
            encode_value(sink, inner, depth - 1)?;
        }
        Value::Ref(id) => {
            sink.push(&[TAG_REF])?;
            sink.push(&id.to_le_bytes())?;
        }
        Value::Bytes(bytes) => write_blob(sink, TAG_BYTES, bytes)?,
        Value::TimestampUs(micros) => {
            sink.push(&[TAG_TIMESTAMP])?;
            sink.push(&micros.to_le_bytes())?;
        }
    }
    Ok(())
}

fn write_count(sink: &mut Sink<'_>, tag: u8, len: usize) -> Result<(), Error> {
    if len > MAX_ITEMS {
        return Err(Error::new(
            "RMT-E8001",
            "canonical collection exceeds its item budget",
        ));
    }
    let count = u32::try_from(len)
        .map_err(|_| Error::new("RMT-E8001", "canonical collection exceeds its item budget"))?;
    sink.push(&[tag])?;
    sink.push(&count.to_le_bytes())
}

fn write_blob(sink: &mut Sink<'_>, tag: u8, bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_BLOB {
        return Err(Error::new(
            "RMT-E8001",
            "canonical blob exceeds its byte budget",
        ));
    }
    let len = u32::try_from(bytes.len())
        .map_err(|_| Error::new("RMT-E8001", "canonical blob exceeds its byte budget"))?;
    if tag != 0 {
        sink.push(&[tag])?;
    }
    sink.push(&len.to_le_bytes())?;
    sink.push(bytes)
}

fn decode_value(cur: &mut Cur<'_>, depth: usize) -> Result<Value, Error> {
    if depth == 0 {
        return Err(Error::new(
            "RMT-E2001",
            "canonical value is nested too deeply",
        ));
    }
    match cur.u8()? {
        TAG_BOOL => match cur.u8()? {
            0 => Ok(Value::Bool(false)),
            1 => Ok(Value::Bool(true)),
            _ => Err(Error::new("RMT-E2001", "boolean byte is not 0 or 1")),
        },
        TAG_INT => Ok(Value::Int(cur.u64()? as i64)),
        TAG_FLOAT => {
            let bits = cur.u64()?;
            if bits == (-0.0f64).to_bits() {
                return Err(Error::new("RMT-E2001", "negative zero is not canonical"));
            }
            let number = f64::from_bits(bits);
            if !number.is_finite() {
                return Err(Error::new(
                    "RMT-E2001",
                    "non-finite floating point is rejected",
                ));
            }
            Ok(Value::Float(OrderedF64 { bits }))
        }
        TAG_TEXT => Ok(Value::Text(decode_text(cur)?)),
        TAG_ENUM => Ok(Value::Enum(cur.u32()?)),
        TAG_ARRAY => {
            let count = read_count(cur)?;
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(decode_value(cur, depth - 1)?);
            }
            Ok(Value::Array(items))
        }
        TAG_MAP => {
            let count = read_count(cur)?;
            let mut map = BTreeMap::new();
            let mut previous: Option<Vec<u8>> = None;
            for _ in 0..count {
                let key_bytes = read_blob(cur)?;
                if previous
                    .as_ref()
                    .is_some_and(|prior| key_bytes.as_slice() <= prior.as_slice())
                {
                    return Err(Error::new(
                        "RMT-E2001",
                        "canonical map keys are not strictly increasing",
                    ));
                }
                let key = String::from_utf8(key_bytes.clone())
                    .map_err(|_| Error::new("RMT-E2001", "canonical map key is not utf-8"))?;
                previous = Some(key_bytes);
                let value = decode_value(cur, depth - 1)?;
                map.insert(key, value);
            }
            Ok(Value::Map(map))
        }
        TAG_RECORD => {
            let count = read_count(cur)?;
            let mut fields = BTreeMap::new();
            let mut previous: Option<u32> = None;
            for _ in 0..count {
                let id = cur.u32()?;
                if previous.is_some_and(|prior| id <= prior) {
                    return Err(Error::new(
                        "RMT-E2001",
                        "canonical record fields are not strictly increasing",
                    ));
                }
                previous = Some(id);
                fields.insert(id, decode_value(cur, depth - 1)?);
            }
            Ok(Value::Record(fields))
        }
        TAG_OPTIONAL => match cur.u8()? {
            0 => Ok(Value::Optional(None)),
            1 => Ok(Value::Optional(Some(Box::new(decode_value(
                cur,
                depth - 1,
            )?)))),
            _ => Err(Error::new(
                "RMT-E2001",
                "optional presence byte is not 0 or 1",
            )),
        },
        TAG_REF => Ok(Value::Ref(cur.u64()?)),
        TAG_BYTES => Ok(Value::Bytes(read_blob(cur)?)),
        TAG_TIMESTAMP => Ok(Value::TimestampUs(cur.u64()? as i64)),
        _ => Err(Error::new("RMT-E2001", "unknown canonical type tag")),
    }
}

fn read_count(cur: &mut Cur<'_>) -> Result<usize, Error> {
    let count = cur.u32()? as usize;
    if count > MAX_ITEMS {
        return Err(Error::new(
            "RMT-E8001",
            "canonical collection exceeds its item budget",
        ));
    }
    Ok(count)
}

fn read_blob(cur: &mut Cur<'_>) -> Result<Vec<u8>, Error> {
    let len = cur.u32()? as usize;
    if len > MAX_BLOB {
        return Err(Error::new(
            "RMT-E8001",
            "canonical blob exceeds its byte budget",
        ));
    }
    Ok(cur.take(len)?.to_vec())
}

fn decode_text(cur: &mut Cur<'_>) -> Result<String, Error> {
    let bytes = read_blob(cur)?;
    String::from_utf8(bytes).map_err(|_| Error::new("RMT-E2001", "canonical text is not utf-8"))
}

/// Cursor over an already-framed byte string. Used by the mutation codec.
pub(crate) struct Raw<'a> {
    buf: &'a [u8],
}

impl<'a> Raw<'a> {
    pub(crate) fn new(buf: &'a [u8]) -> Self {
        Self { buf }
    }

    pub(crate) fn u8(&mut self) -> Result<u8, Error> {
        let mut cur = Cur { buf: self.buf };
        let value = cur.u8()?;
        self.buf = cur.buf;
        Ok(value)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, Error> {
        let mut cur = Cur { buf: self.buf };
        let value = cur.u32()?;
        self.buf = cur.buf;
        Ok(value)
    }

    pub(crate) fn value(&mut self) -> Result<Value, Error> {
        let mut cur = Cur { buf: self.buf };
        let value = decode_value(&mut cur, MAX_DEPTH)?;
        self.buf = cur.buf;
        Ok(value)
    }

    pub(crate) fn exhausted(&self) -> bool {
        self.buf.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_true_has_stable_bytes() {
        assert_eq!(encode(&Value::Bool(true)).unwrap(), b"RCB1\x01\x01");
    }

    #[test]
    fn integers_round_trip_including_min() {
        let encoded = encode(&Value::Int(i64::MIN)).unwrap();
        assert_eq!(decode(&encoded).unwrap(), Value::Int(i64::MIN));
    }

    #[test]
    fn negative_zero_matches_positive_zero() {
        let positive = OrderedF64::try_new(0.0).unwrap();
        let negative = OrderedF64::try_new(-0.0).unwrap();
        assert_eq!(positive, negative);
        assert_eq!(
            encode(&Value::Float(positive)).unwrap(),
            encode(&Value::Float(negative)).unwrap()
        );
    }

    #[test]
    fn non_finite_floats_are_rejected() {
        assert!(OrderedF64::try_new(f64::NAN).is_err());
        assert!(OrderedF64::try_new(f64::INFINITY).is_err());
        assert!(OrderedF64::try_new(f64::NEG_INFINITY).is_err());
    }

    #[test]
    fn map_and_record_order_does_not_change_bytes() {
        let mut first = BTreeMap::new();
        first.insert(3, Value::Int(1));
        first.insert(1, Value::Bool(false));
        let mut second = BTreeMap::new();
        second.insert(1, Value::Bool(false));
        second.insert(3, Value::Int(1));
        assert_eq!(
            encode(&Value::Record(first)).unwrap(),
            encode(&Value::Record(second)).unwrap()
        );
    }

    #[test]
    fn unsorted_map_is_rejected() {
        let bytes = [
            b'R', b'C', b'B', b'1', 7, 2, 0, 0, 0, 1, 0, 0, 0, b'b', 1, 1, 1, 0, 0, 0, b'a', 1, 0,
        ];
        assert_eq!(decode(&bytes).unwrap_err().code, "RMT-E2001");
    }

    #[test]
    fn trailing_byte_is_rejected() {
        let mut bytes = encode(&Value::Bool(false)).unwrap();
        bytes.push(0);
        assert_eq!(decode(&bytes).unwrap_err().code, "RMT-E2001");
    }

    #[test]
    fn depth_limit_is_enforced() {
        let mut value = Value::Bool(true);
        for _ in 0..MAX_DEPTH {
            value = Value::Optional(Some(Box::new(value)));
        }
        assert_eq!(encode(&value).unwrap_err().code, "RMT-E2001");
    }
}
