//! Experimental in-process handoff from GY-LANG source to consumers.
//!
//! This preview wraps a strictly admitted scene and its available source
//! origins. Its identifier is provisional: this is not an approved shared
//! contract, a serialized format, or proof of Android target support. Strict
//! parsing does not supply every origin or resolve bindings and persistence.

use crate::{parse_strict, ParsedScene, StrictParseError};

/// Provisional identifier for this in-process preview.
pub const CONTRACT: &str = "GSSH/1";

/// A strictly admitted, source-bound semantic scene snapshot.
#[derive(Debug, Clone)]
pub struct SemanticHandoff {
    /// Contract identifier for versioned downstream interpretation.
    pub contract: &'static str,
    /// Parsed scene and available source-map origins from one parse.
    pub parsed: ParsedScene,
}

/// Parse and admit source for transfer to a semantic consumer.
///
/// This uses strict GY-LANG admission, so compatibility-only constructs,
/// missing or duplicate authored IDs, and invalid interactions are rejected.
pub fn admit(source: &str) -> Result<SemanticHandoff, StrictParseError> {
    Ok(SemanticHandoff {
        contract: CONTRACT,
        parsed: parse_strict(source)?,
    })
}
