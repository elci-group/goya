//! Experimental in-process handoff from GY-LANG source to consumers.
//!
//! This preview wraps a strictly admitted scene and its available source
//! origins. Its identifier is provisional: this is not an approved shared
//! contract, a serialized format, or proof of Android target support. Strict
//! parsing does not supply every origin or resolve bindings and persistence.

use crate::{parse_strict, ParsedScene, StrictParseError};
use std::ops::Deref;

/// Provisional identifier for this in-process preview.
pub const CONTRACT: &str = "GSSH/1";

/// Read-only view of the admitted handoff contents.
///
/// The view may be constructed independently, but only [`SemanticHandoff`]
/// represents a snapshot returned by [`admit`].
#[derive(Debug, Clone)]
pub struct SemanticHandoffView {
    /// Contract identifier for versioned downstream interpretation.
    pub contract: &'static str,
    /// Parsed scene and available source-map origins from one parse.
    pub parsed: ParsedScene,
}

/// A strictly admitted, source-bound semantic scene snapshot.
///
/// Its contents are exposed through an immutable dereference so downstream
/// code cannot change the parsed scene after strict admission.
#[derive(Debug, Clone)]
pub struct SemanticHandoff {
    view: SemanticHandoffView,
}

impl Deref for SemanticHandoff {
    type Target = SemanticHandoffView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

/// Parse and admit source for transfer to a semantic consumer.
///
/// This uses strict GY-LANG admission, so compatibility-only constructs,
/// missing or duplicate authored IDs, and invalid interactions are rejected.
pub fn admit(source: &str) -> Result<SemanticHandoff, StrictParseError> {
    Ok(SemanticHandoff {
        view: SemanticHandoffView {
            contract: CONTRACT,
            parsed: parse_strict(source)?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_exposes_consistent_read_only_snapshot() {
        let handoff = admit(
            "@ontology demo { @cell root { id: \"root\"; text: \"Hi\" } }",
        )
        .unwrap();
        assert_eq!(handoff.contract, CONTRACT);
        assert!(handoff
            .parsed
            .entries
            .iter()
            .all(|entry| entry.source_digest == handoff.parsed.source_digest));
    }
}
