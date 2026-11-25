//! State classes, schema fingerprints, and manifest negotiation.
//!
//! A fingerprint covers the stable field id, its class, and its wire type.
//! The source name is an alias. Renaming that alias keeps the fingerprint.

use crate::canonical::{encode, fingerprint_bytes, Value};
use crate::{Error, PROTOCOL};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Shared = 1,
    Local = 2,
    Derived = 3,
    Ephemeral = 4,
    Private = 5,
    Authoritative = 6,
    Resumable = 7,
}

impl Class {
    pub const fn is_portable(self) -> bool {
        matches!(self, Self::Shared | Self::Authoritative | Self::Resumable)
    }

    pub const fn is_device_local(self) -> bool {
        matches!(self, Self::Local | Self::Ephemeral | Self::Private)
    }
}

/// Closed wire-type set for schema identity. Discriminants are part of the fingerprint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WireType {
    Bool = 1,
    Int = 2,
    Float = 3,
    Text = 4,
    Enum = 5,
    IntList = 6,
    Bytes = 7,
    Timestamp = 8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldDef {
    pub id: u32,
    pub name: &'static str,
    pub class: Class,
    pub wire: WireType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schema {
    pub application_id: &'static str,
    pub version: u32,
    pub fields: &'static [FieldDef],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub application_id: &'static str,
    pub protocol: &'static str,
    pub state_schema_version: u32,
    pub schema_fingerprint: [u8; 32],
    pub authority_model: &'static str,
    pub supported_features: &'static [&'static str],
    pub required_features: &'static [&'static str],
}

const SUPPORTED: &[&str] = &["snapshot", "mutation", "handoff", "reconnection"];
const REQUIRED: &[&str] = &["authenticated_transport"];

impl Schema {
    pub fn check(self) -> Result<(), Error> {
        if self.version == 0 {
            return Err(Error::new("RMT-E1001", "schema version zero is reserved"));
        }
        if !application_id_ok(self.application_id) {
            return Err(Error::new(
                "RMT-E1001",
                "application identity is not a stable token",
            ));
        }
        let mut previous: Option<u32> = None;
        let mut names: Vec<&str> = Vec::new();
        for field in self.fields {
            if previous.is_some_and(|id| field.id <= id) {
                return Err(Error::new(
                    "RMT-E1001",
                    "schema field identifiers must be strictly increasing",
                ));
            }
            if !field_name_ok(field.name) || names.contains(&field.name) {
                return Err(Error::new(
                    "RMT-E1001",
                    "schema field name is not a stable token",
                ));
            }
            previous = Some(field.id);
            names.push(field.name);
        }
        Ok(())
    }

    pub fn get(self, id: u32) -> Option<&'static FieldDef> {
        self.fields.iter().find(|field| field.id == id)
    }

    pub fn fingerprint(self) -> Result<[u8; 32], Error> {
        self.check()?;
        let mut items = Vec::with_capacity(self.fields.len());
        for field in self.fields {
            let mut record = BTreeMap::new();
            record.insert(1, Value::Int(i64::from(field.id)));
            record.insert(2, Value::Enum(field.class as u32));
            record.insert(3, Value::Enum(field.wire as u32));
            items.push(Value::Record(record));
        }
        let bytes = encode(&Value::Array(items))?;
        Ok(fingerprint_bytes(&bytes))
    }

    pub fn manifest(self) -> Result<Manifest, Error> {
        Ok(Manifest {
            application_id: self.application_id,
            protocol: PROTOCOL,
            state_schema_version: self.version,
            schema_fingerprint: self.fingerprint()?,
            authority_model: "single_writer",
            supported_features: SUPPORTED,
            required_features: REQUIRED,
        })
    }

    pub fn manifest_json(self) -> Result<String, Error> {
        let manifest = self.manifest()?;
        Ok(manifest_json(&manifest))
    }
}

pub fn negotiate(local: &Manifest, remote: &Manifest) -> Result<(), Error> {
    if local.application_id != remote.application_id
        || local.protocol != remote.protocol
        || local.state_schema_version != remote.state_schema_version
        || local.schema_fingerprint != remote.schema_fingerprint
        || local.authority_model != remote.authority_model
    {
        return Err(Error::new(
            "RMT-E2002",
            "peer schema is incompatible and no migration is registered",
        ));
    }
    Ok(())
}

pub fn manifest_json(manifest: &Manifest) -> String {
    let mut features = String::new();
    for (index, feature) in manifest.supported_features.iter().enumerate() {
        if index > 0 {
            features.push_str(", ");
        }
        features.push('"');
        features.push_str(feature);
        features.push('"');
    }
    let mut required = String::new();
    for (index, feature) in manifest.required_features.iter().enumerate() {
        if index > 0 {
            required.push_str(", ");
        }
        required.push('"');
        required.push_str(feature);
        required.push('"');
    }
    format!(
        "{{\n  \"application_id\": \"{id}\",\n  \"romantico_protocol\": \"{protocol}\",\n  \"state_schema_version\": {version},\n  \"schema_fingerprint\": \"sha256:{fingerprint}\",\n  \"authority_model\": \"{authority}\",\n  \"supported_features\": [{features}],\n  \"required_features\": [{required}]\n}}",
        id = manifest.application_id,
        protocol = manifest.protocol,
        version = manifest.state_schema_version,
        fingerprint = hex(&manifest.schema_fingerprint),
        authority = manifest.authority_model,
        features = features,
        required = required,
    )
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn application_id_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('.')
        && !name.ends_with('.')
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.')
}

fn field_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS: &[FieldDef] = &[FieldDef {
        id: 1,
        name: "count",
        class: Class::Shared,
        wire: WireType::Int,
    }];

    const RENAMED: &[FieldDef] = &[FieldDef {
        id: 1,
        name: "total",
        class: Class::Shared,
        wire: WireType::Int,
    }];

    #[test]
    fn renaming_the_alias_keeps_the_fingerprint() {
        let original = Schema {
            application_id: "goya.pairs",
            version: 1,
            fields: FIELDS,
        };
        let renamed = Schema {
            application_id: "goya.pairs",
            version: 1,
            fields: RENAMED,
        };
        assert_eq!(
            original.fingerprint().unwrap(),
            renamed.fingerprint().unwrap()
        );
        const RETYPED: &[FieldDef] = &[FieldDef {
            id: 1,
            name: "count",
            class: Class::Shared,
            wire: WireType::Text,
        }];
        let changed = Schema {
            application_id: "goya.pairs",
            version: 1,
            fields: RETYPED,
        };
        assert_ne!(
            original.fingerprint().unwrap(),
            changed.fingerprint().unwrap()
        );
    }

    #[test]
    fn mismatched_fingerprint_fails_negotiation() {
        let schema = Schema {
            application_id: "goya.pairs",
            version: 1,
            fields: FIELDS,
        };
        let mut remote = schema.manifest().unwrap();
        remote.schema_fingerprint[0] ^= 0xff;
        let error = negotiate(&schema.manifest().unwrap(), &remote).unwrap_err();
        assert_eq!(error.code, "RMT-E2002");
    }
}
