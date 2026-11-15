//! Experimental source preview for an ordered column of static text cells.
//!
//! The generated `VStack` preserves child order. It does not reproduce solved
//! rectangles, Dynamic Type, safe area, or accessibility behavior, and it is
//! not an admitted iOS build.

use crate::swift::{Argument, Expr, Identifier};
use goya_ir::Node;
use goya_lang::{CellKind, Lowered, Role, Scene, SceneCell, SourceEntry, SourceRange};
use sha2::{Digest, Sha256};

pub const MAPPING: &str = "neptune.swiftui.static-column/0.0.1";
const MAX_CELLS: usize = 32;
const MAX_DEPTH: usize = 16;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub struct Preview(serde_json::Value);

impl Preview {
    pub fn to_json(&self) -> serde_json::Value {
        self.0.clone()
    }

    pub fn swift(&self) -> &str {
        self.0["swift"].as_str().expect("generated Swift")
    }
}

struct Mapping {
    declared_id: String,
    source: SourceRange,
    swift_start: usize,
    swift_end: usize,
}

fn reject(message: &str, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E2104",
        message: message.into(),
        range,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn text_expr(value: &str) -> String {
    Expr::Call {
        callee: Identifier::new("Text").expect("fixed identifier"),
        arguments: vec![Argument {
            label: Some(Identifier::new("verbatim").expect("fixed identifier")),
            value: Expr::StringLiteral(value.to_owned()),
        }],
    }
    .render()
}

fn indent(out: &mut String, level: usize) {
    out.push_str(&"    ".repeat(level));
}

fn is_virtual_root(scene: &Scene) -> bool {
    let root = &scene.root;
    root.authored_id.is_none()
        && root.name == scene.name
        && root.kind == CellKind::Cell
        && !root.is_input
        && root.position.is_none()
        && root.extent.is_none()
        && root.position_p.is_none()
        && root.extent_p.is_none()
        && root.fill.is_none()
        && root.stroke.is_none()
        && root.stroke_width.is_none()
        && root.text.is_none()
        && root.text_size.is_none()
        && root.anchor.is_none()
        && root.wrap.is_none()
        && !root.scroll
        && root.children.len() > 1
}

fn cell_entry<'a>(entries: &'a [SourceEntry], path: &[usize]) -> Option<&'a SourceEntry> {
    entries.iter().find(|entry| {
        matches!(entry.kind.as_str(), "cell" | "button" | "input")
            && entry.scene_path.as_deref() == Some(path)
    })
}

fn text_entry<'a>(entries: &'a [SourceEntry], path: &[usize]) -> Option<&'a SourceEntry> {
    entries.iter().find(|entry| {
        entry.kind == "property"
            && entry.name == "text"
            && entry.scene_path.as_deref() == Some(path)
    })
}

fn unsupported_entry(entries: &[SourceEntry]) -> Option<&SourceEntry> {
    entries.iter().find(|entry| match entry.kind.as_str() {
        "ontology" | "cell" => false,
        "property" => !matches!(entry.name.as_str(), "id" | "text"),
        _ => true,
    })
}

fn unexpected_visual(cell: &SceneCell) -> Option<&'static str> {
    if cell.position.is_some() {
        Some("position")
    } else if cell.extent.is_some() {
        Some("extent")
    } else if cell.position_p.is_some() {
        Some("position_p")
    } else if cell.extent_p.is_some() {
        Some("extent_p")
    } else if cell.fill.is_some() {
        Some("fill")
    } else if cell.stroke.is_some() {
        Some("stroke")
    } else if cell.stroke_width.is_some() {
        Some("stroke_width")
    } else if cell.text_size.is_some() {
        Some("text_size")
    } else if cell.anchor.is_some() {
        Some("anchor")
    } else if cell.wrap.is_some() {
        Some("wrap")
    } else if cell.scroll {
        Some("scroll")
    } else {
        None
    }
}

