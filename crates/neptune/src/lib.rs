//! Neptune frontend inspection. No iOS semantics are admitted yet.
use goya_lang::{lex, parse_with_source_map, SourceRange, TokenKind};

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
        return Err(Diagnostic {
            code: "NPT-E1001",
            message: "source exceeds 128 KiB".into(),
            range: None,
        });
    }
    let tokens = lex(source).map_err(|error| Diagnostic {
        code: "NPT-E1002",
        message: error.to_string(),
        range: None,
    })?;
    let mut depth = 0usize;
    for token in tokens {
        match token.kind {
            TokenKind::LBrace | TokenKind::LParen => {
                depth += 1;
                if depth > 128 {
                    return Err(Diagnostic {
                        code: "NPT-E1003",
                        message: "source nesting exceeds 128".into(),
                        range: Some(SourceRange {
                            start: token.byte_start,
                            end: token.byte_end,
                        }),
                    });
                }
            }
            TokenKind::RBrace | TokenKind::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let parsed = parse_with_source_map(source).map_err(|error| Diagnostic {
        code: "NPT-E1002",
        message: error.to_string(),
        range: None,
    })?;
    let mut diagnostics: Vec<_> = parsed
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind.as_str(),
                "button" | "input" | "interaction" | "template" | "property"
            )
        })
        .map(|entry| Diagnostic {
            code: "NPT-E2104",
            message: format!(
                "{} '{}' has no admitted iOS lowering",
                entry.kind, entry.name
            ),
            range: Some(entry.range),
        })
        .collect();
    diagnostics.push(Diagnostic {
        code: "NPT-E2104",
        message: "no iOS presentation mapping is admitted yet".into(),
        range: None,
    });
    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_property_is_reported_at_original_bytes() {
        let source = "@ontology demo { @cell root { text: \"é\" } }";
        let diagnostics = inspect(source).unwrap();
        let range = diagnostics.iter().find_map(|d| d.range).unwrap();
        assert_eq!(&source[range.start..range.end], "text: \"é\"");
        assert!(diagnostics.iter().any(|d| d.range.is_none()));
    }

    #[test]
    fn malformed_and_deep_input_are_rejected() {
        assert_eq!(inspect("not a scene").unwrap_err().code, "NPT-E1002");
        let source = format!(
            "@ontology x {{ {} {} }}",
            "@cell a {".repeat(129),
            "}".repeat(129)
        );
        assert_eq!(inspect(&source).unwrap_err().code, "NPT-E1003");
    }
}
