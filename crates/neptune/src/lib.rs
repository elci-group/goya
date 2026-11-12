//! Neptune frontend inspection. No iOS semantics are admitted yet.
use goya_lang::{lex, parse_strict, parse_with_source_map, SourceRange, TokenKind};
use sha2::{Digest, Sha256};

pub mod swift;

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

/// An experimental source preview and the text expression's source mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePreview {
    pub swift: String,
    pub declared_id: String,
    pub source_range: SourceRange,
    pub swift_range: SourceRange,
}

/// Canonical JSON package for the experimental source preview. Hashes cover
/// the exact UTF-8 bytes of the source and the included artifact strings.
pub fn preview_bundle(source: &str) -> Result<String, Diagnostic> {
    let preview = preview_static_text(source)?;
    let map = serde_json::json!({"schema_version":"1", "mappings":[{"declared_id":preview.declared_id,"source":{"start_byte":preview.source_range.start,"end_byte":preview.source_range.end},"swift":{"start_byte":preview.swift_range.start,"end_byte":preview.swift_range.end}}]}).to_string() + "\n";
    let capabilities = serde_json::json!({"schema_version":"1", "profile":"source_preview", "capabilities":[{"semantic_id":"goya.text.static", "adapter":"swiftui.Text", "status":"experimental", "minimum_os":null, "test_status":"apple_validation_not_run", "fallback":"reject"}]}).to_string() + "\n";
    fn hash(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }
    let bundle = serde_json::json!({
        "schema_version":"1", "operation":"preview", "verification":"SOURCE_PREVIEW",
        "artifacts": {"App.swift":preview.swift, "source-map.json":map, "capabilities.json":capabilities},
        "report": {"schema_version":"1", "input_sha256":hash(source.as_bytes()),
            "compiler":{"neptune_version":env!("CARGO_PKG_VERSION"), "goya_language_contract":goya_lang::LANG_CONTRACT, "goya_ir_contract":goya_ir::IR_CONTRACT},
            "artifact_sha256":{"App.swift":hash(preview.swift.as_bytes()), "source-map.json":hash(map.as_bytes()), "capabilities.json":hash(capabilities.as_bytes())},
            "stages":{"goya_frontend":"PASS", "ios_admission":"NOT_RUN", "apple_validation":"NOT_RUN"}}
    });
    Ok(bundle.to_string() + "\n")
}

