//! Continuity contract shared by compiled and interpreted hosts.
//!
//! Execution mode is a host label. It is not an input to this document, the
//! schema fingerprint, HELLO, or a mutation id. The record carries no target
//! name and no `.gscene` syntax.

use crate::canonical::{decode, encode, Value};
use crate::schema::Schema;
use crate::{Error, PROTOCOL};
use std::collections::BTreeMap;

/// How this process runs the application. Not part of session identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    Compiled,
    Interpreted,
}

/// The caller holds the journal. The core does not open a file.
pub const PERSISTENCE_CALLER_HELD: u32 = 1;
/// A gap is rejected. Last-write-wins is not a policy.
pub const RECONCILE_REJECT_GAP: u32 = 1;

/// Closed continuity contract. Nine fields. No execution mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContinuityContract {
    pub application_id: String,
    pub protocol: String,
    pub schema_version: u32,
    pub schema_fingerprint: [u8; 32],
    pub authority_model: String,
    pub actions: Vec<u32>,
    pub persistence: u32,
    pub reconciliation: u32,
    pub migrations: u32,
}

pub fn encode_contract(schema: Schema, actions: &[u32]) -> Result<Vec<u8>, Error> {
    schema.check()?;
    let mut previous: Option<u32> = None;
    for id in actions {
        if previous.is_some_and(|prior| *id <= prior) {
            return Err(Error::new(
                "RMT-E1001",
                "action identifiers must be strictly increasing",
            ));
        }
        previous = Some(*id);
    }
    let mut record = BTreeMap::new();
    record.insert(1, Value::Text(schema.application_id.to_string()));
    record.insert(2, Value::Text(PROTOCOL.to_string()));
    record.insert(3, Value::Int(i64::from(schema.version)));
    record.insert(4, Value::Bytes(schema.fingerprint()?.to_vec()));
    record.insert(5, Value::Text("single_writer".to_string()));
    record.insert(
        6,
        Value::Array(actions.iter().copied().map(Value::Enum).collect()),
    );
    record.insert(7, Value::Enum(PERSISTENCE_CALLER_HELD));
    record.insert(8, Value::Enum(RECONCILE_REJECT_GAP));
    record.insert(9, Value::Int(0));
    encode(&Value::Record(record))
}

pub fn decode_contract(bytes: &[u8]) -> Result<ContinuityContract, Error> {
    let value = decode(bytes)?;
    if encode(&value)? != bytes {
        return Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        ));
    }
    let Value::Record(fields) = value else {
        return Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        ));
    };
    if fields.len() != 9 {
        return Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        ));
    }
    let contract = ContinuityContract {
        application_id: text_field(&fields, 1)?,
        protocol: text_field(&fields, 2)?,
        schema_version: u32_field(&fields, 3)?,
        schema_fingerprint: bytes32(&fields, 4)?,
        authority_model: text_field(&fields, 5)?,
        actions: action_list(&fields, 6)?,
        persistence: enum_field(&fields, 7)?,
        reconciliation: enum_field(&fields, 8)?,
        migrations: u32_field(&fields, 9)?,
    };
    if contract.protocol != PROTOCOL
        || contract.authority_model != "single_writer"
        || contract.persistence != PERSISTENCE_CALLER_HELD
        || contract.reconciliation != RECONCILE_REJECT_GAP
        || contract.migrations != 0
    {
        return Err(Error::new(
            "RMT-E2002",
            "peer schema is incompatible and no migration is registered",
        ));
    }
    Ok(contract)
}

pub fn negotiate_contract(local: &[u8], remote: &[u8]) -> Result<(), Error> {
    let left = decode_contract(local)?;
    let right = decode_contract(remote)?;
    if left != right {
        return Err(Error::new(
            "RMT-E2002",
            "peer schema is incompatible and no migration is registered",
        ));
    }
    Ok(())
}

/// Goya Pairs contract. Action names are aliases and are not encoded.
pub fn pairs_continuity_contract() -> Result<Vec<u8>, Error> {
    encode_contract(
        crate::pairs::SCHEMA,
        &[crate::pairs::ACTION_SELECT, crate::pairs::ACTION_RESOLVE],
    )
}

