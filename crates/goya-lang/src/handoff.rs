//! Versioned semantic handoff from GY-LANG source into downstream consumers.
//!
//! GSSH/1 is an in-process Rust contract: it carries the complete strictly
//! admitted scene together with source origins bound to the exact source
//! digest. Consumers should branch on `contract` before interpreting it.

use crate::{parse_strict, ParsedScene, StrictParseError};

/// Version identifier for the Goya Shared Semantic Handoff contract.
pub const CONTRACT: &str = "GSSH/1";

/// A strictly admitted, source-bound semantic scene snapshot.
#[derive(Debug, Clone)]
pub struct SemanticHandoff {
    /// Contract identifier for versioned downstream interpretation.
    pub contract: &'static str,
    /// Complete scene semantics and source-map origins from one parse.
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
