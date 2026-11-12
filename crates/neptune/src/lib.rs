//! Neptune frontend inspection. No iOS semantics are admitted yet.
use goya_lang::{lex, parse_with_source_map, SourceRange, TokenKind};

pub const MAX_SOURCE_BYTES: usize = 128 * 1024;
pub const MAX_DIAGNOSTICS: usize = 256;

fn line_range(source: &str, line: usize) -> Option<SourceRange> {
    if line == 0 {
        return None;
    }
    let mut start = 0;
    for (index, part) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            return Some(SourceRange {
                start,
                end: start + part.trim_end_matches('\n').len(),
            });
        }
        start += part.len();
    }
    if line == source.bytes().filter(|b| *b == b'\n').count() + 1 {
        Some(SourceRange {
            start: source.len(),
            end: source.len(),
        })
    } else {
        None
    }
}

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
        range: line_range(source, error.line),
    })?;
    let mut delimiters = Vec::new();
    for token in tokens {
        match token.kind {
            TokenKind::LBrace | TokenKind::LParen => {
                delimiters.push(token.kind);
                if delimiters.len() > 128 {
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
            TokenKind::RBrace | TokenKind::RParen => {
                let expected = if token.kind == TokenKind::RBrace {
                    TokenKind::LBrace
                } else {
                    TokenKind::LParen
                };
                if delimiters.pop() != Some(expected) {
                    return Err(Diagnostic {
                        code: "NPT-E1002",
                        message: "mismatched closing delimiter".into(),
                        range: Some(SourceRange {
                            start: token.byte_start,
                            end: token.byte_end,
                        }),
                    });
                }
            }
            _ => {}
        }
    }
    let parsed = parse_with_source_map(source).map_err(|error| Diagnostic {
        code: "NPT-E1002",
        message: error.to_string(),
        range: line_range(source, error.line),
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
        .take(MAX_DIAGNOSTICS - 2)
        .collect();
    let omitted = parsed
        .entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.kind.as_str(),
                "button" | "input" | "interaction" | "template" | "property"
            )
        })
        .count()
        .saturating_sub(diagnostics.len());
    if omitted > 0 {
        diagnostics.push(Diagnostic {
            code: "NPT-E1004",
            message: format!("{omitted} further diagnostics omitted"),
            range: None,
        });
    }
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
        let mismatch = inspect("@ontology x { )").unwrap_err();
        assert_eq!(mismatch.range, Some(SourceRange { start: 14, end: 15 }));
    }

    #[test]
    fn diagnostic_output_is_bounded() {
        let source = format!(
            "@ontology x {{ {} }}",
            "@cell a { text: \"x\" } ".repeat(300)
        );
        let diagnostics = inspect(&source).unwrap();
        assert_eq!(diagnostics.len(), MAX_DIAGNOSTICS);
        assert!(diagnostics.iter().any(|d| d.code == "NPT-E1004"));
    }

    #[test]
    fn parse_error_points_to_its_source_line() {
        let source = "@ontology x {\n  ?\n}";
        let error = inspect(source).unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(&source[range.start..range.end], "  ?");
    }
}