fn validate_cell(
    cell: &SceneCell,
    path: &[usize],
    depth: usize,
    count: &mut usize,
    entries: &[SourceEntry],
) -> Result<(), crate::Diagnostic> {
    let range = cell_entry(entries, path).map(|entry| entry.range);
    if depth > MAX_DEPTH {
        return Err(reject(
            "static column supports at most 16 nested cells",
            range,
        ));
    }
    *count += 1;
    if *count > MAX_CELLS {
        return Err(reject("static column supports at most 32 cells", range));
    }
    if cell.kind != CellKind::Cell || cell.is_input {
        return Err(reject("static column admits only cells", range));
    }
    if let Some(name) = unexpected_visual(cell) {
        return Err(reject(
            &format!("static column rejects property '{name}'"),
            range,
        ));
    }
    if cell.text.is_none() && cell.children.is_empty() {
        return Err(reject(
            "static cell needs authored text or child cells",
            range,
        ));
    }
    for (index, child) in cell.children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(index);
        validate_cell(child, &child_path, depth + 1, count, entries)?;
    }
    Ok(())
}

fn scene_texts(cell: &SceneCell, out: &mut Vec<(String, String)>) {
    if let (Some(id), Some(text)) = (&cell.authored_id, &cell.text) {
        out.push((id.clone(), text.clone()));
    }
    for child in &cell.children {
        scene_texts(child, out);
    }
}

fn collect_ir_text(lowered: &Lowered, id: goya_ir::NodeId, out: &mut Vec<String>) -> bool {
    match lowered.module.node(id) {
        Some(Node::Text { content }) => {
            out.push(content.clone());
            true
        }
        Some(Node::Column { children }) => children
            .iter()
            .all(|child| collect_ir_text(lowered, *child, out)),
        _ => false,
    }
}

fn ir_matches(scene: &Scene, lowered: &Lowered) -> Result<(), crate::Diagnostic> {
    if lowered
        .table
        .values()
        .any(|record| record.explicit_rect_mp.is_some())
    {
        return Err(reject("shared IR retained solved geometry", None));
    }
    let mut expected = Vec::new();
    scene_texts(&scene.root, &mut expected);
    let mut actual = Vec::new();
    let root = lowered
        .module
        .root
        .ok_or_else(|| reject("shared IR text order differs from the scene", None))?;
    if !collect_ir_text(lowered, root, &mut actual)
        || actual.len() != expected.len()
        || expected
            .iter()
            .zip(&actual)
            .any(|((_, text), actual)| text != actual)
    {
        return Err(reject("shared IR text order differs from the scene", None));
    }
    for (id, text) in &expected {
        let preserved = lowered.table.values().any(|record| {
            record.role == Role::Text
                && record.semantic_id.as_deref() == Some(id.as_str())
                && matches!(
                    lowered.module.node(record.ir_node),
                    Some(Node::Text { content }) if content == text
                )
        });
        if !preserved {
            return Err(reject("shared IR did not preserve a text identity", None));
        }
    }
    Ok(())
}

fn push_mapping(
    mappings: &mut Vec<Mapping>,
    declared_id: &str,
    source: SourceRange,
    swift_start: usize,
    swift_end: usize,
) {
    mappings.push(Mapping {
        declared_id: declared_id.to_owned(),
        source,
        swift_start,
        swift_end,
    });
}

