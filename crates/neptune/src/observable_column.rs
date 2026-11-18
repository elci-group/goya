//! Experimental source preview for static text plus one input-free D1 button.
//!
//! Child order follows the scene. The state readout is not an authored cell.
//! Layout fidelity, accessibility, Apple compilation, and iOS admission are
//! not claimed.

use crate::native_swift;
use goya_executable::D1Value;
use goya_ir::{ActionId, Node};
use goya_lang::{
    CellKind, InteractionClass, Lowered, Role, Scene, SceneCell, SourceEntry, SourceRange,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const MAPPING: &str = "neptune.swiftui.observable-column/0.0.1";
const MAX_NODES: usize = 32;
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

struct ButtonFound {
    id: String,
    label: String,
    path: Vec<usize>,
}

#[derive(Clone, PartialEq, Eq)]
enum Item {
    Text(String),
    Button(String),
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

fn node_entry<'a>(entries: &'a [SourceEntry], path: &[usize]) -> Option<&'a SourceEntry> {
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
        "ontology" | "cell" | "button" | "interaction" => false,
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

fn validate_node(
    cell: &SceneCell,
    path: &[usize],
    depth: usize,
    count: &mut usize,
    entries: &[SourceEntry],
    button: &mut Option<ButtonFound>,
) -> Result<(), crate::Diagnostic> {
    let range = node_entry(entries, path).map(|entry| entry.range);
    if depth > MAX_DEPTH {
        return Err(reject(
            "observable column supports at most 16 nested nodes",
            range,
        ));
    }
    *count += 1;
    if *count > MAX_NODES {
        return Err(reject("observable column supports at most 32 nodes", range));
    }
    if cell.is_input || cell.kind == CellKind::Input {
        return Err(reject("observable column rejects inputs", range));
    }
    if let Some(name) = unexpected_visual(cell) {
        return Err(reject(
            &format!("observable column rejects property '{name}'"),
            range,
        ));
    }
    if cell.kind == CellKind::Button {
        if button.is_some() {
            return Err(reject("observable column admits one button", range));
        }
        if !cell.children.is_empty() {
            return Err(reject(
                "observable column button cannot contain children",
                range,
            ));
        }
        let label = cell
            .text
            .as_ref()
            .filter(|text| !text.trim().is_empty())
            .ok_or_else(|| {
                reject(
                    "button needs a nonempty accessible label",
                    text_entry(entries, path).map(|entry| entry.range).or(range),
                )
            })?;
        let id = cell
            .authored_id
            .as_deref()
            .ok_or_else(|| reject("button requires an authored identity", range))?;
        *button = Some(ButtonFound {
            id: id.to_owned(),
            label: label.clone(),
            path: path.to_vec(),
        });
        return Ok(());
    }
    if cell.kind != CellKind::Cell {
        return Err(reject(
            "observable column admits cells and one button",
            range,
        ));
    }
    if cell.text.is_none() && cell.children.is_empty() {
        return Err(reject(
            "static cell needs authored text or child nodes",
            range,
        ));
    }
    for (index, child) in cell.children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(index);
        validate_node(child, &child_path, depth + 1, count, entries, button)?;
    }
    Ok(())
}

fn scene_items(cell: &SceneCell, out: &mut Vec<Item>) {
    if cell.kind == CellKind::Button {
        out.push(Item::Button(cell.text.clone().unwrap_or_default()));
        return;
    }
    if let Some(text) = &cell.text {
        out.push(Item::Text(text.clone()));
    }
    for child in &cell.children {
        scene_items(child, out);
    }
}

fn collect_ir(
    lowered: &Lowered,
    id: goya_ir::NodeId,
    out: &mut Vec<Item>,
    action: &mut Option<ActionId>,
) -> bool {
    match lowered.module.node(id) {
        Some(Node::Text { content }) => {
            out.push(Item::Text(content.clone()));
            true
        }
        Some(Node::Button { label, on_activate }) => {
            if action.is_some() {
                return false;
            }
            *action = Some(*on_activate);
            out.push(Item::Button(label.clone()));
            true
        }
        Some(Node::Column { children }) => children
            .iter()
            .all(|child| collect_ir(lowered, *child, out, action)),
        _ => false,
    }
}

