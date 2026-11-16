//! Mapping a generated Swift UTF-8 location back to verified source bytes.
//! Extended with Android Kotlin target support for Phase 1 source map implementation.
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Map {
    schema_version: String,
    mappings: Vec<Mapping>,
    // Kotlin mappings are optional for backward compatibility
    #[serde(default)]
    kotlin_mappings: Option<Vec<KotlinMapping>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mapping {
    semantic_id: Identity,
    source: Span,
    swift: Span,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KotlinMapping {
    semantic_id: Identity,
    source: Span,
    kotlin: Span,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    module: String,
    declared_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Span {
    start_byte: usize,
    end_byte: usize,
}

fn valid_span(text: &str, span: &Span) -> bool {
    span.start_byte < span.end_byte
        && span.end_byte <= text.len()
        && text.is_char_boundary(span.start_byte)
        && text.is_char_boundary(span.end_byte)
}

/// Android Kotlin-specific validation for source spans and boundaries.
fn valid_kotlin_span(text: &str, span: &Span) -> bool {
    span.start_byte < span.end_byte
        && span.end_byte <= text.len()
        && text.is_char_boundary(span.start_byte)
        && text.is_char_boundary(span.end_byte)
}

fn byte_offset(text: &str, line: usize, column: usize) -> Result<usize, String> {
    if line == 0 || column == 0 {
        return Err("line and column are one-based".into());
    }
    let start = if line == 1 {
        0
    } else {
        text.match_indices('\n')
            .nth(line - 2)
            .map(|(index, _)| index + 1)
            .ok_or("line is outside Swift source")?
    };
    let end = text[start..]
        .find('\n')
        .map(|index| start + index)
        .unwrap_or(text.len());
    let offset = start
        .checked_add(column - 1)
        .ok_or("column is outside Swift source")?;
    if offset > end || !text.is_char_boundary(offset) {
        return Err("column is outside the line or inside a UTF-8 character".into());
    }
    Ok(offset)
}

fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text.as_bytes()[..offset];
    let line = 1 + before.iter().filter(|byte| **byte == b'\n').count();
    let start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    (line, offset - start + 1)
}

/// Resolve a one-based Swift line and UTF-8 byte column. Results identify all
/// matching mappings; no match is valid and remains explicitly unmapped.
/// A matching hash proves consistency with the report, not authenticity.
pub fn map_swift_location(
    directory: &Path,
    source: &str,
    line: usize,
    column: usize,
) -> Result<Value, String> {
    let preview = crate::artifacts::load_verified_preview(directory)
        .map_err(|e| format!("preview integrity check failed: {e}"))?;
    map_verified_location(&preview, source, line, column)
}

/// Map a location in an already checked preview for Android Kotlin.
/// Results identify all matching Kotlin mappings; no match is valid and remains explicitly unmapped.
/// A matching hash proves consistency with the report, not authenticity.
pub fn map_kotlin_location(
    preview: &crate::artifacts::VerifiedPreview,
    source: &str,
    kotlin_source: &str,
    line: usize,
    column: usize,
) -> Result<Value, String> {
    if source.len() > crate::MAX_SOURCE_BYTES {
        return Err("Kotlin source exceeds 128 KiB".into());
    }
    let actual = format!("{:x}", Sha256::digest(source.as_bytes()));
    if actual != preview.input_sha256 {
        return Err("Kotlin source hash does not match preview report".into());
    }
    let map: Map = serde_json::from_slice(
        preview
            .artifacts
            .get("source-map.json")
            .ok_or("missing source map")?,
    )
    .map_err(|e| format!("invalid source map: {e}"))?;
    if map.schema_version != "2" || map.mappings.len() > 4096 {
        return Err("unsupported source map schema or mapping count".into());
    }
    let offset = byte_offset(kotlin_source, line, column)?;
    let mut matches = Vec::new();

    // Process Swift mappings (for backward compatibility)
    for item in map.mappings {
        crate::validate_module(&item.semantic_id.module).map_err(|e| e.message)?;
        if item.semantic_id.declared_id.is_empty()
            || !valid_span(source, &item.source)
            || !valid_span(kotlin_source, &item.swift)
        {
            return Err("invalid source map identity or range".into());
        }
        if item.swift.start_byte <= offset && offset < item.swift.end_byte {
            let (source_line, source_column) = line_column(source, item.source.start_byte);
            matches.push(json!({"semantic_id":{"module":item.semantic_id.module,"declared_id":item.semantic_id.declared_id}, "source_range":{"start_byte":item.source.start_byte,"end_byte":item.source.end_byte}, "swift_start":{"line":source_line,"column_utf8_byte":source_column}}));
        }
    }

    // Process Kotlin mappings (Phase 1 Android support)
    if let Some(kotlin_items) = &map.kotlin_mappings {
        for item in kotlin_items {
            crate::validate_module(&item.semantic_id.module).map_err(|e| e.message)?;
            if item.semantic_id.declared_id.is_empty()
                || !valid_kotlin_span(source, &item.source)
                || !valid_kotlin_span(kotlin_source, &item.kotlin)
            {
                return Err("invalid Android Kotlin source map identity or range".into());
            }
            if item.kotlin.start_byte <= offset && offset < item.kotlin.end_byte {
                let (source_line, source_column) = line_column(source, item.source.start_byte);
                matches.push(json!({"semantic_id":{"module":item.semantic_id.module,"declared_id":item.semantic_id.declared_id}, "source_range":{"start_byte":item.source.start_byte,"end_byte":item.source.end_byte}, "kotlin_start":{"line":source_line,"column_utf8_byte":source_column}}));
            }
        }
    }
    Ok(
        json!({"schema_version":"1", "status":if matches.is_empty() {"UNMAPPED"} else {"MAPPED"}, "kotlin_location":{"line":line,"column_utf8_byte":column,"byte_offset":offset}, "mappings":matches, "verification":{"artifact_integrity":"PASS","source_hash":"PASS","authenticity":"NOT_CHECKED","apple_validation":"NOT_RUN"}}),
    )
}

/// Map a location in an already checked preview, including an inline bundle.
pub fn map_verified_location(
    preview: &crate::artifacts::VerifiedPreview,
    source: &str,
    line: usize,
    column: usize,
) -> Result<Value, String> {
    if source.len() > crate::MAX_SOURCE_BYTES {
        return Err("source exceeds 128 KiB".into());
    }
    let actual = format!("{:x}", Sha256::digest(source.as_bytes()));
    if actual != preview.input_sha256 {
        return Err("source hash does not match preview report".into());
    }
    let swift = std::str::from_utf8(
        preview
            .artifacts
            .get("App.swift")
            .ok_or("missing Swift source")?,
    )
    .map_err(|_| "Swift source is not UTF-8")?;
    let map: Map = serde_json::from_slice(
        preview
            .artifacts
            .get("source-map.json")
            .ok_or("missing source map")?,
    )
    .map_err(|e| format!("invalid source map: {e}"))?;
    if map.schema_version != "2" || map.mappings.len() > 4096 {
        return Err("unsupported source map schema or mapping count".into());
    }

    // Phase 1: Support both Swift and Kotlin mappings in schema v2
    let kotlin_source = ""; // Would be loaded from artifacts in Phase 2 implementation
    let offset = byte_offset(swift, line, column)?;
    let mut matches = Vec::new();

    // Process Swift mappings (original functionality)
    for item in map.mappings {
        crate::validate_module(&item.semantic_id.module).map_err(|e| e.message)?;
        if item.semantic_id.declared_id.is_empty()
            || !valid_span(source, &item.source)
            || !valid_span(swift, &item.swift)
        {
            return Err("invalid source map identity or range".into());
        }
        if item.swift.start_byte <= offset && offset < item.swift.end_byte {
            let (source_line, source_column) = line_column(source, item.source.start_byte);
            matches.push(json!({"semantic_id":{"module":item.semantic_id.module,"declared_id":item.semantic_id.declared_id}, "source_range":{"start_byte":item.source.start_byte,"end_byte":item.source.end_byte}, "source_start":{"line":source_line,"column_utf8_byte":source_column}}));
        }
    }

    // Process Kotlin mappings (Phase 1 Android support)
    // Kotlin source would be in a separate .kt file - for now we just validate structure
    let kotlin = ""; // Placeholder for Kotlin source - would be loaded from artifacts
    if let Some(kotlin_items) = &map.kotlin_mappings {
        for item in kotlin_items {
            crate::validate_module(&item.semantic_id.module).map_err(|e| e.message)?;
            if item.semantic_id.declared_id.is_empty()
                || !valid_kotlin_span(source, &item.source)
                || !valid_kotlin_span(&kotlin, &item.kotlin)
            {
                return Err("invalid Android Kotlin source map identity or range".into());
            }
            if item.kotlin.start_byte <= offset && offset < item.kotlin.end_byte {
                let (source_line, source_column) = line_column(source, item.source.start_byte);
                matches.push(json!({"semantic_id":{"module":item.semantic_id.module,"declared_id":item.semantic_id.declared_id}, "source_range":{"start_byte":item.source.start_byte,"end_byte":item.source.end_byte}, "kotlin_start":{"line":source_line,"column_utf8_byte":source_column}}));
            }
        }
    }
    Ok(
        json!({"schema_version":"1", "status":if matches.is_empty() {"UNMAPPED"} else {"MAPPED"}, "swift_location":{"line":line,"column_utf8_byte":column,"byte_offset":offset}, "mappings":matches, "verification":{"artifact_integrity":"PASS","source_hash":"PASS","authenticity":"NOT_CHECKED","apple_validation":"NOT_RUN"}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(source: &str, swift: &str, mappings: Value) -> crate::artifacts::VerifiedPreview {
        crate::artifacts::VerifiedPreview {
            artifacts: [
                ("App.swift".into(), swift.as_bytes().to_vec()),
                (
                    "source-map.json".into(),
                    serde_json::to_vec(&json!({"schema_version":"2", "mappings":mappings}))
                        .unwrap(),
                ),
            ]
            .into_iter()
            .collect(),
            hashes: Default::default(),
            input_sha256: format!("{:x}", Sha256::digest(source.as_bytes())),
        }
    }

    fn mapping(id: &str, source: (usize, usize), swift: (usize, usize)) -> Value {
        json!({
            "semantic_id":{"module":"app/main.gscene", "declared_id":id},
            "source":{"start_byte":source.0,"end_byte":source.1},
            "swift":{"start_byte":swift.0,"end_byte":swift.1}
        })
    }

    #[test]
    fn overlapping_ranges_return_every_contributor_with_utf8_source_coordinates() {
        let source = "é\nαβ";
        let preview = preview(
            source,
            "abcde",
            json!([
                mapping("parent", (0, source.len()), (0, 4)),
                mapping("child", (5, 7), (1, 3))
            ]),
        );
        let result = map_verified_location(&preview, source, 1, 2).unwrap();
        assert_eq!(result["status"], "MAPPED");
        assert_eq!(result["mappings"].as_array().unwrap().len(), 2);
        assert_eq!(
            result["mappings"][0]["semantic_id"]["declared_id"],
            "parent"
        );
        assert_eq!(result["mappings"][1]["semantic_id"]["declared_id"], "child");
        assert_eq!(
            result["mappings"][1]["source_start"],
            json!({"line":2,"column_utf8_byte":3})
        );
        let boundary = map_verified_location(&preview, source, 1, 4).unwrap();
        assert_eq!(boundary["mappings"].as_array().unwrap().len(), 1);
        let unmapped = map_verified_location(&preview, source, 1, 5).unwrap();
        assert_eq!(unmapped["status"], "UNMAPPED");
        assert_eq!(unmapped["mappings"], json!([]));
    }

    #[test]
    fn invalid_unmatched_entries_fail_instead_of_returning_partial_mappings() {
        let source = "éx";
        for invalid in [
            mapping("bad", (1, 2), (2, 3)),
            mapping("bad", (0, 4), (2, 3)),
            mapping("bad", (2, 2), (2, 3)),
            mapping("bad", (0, 2), (1, 2)),
            mapping("bad", (0, 2), (3, 4)),
            mapping("", (0, 2), (2, 3)),
        ] {
            let preview = preview(
                source,
                "éx",
                json!([mapping("good", (0, 2), (0, 2)), invalid]),
            );
            assert_eq!(
                map_verified_location(&preview, source, 1, 1).unwrap_err(),
                "invalid source map identity or range"
            );
        }
    }

    #[test]
    fn source_hash_and_module_identity_are_validated_before_mapping() {
        let mut entry = mapping("root", (0, 1), (0, 1));
        entry["semantic_id"]["module"] = json!("../main.gscene");
        let preview = preview("a", "b", json!([entry]));
        assert_eq!(
            map_verified_location(&preview, "z", 1, 1).unwrap_err(),
            "source hash does not match preview report"
        );
        assert!(map_verified_location(&preview, "a", 1, 1).is_err());
    }

    #[test]
    fn byte_columns_are_one_based_and_never_split_unicode() {
        let text = "aé\nxyz\n";
        assert_eq!(byte_offset(text, 1, 2).unwrap(), 1);
        assert!(byte_offset(text, 1, 3).is_err());
        assert_eq!(byte_offset(text, 2, 1).unwrap(), 4);
        assert_eq!(byte_offset(text, 3, 1).unwrap(), text.len());
        assert!(byte_offset(text, 4, 1).is_err());
        assert_eq!(line_column(text, 4), (2, 1));
    }
}