fn emit_cell(
    out: &mut String,
    cell: &SceneCell,
    path: &[usize],
    entries: &[SourceEntry],
    level: usize,
    mappings: &mut Vec<Mapping>,
) -> Result<(), crate::Diagnostic> {
    let cell_source = cell_entry(entries, path)
        .map(|entry| entry.range)
        .ok_or_else(|| reject("cell source range unavailable", None))?;
    let declared_id = cell.authored_id.as_deref().ok_or_else(|| {
        reject(
            "static cell requires an authored identity",
            Some(cell_source),
        )
    })?;
    indent(out, level);
    let start = out.len();
    let mapping_at = mappings.len();
    push_mapping(mappings, declared_id, cell_source, start, start);
    if cell.children.is_empty() {
        let text = cell.text.as_deref().ok_or_else(|| {
            reject(
                "static cell needs authored text or child cells",
                Some(cell_source),
            )
        })?;
        out.push_str(&text_expr(text));
        let end = out.len();
        mappings[mapping_at].swift_end = end;
        let text_source = text_entry(entries, path)
            .map(|entry| entry.range)
            .ok_or_else(|| reject("text source range unavailable", Some(cell_source)))?;
        push_mapping(mappings, declared_id, text_source, start, end);
        return Ok(());
    }
    out.push_str("VStack(alignment: .leading, spacing: 0) {\n");
    if let Some(text) = &cell.text {
        indent(out, level + 1);
        let text_start = out.len();
        out.push_str(&text_expr(text));
        let text_end = out.len();
        out.push('\n');
        let text_source = text_entry(entries, path)
            .map(|entry| entry.range)
            .ok_or_else(|| reject("text source range unavailable", Some(cell_source)))?;
        push_mapping(mappings, declared_id, text_source, text_start, text_end);
    }
    for (index, child) in cell.children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(index);
        emit_cell(out, child, &child_path, entries, level + 1, mappings)?;
        out.push('\n');
    }
    indent(out, level);
    out.push('}');
    mappings[mapping_at].swift_end = out.len();
    Ok(())
}

fn emit_scene(
    scene: &Scene,
    entries: &[SourceEntry],
    mappings: &mut Vec<Mapping>,
) -> Result<String, crate::Diagnostic> {
    let mut out = String::from(
        "import SwiftUI\n\n@main\nstruct NeptuneApp: App {\n    var body: some Scene {\n        WindowGroup {\n",
    );
    if is_virtual_root(scene) {
        indent(&mut out, 3);
        out.push_str("VStack(alignment: .leading, spacing: 0) {\n");
        for (index, child) in scene.root.children.iter().enumerate() {
            emit_cell(&mut out, child, &[index], entries, 4, mappings)?;
            out.push('\n');
        }
        indent(&mut out, 3);
        out.push('}');
    } else {
        emit_cell(&mut out, &scene.root, &[], entries, 3, mappings)?;
    }
    out.push_str("\n        }\n    }\n}\n");
    if out.len() > MAX_OUTPUT_BYTES {
        return Err(reject("static column preview exceeds 2 MiB", None));
    }
    Ok(out)
}