fn ir_matches(
    scene: &Scene,
    lowered: &Lowered,
    button: &ButtonFound,
) -> Result<(), crate::Diagnostic> {
    if lowered
        .table
        .values()
        .any(|record| record.explicit_rect_mp.is_some())
    {
        return Err(reject("shared IR retained solved geometry", None));
    }
    let mut expected = Vec::new();
    scene_items(&scene.root, &mut expected);
    let mut actual = Vec::new();
    let mut action = None;
    let root = lowered
        .module
        .root
        .ok_or_else(|| reject("shared IR order differs from the scene", None))?;
    if !collect_ir(lowered, root, &mut actual, &mut action) || actual != expected {
        return Err(reject("shared IR order differs from the scene", None));
    }
    let action = action.ok_or_else(|| reject("shared IR did not preserve the button", None))?;
    let reference = goya_lang::compile_d1_interactions(scene)
        .map_err(|error| reject(&error.to_string(), None))?;
    if reference.len() != 1 || action != reference[0].trigger() {
        return Err(reject(
            "shared IR button action differs from the D1 interaction",
            None,
        ));
    }
    let preserved = lowered.table.values().any(|record| {
        record.role == Role::Button
            && record.semantic_id.as_deref() == Some(button.id.as_str())
            && matches!(
                lowered.module.node(record.ir_node),
                Some(Node::Button { label, .. }) if label == &button.label
            )
    });
    if !preserved {
        return Err(reject(
            "shared IR did not preserve the button identity",
            None,
        ));
    }
    fn texts(cell: &SceneCell, out: &mut Vec<(String, String)>) {
        if cell.kind != CellKind::Button {
            if let (Some(id), Some(text)) = (&cell.authored_id, &cell.text) {
                out.push((id.clone(), text.clone()));
            }
            for child in &cell.children {
                texts(child, out);
            }
        }
    }
    let mut text_ids = Vec::new();
    texts(&scene.root, &mut text_ids);
    for (id, text) in text_ids {
        let found = lowered.table.values().any(|record| {
            record.role == Role::Text
                && record.semantic_id.as_deref() == Some(id.as_str())
                && matches!(
                    lowered.module.node(record.ir_node),
                    Some(Node::Text { content }) if content == &text
                )
        });
        if !found {
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

fn emit_button(
    out: &mut String,
    button: &SceneCell,
    path: &[usize],
    entries: &[SourceEntry],
    level: usize,
    mappings: &mut Vec<Mapping>,
) -> Result<(), crate::Diagnostic> {
    let source = node_entry(entries, path)
        .map(|entry| entry.range)
        .ok_or_else(|| reject("button source range unavailable", None))?;
    let declared_id = button
        .authored_id
        .as_deref()
        .ok_or_else(|| reject("button requires an authored identity", Some(source)))?;
    let label = button
        .text
        .as_deref()
        .ok_or_else(|| reject("button needs a nonempty accessible label", Some(source)))?;
    indent(out, level);
    let start = out.len();
    let mapping_at = mappings.len();
    push_mapping(mappings, declared_id, source, start, start);
    out.push_str("Button(action: {\n");
    indent(out, level + 1);
    out.push_str("do {\n");
    indent(out, level + 2);
    out.push_str("try store.dispatch0(input: NeptuneInput0())\n");
    indent(out, level + 2);
    out.push_str("actionError = nil\n");
    indent(out, level + 1);
    out.push_str("} catch {\n");
    indent(out, level + 2);
    out.push_str("actionError = String(describing: error)\n");
    indent(out, level + 1);
    out.push_str("}\n");
    indent(out, level);
    out.push_str("}, label: {\n");
    indent(out, level + 1);
    let text_start = out.len();
    out.push_str(
        &crate::swift::Expr::Call {
            callee: crate::swift::Identifier::new("Text").expect("fixed identifier"),
            arguments: vec![crate::swift::Argument {
                label: Some(crate::swift::Identifier::new("verbatim").expect("fixed identifier")),
                value: crate::swift::Expr::StringLiteral(label.to_owned()),
            }],
        }
        .render(),
    );
    let text_end = out.len();
    out.push('\n');
    indent(out, level);
    out.push_str("})");
    mappings[mapping_at].swift_end = out.len();
    let text_source = text_entry(entries, path)
        .map(|entry| entry.range)
        .ok_or_else(|| reject("button label source range unavailable", Some(source)))?;
    push_mapping(mappings, declared_id, text_source, text_start, text_end);
    Ok(())
}

fn emit_node(
    out: &mut String,
    cell: &SceneCell,
    path: &[usize],
    entries: &[SourceEntry],
    level: usize,
    mappings: &mut Vec<Mapping>,
) -> Result<(), crate::Diagnostic> {
    if cell.kind == CellKind::Button {
        return emit_button(out, cell, path, entries, level, mappings);
    }
    let source = node_entry(entries, path)
        .map(|entry| entry.range)
        .ok_or_else(|| reject("cell source range unavailable", None))?;
    let declared_id = cell
        .authored_id
        .as_deref()
        .ok_or_else(|| reject("static cell requires an authored identity", Some(source)))?;
    indent(out, level);
    let start = out.len();
    let mapping_at = mappings.len();
    push_mapping(mappings, declared_id, source, start, start);
    if cell.children.is_empty() {
        let text = cell.text.as_deref().ok_or_else(|| {
            reject(
                "static cell needs authored text or child nodes",
                Some(source),
            )
        })?;
        out.push_str(&text_literal(text));
        let end = out.len();
        mappings[mapping_at].swift_end = end;
        let text_source = text_entry(entries, path)
            .map(|entry| entry.range)
            .ok_or_else(|| reject("text source range unavailable", Some(source)))?;
        push_mapping(mappings, declared_id, text_source, start, end);
        return Ok(());
    }
    out.push_str("VStack(alignment: .leading, spacing: 0) {\n");
    if let Some(text) = &cell.text {
        indent(out, level + 1);
        let text_start = out.len();
        out.push_str(&text_literal(text));
        let text_end = out.len();
        out.push('\n');
        let text_source = text_entry(entries, path)
            .map(|entry| entry.range)
            .ok_or_else(|| reject("text source range unavailable", Some(source)))?;
        push_mapping(mappings, declared_id, text_source, text_start, text_end);
    }
    for (index, child) in cell.children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(index);
        emit_node(out, child, &child_path, entries, level + 1, mappings)?;
        out.push('\n');
    }
    indent(out, level);
    out.push('}');
    mappings[mapping_at].swift_end = out.len();
    Ok(())
}

fn text_literal(value: &str) -> String {
    crate::swift::Expr::Call {
        callee: crate::swift::Identifier::new("Text").expect("fixed identifier"),
        arguments: vec![crate::swift::Argument {
            label: Some(crate::swift::Identifier::new("verbatim").expect("fixed identifier")),
            value: crate::swift::Expr::StringLiteral(value.to_owned()),
        }],
    }
    .render()
}

fn emit_scene(
    scene: &Scene,
    entries: &[SourceEntry],
    owner_swift: &str,
    fields: &[crate::transition_swift::Field],
    mappings: &mut Vec<Mapping>,
) -> Result<(String, Vec<serde_json::Value>), crate::Diagnostic> {
    let mut swift = String::from(owner_swift);
    swift.push_str("\n@MainActor\nstruct NeptuneRootView: View {\n    @StateObject private var store = NeptuneStore()\n    @State private var actionError: String? = nil\n\n    var body: some View {\n        VStack(alignment: .leading, spacing: 0) {\n");
    if is_virtual_root(scene) {
        for (index, child) in scene.root.children.iter().enumerate() {
            emit_node(&mut swift, child, &[index], entries, 3, mappings)?;
            swift.push('\n');
        }
    } else {
        emit_node(&mut swift, &scene.root, &[], entries, 3, mappings)?;
        swift.push('\n');
    }
    let readout = native_swift::append_readout(&mut swift, fields)?;
    swift.push_str("            if let actionError {\n                Text(verbatim: \"Action failed: \" + actionError)\n            }\n        }\n    }\n}\n\n@main\n@MainActor\nstruct NeptuneApp: App {\n    var body: some Scene {\n        WindowGroup {\n            NeptuneRootView()\n        }\n    }\n}\n");
    if swift.len() > MAX_OUTPUT_BYTES {
        return Err(reject("observable column preview exceeds 2 MiB", None));
    }
    Ok((swift, readout))
}

/// Generate a source preview. Apple compilation and iOS admission are not run.
pub fn preview(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
) -> Result<Preview, crate::Diagnostic> {
    crate::validate_module(module)?;
    crate::inspect(source)?;
    let parsed = goya_lang::parse_strict(source).map_err(|error| crate::Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
    })?;
    if let Some(entry) = unsupported_entry(&parsed.entries) {
        let message = match entry.kind.as_str() {
            "property" => format!("observable column rejects property '{}'", entry.name),
            "input" => "observable column rejects inputs".to_owned(),
            _ => format!("observable column rejects {}", entry.kind),
        };
        return Err(reject(&message, Some(entry.range)));
    }
    if parsed.scene.interactions.len() != 1 {
        return Err(reject(
            "observable column requires one D1 interaction",
            parsed
                .entries
                .iter()
                .find(|entry| entry.kind == "interaction")
                .map(|entry| entry.range),
        ));
    }
    let interaction = &parsed.scene.interactions[0];
    let action_range = parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "interaction")
        .map(|entry| entry.range);
    if interaction.class != InteractionClass::D1 || !interaction.inputs.is_empty() {
        return Err(reject(
            "observable column requires an input-free D1 action",
            action_range,
        ));
    }
    let mut count = 0usize;
    let mut found = None;
    if is_virtual_root(&parsed.scene) {
        for (index, child) in parsed.scene.root.children.iter().enumerate() {
            validate_node(child, &[index], 1, &mut count, &parsed.entries, &mut found)?;
        }
    } else {
        validate_node(
            &parsed.scene.root,
            &[],
            1,
            &mut count,
            &parsed.entries,
            &mut found,
        )?;
    }
    let button = found.ok_or_else(|| reject("observable column requires one button", None))?;
    let lowered = goya_lang::lower(&parsed.scene, 1.0, 1.0)
        .map_err(|error| reject(&error.to_string(), None))?;
    ir_matches(&parsed.scene, &lowered, &button)?;
    let owner = crate::application_swift::preview_observable(source, module, initial)?;
    let action = owner
        .actions()
        .first()
        .ok_or_else(|| reject("state owner did not preserve the action", None))?;
    if owner.actions().len() != 1
        || action.trigger.declared_id != button.id
        || !action.inputs.is_empty()
        || action.method != "dispatch0"
    {
        return Err(reject(
            "state-owner action binding differs from the button",
            node_entry(&parsed.entries, &button.path).map(|entry| entry.range),
        ));
    }
    let mut mappings = Vec::new();
    let (swift, readout) = emit_scene(
        &parsed.scene,
        &parsed.entries,
        owner.swift(),
        owner.state_fields(),
        &mut mappings,
    )?;
    let source_range =
        |range: SourceRange| serde_json::json!({"start_byte": range.start, "end_byte": range.end});
    let swift_range = |start, end| serde_json::json!({"start_byte": start, "end_byte": end});
    let mut mapped: Vec<_> = mappings
        .iter()
        .map(|mapping| {
            serde_json::json!({
                "semantic_id": {"module": module, "declared_id": mapping.declared_id},
                "source_range": source_range(mapping.source),
                "swift_range": swift_range(mapping.swift_start, mapping.swift_end)
            })
        })
        .collect();
    mapped.push(serde_json::json!({
        "semantic_id": {"module": module, "declared_id": action.id.declared_id},
        "source_range": source_range(action.source),
        "swift_range": source_range(action.swift)
    }));
    Ok(Preview(serde_json::json!({
        "schema_version": "1",
        "operation": "preview-observable-column",
        "profile": "experimental",
        "mapping": MAPPING,
        "verification": "SOURCE_PREVIEW",
        "scope": "static_text_and_one_button",
        "module": module,
        "node_count": count,
        "source_sha256": hex(&parsed.source_digest),
        "initial_state_sha256": hex(&owner.initial_digest()),
        "initial_hash_contract": "NPT-D1-INIT/1",
        "swift_sha256": format!("{:x}", Sha256::digest(swift.as_bytes())),
        "swift": swift,
        "reference_program_hash": hex(&action.reference_program_hash),
        "source_map": {"schema_version": "2", "mappings": mapped},
        "readout": readout,
        "capabilities": {
            "schema_version": "1",
            "profile": "experimental",
            "mapping": MAPPING,
            "frameworks": ["SwiftUI", "Combine"],
            "minimum_os": null,
            "entitlements": [],
            "usage_description_keys": [],
            "privacy_declarations": "owner_review_required",
            "test_status": "apple_validation_not_run",
            "fallback": "reject",
            "admitted": ["goya.text.static", "goya.button.activate"],
            "layout": "child_order_only"
        },
        "stages": {
            "goya_frontend": "PASS",
            "layout_fidelity": "NOT_CLAIMED",
            "state_readout": "SOURCE_PREVIEW",
            "runtime_limit_parity": "NOT_VERIFIED",
            "accessibility": "NOT_VERIFIED",
            "apple_validation": "NOT_RUN",
            "ios_admission": "NOT_RUN"
        }
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_executable::D1Value;

    const SOURCE: &str = include_str!("../tests/observable-column.gscene");

    fn initial() -> BTreeMap<String, D1Value> {
        BTreeMap::from([("count".into(), D1Value::Int(0))])
    }

    #[test]
    fn column_places_text_before_the_button_and_readout() {
        let first = preview(SOURCE, "app/main.gscene", initial()).unwrap();
        assert_eq!(
            first.to_json(),
            preview(SOURCE, "app/main.gscene", initial())
                .unwrap()
                .to_json()
        );
        let swift = first.swift();
        let heading = swift.find("Text(verbatim: \"Counter\")").unwrap();
        let button = swift.find("Text(verbatim: \"Increment\")").unwrap();
        let value = swift
            .find("Text(verbatim: String(store.state.s0))")
            .unwrap();
        let revision = swift
            .find("Text(verbatim: String(store.state.revision))")
            .unwrap();
        assert!(heading < button && button < value && value < revision);
        assert_eq!(swift.matches("store.dispatch0").count(), 1);
        assert!(!swift.contains(".frame("));
        let body = &swift[swift.find("var body").unwrap()..];
        let readout = &body[body.find("})").unwrap()..body.find("if let actionError").unwrap()];
        assert!(!readout.contains("dispatch"));
        let json = first.to_json();
        assert_eq!(json["node_count"], 2);
        assert_eq!(json["mapping"], MAPPING);
        assert_eq!(json["stages"]["layout_fidelity"], "NOT_CLAIMED");
        assert_eq!(json["stages"]["state_readout"], "SOURCE_PREVIEW");
        assert_eq!(json["stages"]["ios_admission"], "NOT_RUN");
        assert_eq!(json["readout"][0]["source_key"], "count");
        assert_eq!(json["readout"][1]["role"], "revision");
        for mapping in json["source_map"]["mappings"].as_array().unwrap() {
            let source = &mapping["source_range"];
            let generated = &mapping["swift_range"];
            assert!(!SOURCE
                .get(
                    source["start_byte"].as_u64().unwrap() as usize
                        ..source["end_byte"].as_u64().unwrap() as usize
                )
                .unwrap()
                .is_empty());
            assert!(!swift
                .get(
                    generated["start_byte"].as_u64().unwrap() as usize
                        ..generated["end_byte"].as_u64().unwrap() as usize
                )
                .unwrap()
                .is_empty());
        }
        assert!(crate::static_column::preview(SOURCE, "app/main.gscene").is_err());
        assert!(crate::native_swift::preview(SOURCE, "app/main.gscene", initial()).is_err());
        let parsed = goya_lang::parse_strict(SOURCE).unwrap();
        let mut application = goya_sdk::D1Application::new(&parsed.scene, initial()).unwrap();
        assert_eq!(application.values().get("count"), Some(&D1Value::Int(0)));
        assert_eq!(application.revision(), 0);
        application
            .dispatch("action.increment", BTreeMap::new())
            .unwrap();
        assert_eq!(application.values().get("count"), Some(&D1Value::Int(1)));
        assert_eq!(application.revision(), 1);
        assert!(swift.contains("s0: 0"));
    }

    #[test]
    fn sibling_order_and_closed_failures() {
        let siblings = r#"@ontology counter {
  @cell title { id: "title" text: "Counter" }
  @button increment { id: "button.increment" text: "Increment" }
  @interaction increment {
    id: "action.increment";
    trigger: button.increment.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 1;
  }
}
"#;
        let swift = preview(siblings, "app/main.gscene", initial())
            .unwrap()
            .swift()
            .to_owned();
        assert!(
            swift.find("Text(verbatim: \"Counter\")").unwrap()
                < swift.find("Button(action:").unwrap()
        );
        let colored = SOURCE.replace("text: \"Counter\"", "text: \"Counter\" fill: #ffffff");
        let error = preview(&colored, "app/main.gscene", initial()).unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(&colored[range.start..range.end], "fill: #ffffff");
        let blank = SOURCE.replace("text: \"Increment\"", "text: \" \"");
        assert!(preview(&blank, "app/main.gscene", initial()).is_err());
        let extra = SOURCE.replace(
            "@button increment { id: \"button.increment\" text: \"Increment\" }",
            "@button increment { id: \"button.increment\" text: \"Increment\" } @button other { id: \"button.other\" text: \"Other\" }",
        );
        assert!(preview(&extra, "app/main.gscene", initial()).is_err());
        let no_button = r#"@ontology counter { @cell title { id: "title" text: "Counter" } @interaction increment { id: "action.increment"; trigger: button.missing.activate; class: D1; state { write count: int; } input { } set count = 1; } }"#;
        assert!(preview(no_button, "app/main.gscene", initial()).is_err());
        let with_input = SOURCE.replace("input { }", "input { amount: int; }");
        assert!(preview(&with_input, "app/main.gscene", initial()).is_err());
        assert!(preview(SOURCE, "app/main.gscene", BTreeMap::new()).is_err());
        assert_eq!(
            preview(SOURCE, "../app.gscene", initial())
                .unwrap_err()
                .code,
            "NPT-E1006"
        );
    }
}
