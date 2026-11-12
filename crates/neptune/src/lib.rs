//! Neptune frontend inspection. No iOS semantics are admitted yet.
use goya_lang::{parse_with_source_map, SourceRange};

pub const MAX_SOURCE_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
    pub range: Option<SourceRange>,
}

/// Parse with Goya and report unsupported target semantics.
pub fn inspect(source: &str) -> Result<Vec<Diagnostic>, Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic { code: "NPT-E1001", message: "source exceeds 128 KiB".into(), range: None });
    }
    let parsed = parse_with_source_map(source).map_err(|error| Diagnostic {
        code: "NPT-E1002", message: error.to_string(), range: None,
    })?;
    let mut diagnostics: Vec<_> = parsed.entries.iter().filter(|entry| matches!(entry.kind.as_str(), "button" | "input" | "interaction" | "template" | "property"))
        .map(|entry| Diagnostic { code: "NPT-E2104", message: format!("{} '{}' has no admitted iOS lowering", entry.kind, entry.name), range: Some(entry.range) }).collect();
    diagnostics.push(Diagnostic { code: "NPT-E2104", message: "no iOS presentation mapping is admitted yet".into(), range: None });
    Ok(diagnostics)
}
