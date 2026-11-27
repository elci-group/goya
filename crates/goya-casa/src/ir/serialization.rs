//! Deterministic IR serialization with schema versioning (directive §7.4).
//!
//! Serialization is canonical: struct field order is the declaration
//! order, collections are ordered (Vec in source order, BTreeMap/Set in
//! key order), so the same IR always serializes to identical bytes.

pub use crate::ir::casa_ir::{CasaIr, IR_SCHEMA_VERSION};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SerializationError {
    #[error("schema version {found} cannot be read by this build (expected {expected})")]
    Schema { found: u32, expected: u32 },
    #[error("invalid CasaIR JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl CasaIr {
    /// Canonical JSON form.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("CasaIR serialization is infallible")
    }

    /// Read back serialized IR, rejecting unknown schema versions.
    pub fn from_json(json: &str) -> Result<Self, SerializationError> {
        let ir: CasaIr = serde_json::from_str(json)?;
        if ir.schema_version != IR_SCHEMA_VERSION {
            return Err(SerializationError::Schema {
                found: ir.schema_version,
                expected: IR_SCHEMA_VERSION,
            });
        }
        Ok(ir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::parser::ingest_to_ir;

    #[test]
    fn round_trips_identically() {
        let ir = ingest_to_ir(
            "app.css",
            "body { color: red } @media screen { .a { margin: 0 } }",
        );
        let first = ir.to_json();
        let back = CasaIr::from_json(&first).unwrap();
        assert_eq!(back, ir);
        assert_eq!(back.to_json(), first);
    }

    #[test]
    fn serialization_is_byte_stable() {
        let ir = ingest_to_ir("app.css", "div { padding: var(--p) }");
        assert_eq!(ir.to_json(), ir.to_json());
        let again = ingest_to_ir("app.css", "div { padding: var(--p) }");
        assert_eq!(ir.to_json(), again.to_json());
    }

    #[test]
    fn rejects_unknown_schema_versions() {
        let ir = ingest_to_ir("app.css", "body { color: red }");
        let mut json = ir.to_json();
        // Rewrite the recorded schema version to a future one.
        let future = json.replace(
            &format!("\"schema_version\":{IR_SCHEMA_VERSION}"),
            &format!("\"schema_version\":{}", IR_SCHEMA_VERSION + 1),
        );
        assert_ne!(future, json);
        json = future;
        match CasaIr::from_json(&json) {
            Err(SerializationError::Schema { found, expected }) => {
                assert_eq!(found, IR_SCHEMA_VERSION + 1);
                assert_eq!(expected, IR_SCHEMA_VERSION);
            }
            other => panic!("expected schema error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(CasaIr::from_json("{").is_err());
    }
}