/// Generate a source preview. Apple compilation and iOS admission are not run.
pub fn preview(source: &str, module: &str) -> Result<Preview, crate::Diagnostic> {
    crate::validate_module(module)?;
    crate::inspect(source)?;
    let parsed = goya_lang::parse_strict(source).map_err(|error| crate::Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
    })?;
    if let Some(entry) = unsupported_entry(&parsed.entries) {
        let message = match entry.kind.as_str() {
            "property" => format!("static column rejects property '{}'", entry.name),
            "button" => "static column rejects buttons".to_owned(),
            "input" => "static column rejects inputs".to_owned(),
            "interaction" => "static column rejects interactions".to_owned(),
            _ => format!("static column rejects {}", entry.kind),
        };
        return Err(reject(&message, Some(entry.range)));
    }
    if !parsed.scene.interactions.is_empty() {
        return Err(reject("static column rejects interactions", None));
    }
    let mut count = 0usize;
    if is_virtual_root(&parsed.scene) {
        for (index, child) in parsed.scene.root.children.iter().enumerate() {
            validate_cell(child, &[index], 1, &mut count, &parsed.entries)?;
        }
    } else {
        validate_cell(&parsed.scene.root, &[], 1, &mut count, &parsed.entries)?;
    }
    let lowered = goya_lang::lower(&parsed.scene, 1.0, 1.0)
        .map_err(|error| reject(&error.to_string(), None))?;
    ir_matches(&parsed.scene, &lowered)?;
    let mut mappings = Vec::new();
    let swift = emit_scene(&parsed.scene, &parsed.entries, &mut mappings)?;
    let source_range =
        |range: SourceRange| serde_json::json!({"start_byte": range.start, "end_byte": range.end});
    let swift_range = |start, end| serde_json::json!({"start_byte": start, "end_byte": end});
    let mapped: Vec<_> = mappings
        .iter()
        .map(|mapping| {
            serde_json::json!({
                "semantic_id": {"module": module, "declared_id": mapping.declared_id},
                "source_range": source_range(mapping.source),
                "swift_range": swift_range(mapping.swift_start, mapping.swift_end)
            })
        })
        .collect();
    Ok(Preview(serde_json::json!({
        "schema_version": "1",
        "operation": "preview-static-column",
        "profile": "experimental",
        "mapping": MAPPING,
        "verification": "SOURCE_PREVIEW",
        "scope": "static_text_column",
        "module": module,
        "cell_count": count,
        "source_sha256": hex(&parsed.source_digest),
        "swift_sha256": format!("{:x}", Sha256::digest(swift.as_bytes())),
        "swift": swift,
        "source_map": {"schema_version": "2", "mappings": mapped},
        "capabilities": {
            "schema_version": "1",
            "profile": "experimental",
            "mapping": MAPPING,
            "frameworks": ["SwiftUI"],
            "minimum_os": null,
            "entitlements": [],
            "usage_description_keys": [],
            "privacy_declarations": "owner_review_required",
            "test_status": "apple_validation_not_run",
            "fallback": "reject",
            "admitted": ["goya.text.static"],
            "layout": "child_order_only"
        },
        "stages": {
            "goya_frontend": "PASS",
            "layout_fidelity": "NOT_CLAIMED",
            "dynamic_type": "NOT_CLAIMED",
            "accessibility": "NOT_VERIFIED",
            "apple_validation": "NOT_RUN",
            "ios_admission": "NOT_RUN"
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLUMN: &str = include_str!("../tests/static-column.gscene");
    const ONE: &str = include_str!("../tests/static.gscene");

    fn cells(count: usize) -> String {
        let mut source = String::from("@ontology demo {\n");
        for index in 0..count {
            source.push_str(&format!(
                "  @cell c{index} {{ id: \"c{index}\" text: \"t{index}\" }}\n"
            ));
        }
        source.push_str("}\n");
        source
    }

    fn nested(depth: usize) -> String {
        let mut source = String::from("@ontology demo { ");
        for index in 0..depth {
            source.push_str(&format!("@cell c{index} {{ id: \"c{index}\" "));
        }
        source.push_str("text: \"end\" ");
        for _ in 0..depth {
            source.push_str("} ");
        }
        source.push('}');
        source
    }

    #[test]
    fn column_preserves_child_order_and_is_deterministic() {
        let first = preview(COLUMN, "app/main.gscene").unwrap();
        assert_eq!(
            first.to_json(),
            preview(COLUMN, "app/main.gscene").unwrap().to_json()
        );
        let swift = first.swift();
        let hello = swift.find("Text(verbatim: \"Hello\")").unwrap();
        let title = swift.find("Text(verbatim: \"Title\")").unwrap();
        let note = swift.find("Text(verbatim: \"Note\")").unwrap();
        assert!(hello < title && title < note);
        assert!(swift.contains("VStack(alignment: .leading, spacing: 0)"));
        assert!(!swift.contains(".font") && !swift.contains(".frame"));
        let json = first.to_json();
        assert_eq!(json["cell_count"], 3);
        assert_eq!(json["mapping"], MAPPING);
        assert_eq!(json["stages"]["layout_fidelity"], "NOT_CLAIMED");
        assert_eq!(json["stages"]["apple_validation"], "NOT_RUN");
        assert_eq!(json["stages"]["ios_admission"], "NOT_RUN");
        for mapping in json["source_map"]["mappings"].as_array().unwrap() {
            let source = &mapping["source_range"];
            let generated = &mapping["swift_range"];
            let source_span = COLUMN
                .get(
                    source["start_byte"].as_u64().unwrap() as usize
                        ..source["end_byte"].as_u64().unwrap() as usize,
                )
                .unwrap();
            let swift_span = swift
                .get(
                    generated["start_byte"].as_u64().unwrap() as usize
                        ..generated["end_byte"].as_u64().unwrap() as usize,
                )
                .unwrap();
            assert!(!source_span.is_empty());
            assert!(!swift_span.is_empty());
        }
        assert!(crate::preview_static_text(COLUMN).is_err());
    }

    #[test]
    fn one_cell_stays_a_verbatim_text_and_escapes() {
        let swift = preview(ONE, "app/main.gscene").unwrap().swift().to_owned();
        assert!(swift.contains("Text(verbatim: \"Hello\")"));
        assert!(!swift.contains("VStack"));
        let escaped = COLUMN.replace("Title", "é\\(unsafe)");
        assert!(preview(&escaped, "app/main.gscene")
            .unwrap()
            .swift()
            .contains("é\\\\(unsafe)"));
    }

    #[test]
    fn sibling_order_follows_the_source() {
        let swift = preview(&cells(2), "app/main.gscene")
            .unwrap()
            .swift()
            .to_owned();
        let left = swift.find("Text(verbatim: \"t0\")").unwrap();
        let right = swift.find("Text(verbatim: \"t1\")").unwrap();
        assert!(left < right);
        assert_eq!(
            preview(&cells(32), "app/main.gscene").unwrap().to_json()["cell_count"],
            32
        );
        assert!(preview(&cells(33), "app/main.gscene").is_err());
        assert!(preview(&nested(16), "app/main.gscene").is_ok());
        let deep = preview(&nested(17), "app/main.gscene").unwrap_err();
        assert_eq!(deep.code, "NPT-E2104");
        assert!(deep.message.contains("16 nested"));
    }

    #[test]
    fn geometry_controls_and_empty_cells_fail_closed() {
        let colored = COLUMN.replace("text: \"Hello\"", "text: \"Hello\" fill: #ffffff");
        let error = preview(&colored, "app/main.gscene").unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(&colored[range.start..range.end], "fill: #ffffff");
        let positioned =
            "@ontology demo { @cell root { id: \"root\" text: \"Hi\" position: (0, 0, 0) } }";
        let error = preview(positioned, "app/main.gscene").unwrap_err();
        let range = error.range.unwrap();
        assert!(positioned[range.start..range.end].starts_with("position"));
        for source in [
            "@ontology demo { @cell root { id: \"root\" text: \"Hi\" @button go { id: \"go\" text: \"Go\" } } }",
            "@ontology demo { @cell root { id: \"root\" text: \"Hi\" @input name { id: \"name\" text: \"Name\" } } }",
            "@ontology demo { @cell root { id: \"root\" } }",
            "@ontology demo { @cell a { id: \"same\" text: \"A\" } @cell b { id: \"same\" text: \"B\" } }",
        ] {
            assert!(preview(source, "app/main.gscene").is_err(), "{source}");
        }
        let duplicate = preview(
            "@ontology demo { @cell a { id: \"same\" text: \"A\" } @cell b { id: \"same\" text: \"B\" } }",
            "app/main.gscene",
        )
        .unwrap_err();
        assert_eq!(duplicate.code, "GY-E1005");
        assert_eq!(
            preview(COLUMN, "../app.gscene").unwrap_err().code,
            "NPT-E1006"
        );
    }
}