fn text_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<String, Error> {
    match fields.get(&id) {
        Some(Value::Text(text)) => Ok(text.clone()),
        _ => Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        )),
    }
}

fn u32_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<u32, Error> {
    match fields.get(&id) {
        Some(Value::Int(number)) => u32::try_from(*number)
            .map_err(|_| Error::new("RMT-E2001", "continuity contract is not canonical")),
        _ => Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        )),
    }
}

fn enum_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<u32, Error> {
    match fields.get(&id) {
        Some(Value::Enum(value)) => Ok(*value),
        _ => Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        )),
    }
}

fn bytes32(fields: &BTreeMap<u32, Value>, id: u32) -> Result<[u8; 32], Error> {
    match fields.get(&id) {
        Some(Value::Bytes(bytes)) if bytes.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(bytes);
            Ok(out)
        }
        _ => Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        )),
    }
}

fn action_list(fields: &BTreeMap<u32, Value>, id: u32) -> Result<Vec<u32>, Error> {
    let Some(Value::Array(items)) = fields.get(&id) else {
        return Err(Error::new(
            "RMT-E2001",
            "continuity contract is not canonical",
        ));
    };
    let mut actions = Vec::with_capacity(items.len());
    let mut previous: Option<u32> = None;
    for item in items {
        let Value::Enum(action) = item else {
            return Err(Error::new(
                "RMT-E2001",
                "continuity contract is not canonical",
            ));
        };
        if previous.is_some_and(|prior| *action <= prior) {
            return Err(Error::new(
                "RMT-E1001",
                "action identifiers must be strictly increasing",
            ));
        }
        previous = Some(*action);
        actions.push(*action);
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairs::SCHEMA;

    #[test]
    fn execution_mode_is_outside_the_contract() {
        let compiled = ExecutionMode::Compiled;
        let interpreted = ExecutionMode::Interpreted;
        assert_ne!(compiled, interpreted);
        let bytes = pairs_continuity_contract().unwrap();
        assert_eq!(bytes, pairs_continuity_contract().unwrap());
        let decoded = decode_contract(&bytes).unwrap();
        assert_eq!(decoded.actions, vec![1, 2]);
        assert_eq!(decoded.migrations, 0);
        assert_eq!(decoded.schema_fingerprint, SCHEMA.fingerprint().unwrap());
        negotiate_contract(&bytes, &bytes).unwrap();
    }

    #[test]
    fn unsorted_actions_and_a_migration_are_rejected() {
        let error = encode_contract(SCHEMA, &[2, 1]).unwrap_err();
        assert_eq!(error.code, "RMT-E1001");
        let mut record = BTreeMap::new();
        let good = decode_contract(&pairs_continuity_contract().unwrap()).unwrap();
        record.insert(1, Value::Text(good.application_id));
        record.insert(2, Value::Text(good.protocol));
        record.insert(3, Value::Int(i64::from(good.schema_version)));
        record.insert(4, Value::Bytes(good.schema_fingerprint.to_vec()));
        record.insert(5, Value::Text(good.authority_model));
        record.insert(
            6,
            Value::Array(good.actions.into_iter().map(Value::Enum).collect()),
        );
        record.insert(7, Value::Enum(PERSISTENCE_CALLER_HELD));
        record.insert(8, Value::Enum(RECONCILE_REJECT_GAP));
        record.insert(9, Value::Int(1));
        let error = decode_contract(&encode(&Value::Record(record)).unwrap()).unwrap_err();
        assert_eq!(error.code, "RMT-E2002");
    }

    #[test]
    fn an_execution_mode_field_is_not_canonical() {
        let Value::Record(mut fields) = decode(&pairs_continuity_contract().unwrap()).unwrap()
        else {
            panic!("contract is not a record");
        };
        fields.insert(10, Value::Enum(2));
        let error = decode_contract(&encode(&Value::Record(fields)).unwrap()).unwrap_err();
        assert_eq!(error.code, "RMT-E2001");
    }
}
