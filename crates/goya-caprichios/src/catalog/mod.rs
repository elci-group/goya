//! Draft capability definitions. Entries describe design intent only: they are
//! not recipes, executable implementations, or conformance attestations.
mod compute;
mod data;
mod device;

/// Version shared by these initial independently identified draft definitions.
pub const DEFINITION_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Definition {
    /// Stable, lowercase, dotted identifier within the draft catalog.
    pub id: &'static str,
    pub summary: &'static str,
    pub operations: &'static [&'static str],
    pub invariants: &'static [&'static str],
}

impl Definition {
    /// Draft definition version, not an implementation or ABI version.
    pub const fn version(&self) -> &'static str {
        DEFINITION_VERSION
    }

    /// Proposed contract identifier. This does not assert a finalized contract.
    pub fn proposed_contract(&self) -> String {
        format!("goya.{}.v1", self.id)
    }
}

/// Fixed catalog order; capability availability must be negotiated separately.
pub fn definitions() -> impl Iterator<Item = &'static Definition> {
    data::DEFINITIONS
        .iter()
        .chain(compute::DEFINITIONS)
        .chain(device::DEFINITIONS)
}

pub fn find(id: &str) -> Option<&'static Definition> {
    definitions().find(|definition| definition.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_has_80_unique_well_formed_versioned_definitions() {
        let mut ids = BTreeSet::new();
        let all: Vec<_> = definitions().collect();
        assert_eq!(all.len(), 80);
        for definition in all {
            assert!(ids.insert(definition.id), "duplicate {}", definition.id);
            assert_eq!(definition.version(), "0.0.1");
            assert!(definition.id.split('.').count() >= 2);
            assert!(definition.id.split('.').all(|part| !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')));
            assert!(!definition.summary.trim().is_empty());
            assert!(definition.operations.len() >= 2);
            assert!(definition.invariants.len() >= 2);
            assert!(definition
                .operations
                .iter()
                .chain(definition.invariants)
                .all(|s| !s.trim().is_empty()));
            assert_eq!(find(definition.id), Some(definition));
        }
        assert_eq!(find("not.a.capability"), None);
    }
}