/// Generate a source-only preview for one static, text-only scene.
/// This does not admit the mapping for an iOS build or verify Swift syntax.
pub fn preview_static_text(source: &str) -> Result<SourcePreview, Diagnostic> {
    // Run the bounded frontend before using the strict parsed result.
    inspect(source)?;
    let parsed = parse_strict(source).map_err(|error| Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
    })?;
    let cells: Vec<_> = parsed
        .entries
        .iter()
        .filter(|entry| entry.kind == "cell")
        .collect();
    let properties: Vec<_> = parsed
        .entries
        .iter()
        .filter(|entry| entry.kind == "property")
        .collect();
    let text_property = properties.iter().find(|entry| entry.name == "text");
    if cells.len() != 1
        || properties.len() != 2
        || text_property.is_none()
        || !properties.iter().any(|entry| entry.name == "id")
        || parsed.entries.len() != 3
        || !parsed.scene.interactions.is_empty()
        || !parsed.scene.root.children.is_empty()
        || cells[0].scene_path.as_deref() != Some(&[])
    {
        return Err(Diagnostic {
            code: "NPT-E2104",
            message: "source preview supports exactly one text-only cell".into(),
            range: None,
        });
    }
    let text_range = text_property.expect("validated text property").range;
    let lowered = goya_lang::lower(&parsed.scene, 1.0, 1.0).map_err(|error| Diagnostic {
        code: "NPT-E2104",
        message: error.to_string(),
        range: None,
    })?;
    let text = parsed.scene.root.text.as_ref().ok_or_else(|| Diagnostic {
        code: "NPT-E2104",
        message: "text is required".into(),
        range: Some(text_range),
    })?;
    if !lowered
        .module
        .nodes
        .iter()
        .any(|(_, node)| matches!(node, goya_ir::Node::Text { content } if content == text))
    {
        return Err(Diagnostic {
            code: "NPT-E2104",
            message: "shared IR did not preserve the text".into(),
            range: Some(text_range),
        });
    }
    let expr = swift::Expr::Call {
        callee: swift::Identifier::new("Text").expect("fixed Swift identifier"),
        arguments: vec![swift::Argument {
            label: None,
            value: swift::Expr::StringLiteral(text.clone()),
        }],
    }
    .render();
    let prefix = "import SwiftUI\n\n@main\nstruct NeptuneApp: App {\n    var body: some Scene {\n        WindowGroup {\n            ";
    let suffix = "\n        }\n    }\n}\n";
    let start = prefix.len();
    let end = start + expr.len();
    Ok(SourcePreview {
        swift: format!("{prefix}{expr}{suffix}"),
        declared_id: parsed
            .scene
            .root
            .authored_id
            .clone()
            .ok_or_else(|| Diagnostic {
                code: "NPT-E2104",
                message: "source node lacks an authored ID".into(),
                range: Some(cells[0].range),
            })?,
        source_range: text_range,
        swift_range: SourceRange { start, end },
    })
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
    // Preserve the line reported by the compatibility parser for syntax errors.
    // The strict frontend currently returns a zero range for those failures.
    parse_with_source_map(source).map_err(|error| Diagnostic {
        code: "NPT-E1002",
        message: error.to_string(),
        range: line_range(source, error.line),
    })?;
    let parsed = parse_strict(source).map_err(|error| Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
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
        let source = "@ontology demo { @cell root { id: \"root\" text: \"é\" } }";
        let diagnostics = inspect(source).unwrap();
        let range = diagnostics
            .iter()
            .find(|d| d.message.contains("'text'"))
            .and_then(|d| d.range)
            .unwrap();
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
        let children: String = (0..300)
            .map(|i| format!("@cell a {{ id: \"item{i}\" text: \"x\" }} "))
            .collect();
        let source = format!("@ontology x {{ @cell root {{ id: \"root\" {children} }} }}");
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

    #[test]
    fn strict_frontend_rejects_discarded_semantics() {
        let source = "@ontology x { @cell a { mystery: 1 } }";
        let error = inspect(source).unwrap_err();
        assert_eq!(error.code, "GY-E1001");
        let range = error.range.unwrap();
        assert!(source[range.start..range.end].starts_with("mystery"));
    }

    #[test]
    fn preview_maps_text_and_rejects_unhandled_properties() {
        let source = "@ontology x { @cell root { id: \"root\" text: \"é\\(name)\" } }";
        let preview = preview_static_text(source).unwrap();
        assert_eq!(
            &source[preview.source_range.start..preview.source_range.end],
            "text: \"é\\(name)\""
        );
        assert_eq!(
            &preview.swift[preview.swift_range.start..preview.swift_range.end],
            "Text(\"é\\\\(name)\")"
        );
        assert_eq!(preview, preview_static_text(source).unwrap());
        assert!(
            preview_static_text("@ontology x { @cell root { text: \"hi\" fill: #ffffff } }")
                .is_err()
        );
        assert!(preview_static_text("@ontology x { @button b { text: \"hi\" } }").is_err());
    }

    #[test]
    fn preview_bundle_is_deterministic_and_hashes_artifact_bytes() {
        let source = "@ontology x { @cell root { id: \"root\" text: \"hello\" } }";
        let first = preview_bundle(source).unwrap();
        assert_eq!(first, preview_bundle(source).unwrap());
        assert_eq!(first, preview_bundle(source).unwrap());
        let bundle: serde_json::Value = serde_json::from_str(&first).unwrap();
        let swift = bundle["artifacts"]["App.swift"].as_str().unwrap();
        assert_eq!(
            bundle["report"]["artifact_sha256"]["App.swift"],
            format!("{:x}", Sha256::digest(swift.as_bytes()))
        );
        assert_eq!(bundle["report"]["stages"]["apple_validation"], "NOT_RUN");
    }
}
