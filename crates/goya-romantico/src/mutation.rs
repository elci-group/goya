//! Authoritative mutation envelopes.
//!
//! A mutation identity is the first 16 bytes of SHA-256 over a domain
//! separator and the canonical operation bytes. The same body always yields
//! the same identity. Applying the operations is all-or-nothing.

use crate::canonical::{encode_value, fingerprint_bytes, Sink, Value, MAX_DEPTH, MAX_ITEMS};
use crate::schema::Schema;
use crate::Error;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Set { field: u32, value: Value },
    Append { field: u32, value: Value },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    pub session: [u8; 16],
    pub mutation: [u8; 16],
    pub origin: [u8; 16],
    pub epoch: u64,
    pub base_revision: u64,
    pub revision: u64,
    pub schema_version: u32,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Apply {
    Applied,
    Duplicate,
    Stale,
}

struct Identity<'a> {
    session: &'a [u8; 16],
    origin: &'a [u8; 16],
    epoch: u64,
    base_revision: u64,
    revision: u64,
    schema_version: u32,
    operations: &'a [Operation],
}

pub fn encode_envelope(envelope: &Envelope) -> Result<Vec<u8>, Error> {
    let id = derive_id(&Identity {
        session: &envelope.session,
        origin: &envelope.origin,
        epoch: envelope.epoch,
        base_revision: envelope.base_revision,
        revision: envelope.revision,
        schema_version: envelope.schema_version,
        operations: &envelope.operations,
    })?;
    if id != envelope.mutation {
        return Err(Error::new(
            "RMT-E2001",
            "mutation identity does not match its body",
        ));
    }
    if envelope.base_revision.checked_add(1) != Some(envelope.revision) {
        return Err(Error::new(
            "RMT-E2001",
            "mutation revision is not the successor of its base",
        ));
    }
    let mut out = Vec::new();
    let mut sink = Sink::new(&mut out);
    sink.push(b"RMU1")?;
    sink.push(&envelope.session)?;
    sink.push(&envelope.mutation)?;
    sink.push(&envelope.origin)?;
    sink.push(&envelope.epoch.to_le_bytes())?;
    sink.push(&envelope.base_revision.to_le_bytes())?;
    sink.push(&envelope.revision.to_le_bytes())?;
    sink.push(&envelope.schema_version.to_le_bytes())?;
    sink.push(&encode_operations(&envelope.operations)?)?;
    Ok(out)
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope, Error> {
    if bytes.len() < 4 + 16 + 16 + 16 + 8 + 8 + 8 + 4 + 4 {
        return Err(Error::new("RMT-E2001", "truncated mutation envelope"));
    }
    if &bytes[..4] != b"RMU1" {
        return Err(Error::new("RMT-E2001", "mutation magic is not RMU1"));
    }
    let mut offset = 4;
    let session = read_id(bytes, &mut offset)?;
    let mutation = read_id(bytes, &mut offset)?;
    let origin = read_id(bytes, &mut offset)?;
    let epoch = read_u64(bytes, &mut offset)?;
    let base_revision = read_u64(bytes, &mut offset)?;
    let revision = read_u64(bytes, &mut offset)?;
    let schema_version = read_u32(bytes, &mut offset)?;
    let operations = decode_operations(&bytes[offset..])?;
    let envelope = Envelope {
        session,
        mutation,
        origin,
        epoch,
        base_revision,
        revision,
        schema_version,
        operations,
    };
    let encoded = encode_envelope(&envelope)?;
    if encoded.as_slice() != bytes {
        return Err(Error::new(
            "RMT-E2001",
            "mutation encoding is not canonical",
        ));
    }
    Ok(envelope)
}

pub(crate) fn seal(
    session: [u8; 16],
    origin: [u8; 16],
    epoch: u64,
    base_revision: u64,
    schema_version: u32,
    operations: Vec<Operation>,
) -> Result<Envelope, Error> {
    let revision = base_revision
        .checked_add(1)
        .ok_or(Error::new("RMT-E8001", "revision counter would overflow"))?;
    let mutation = derive_id(&Identity {
        session: &session,
        origin: &origin,
        epoch,
        base_revision,
        revision,
        schema_version,
        operations: &operations,
    })?;
    Ok(Envelope {
        session,
        mutation,
        origin,
        epoch,
        base_revision,
        revision,
        schema_version,
        operations,
    })
}

pub(crate) fn apply_operations(
    schema: Schema,
    values: &BTreeMap<u32, Value>,
    operations: &[Operation],
) -> Result<BTreeMap<u32, Value>, Error> {
    if operations.is_empty() {
        return Err(Error::new("RMT-E5004", "transaction has no operations"));
    }
    let mut next = values.clone();
    for operation in operations {
        let (field, value, append) = match operation {
            Operation::Set { field, value } => (*field, value, false),
            Operation::Append { field, value } => (*field, value, true),
        };
        let definition = schema.get(field).ok_or(Error::new(
            "RMT-E2002",
            "mutation names a field outside the schema",
        ))?;
        if !definition.class.is_portable() {
            return Err(Error::new(
                "RMT-E2003",
                "mutation targets state that is not replicated",
            ));
        }
        if append {
            let Some(Value::Array(items)) = next.get(&field).cloned() else {
                return Err(Error::new("RMT-E2001", "append target is not an array"));
            };
            if items.len() >= MAX_ITEMS {
                return Err(Error::new(
                    "RMT-E8001",
                    "append would exceed the item budget",
                ));
            }
            let mut items = items;
            items.push(value.clone());
            next.insert(field, Value::Array(items));
        } else {
            next.insert(field, value.clone());
        }
    }
    Ok(next)
}

fn derive_id(parts: &Identity<'_>) -> Result<[u8; 16], Error> {
    let mut body = Vec::new();
    let mut sink = Sink::new(&mut body);
    sink.push(b"GY-ROMANTICO/MUT/1")?;
    sink.push(parts.session)?;
    sink.push(parts.origin)?;
    sink.push(&parts.epoch.to_le_bytes())?;
    sink.push(&parts.base_revision.to_le_bytes())?;
    sink.push(&parts.revision.to_le_bytes())?;
    sink.push(&parts.schema_version.to_le_bytes())?;
    sink.push(&encode_operations(parts.operations)?)?;
    let digest = fingerprint_bytes(&body);
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    Ok(id)
}

fn encode_operations(operations: &[Operation]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut sink = Sink::new(&mut out);
    let count = u32::try_from(operations.len())
        .map_err(|_| Error::new("RMT-E8001", "transaction exceeds the operation budget"))?;
    sink.push(&count.to_le_bytes())?;
    for operation in operations {
        match operation {
            Operation::Set { field, value } => {
                sink.push(&[1])?;
                sink.push(&field.to_le_bytes())?;
                encode_value(&mut sink, value, MAX_DEPTH)?;
            }
            Operation::Append { field, value } => {
                sink.push(&[2])?;
                sink.push(&field.to_le_bytes())?;
                encode_value(&mut sink, value, MAX_DEPTH)?;
            }
        }
    }
    Ok(out)
}

fn decode_operations(mut bytes: &[u8]) -> Result<Vec<Operation>, Error> {
    if bytes.len() < 4 {
        return Err(Error::new("RMT-E2001", "truncated mutation operations"));
    }
    let mut offset = 0;
    let count = read_u32(bytes, &mut offset)?;
    bytes = &bytes[offset..];
    if count > MAX_ITEMS as u32 {
        return Err(Error::new(
            "RMT-E8001",
            "transaction exceeds the operation budget",
        ));
    }
    let mut operations = Vec::with_capacity(count as usize);
    let mut cursor = crate::canonical::Raw::new(bytes);
    for _ in 0..count {
        let tag = cursor.u8()?;
        let field = cursor.u32()?;
        let value = cursor.value()?;
        let operation = match tag {
            1 => Operation::Set { field, value },
            2 => Operation::Append { field, value },
            _ => return Err(Error::new("RMT-E2001", "unknown mutation operation")),
        };
        operations.push(operation);
    }
    if !cursor.exhausted() {
        return Err(Error::new(
            "RMT-E2001",
            "trailing bytes after mutation operations",
        ));
    }
    Ok(operations)
}

fn read_id(bytes: &[u8], offset: &mut usize) -> Result<[u8; 16], Error> {
    let end = offset
        .checked_add(16)
        .ok_or(Error::new("RMT-E2001", "truncated mutation envelope"))?;
    if end > bytes.len() {
        return Err(Error::new("RMT-E2001", "truncated mutation envelope"));
    }
    let mut id = [0u8; 16];
    id.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(id)
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, Error> {
    let end = offset
        .checked_add(4)
        .ok_or(Error::new("RMT-E2001", "truncated mutation envelope"))?;
    if end > bytes.len() {
        return Err(Error::new("RMT-E2001", "truncated mutation envelope"));
    }
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(u32::from_le_bytes(raw))
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, Error> {
    let end = offset
        .checked_add(8)
        .ok_or(Error::new("RMT-E2001", "truncated mutation envelope"))?;
    if end > bytes.len() {
        return Err(Error::new("RMT-E2001", "truncated mutation envelope"));
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(u64::from_le_bytes(raw))
}
