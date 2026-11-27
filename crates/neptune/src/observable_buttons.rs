//! Experimental source preview for static text plus two or more buttons.
//!
//! Each button dispatches the input-free D1 action bound to its authored id.
//! Action methods follow authored action-id order, which can differ from
//! child order. The state readout is not an authored cell. Layout fidelity,
//! accessibility, Apple compilation, and iOS admission are not claimed.

use crate::native_swift;
use goya_executable::D1Value;
use goya_ir::{ActionId, Node};
use goya_lang::{
    CellKind, InteractionClass, Lowered, Role, Scene, SceneCell, SourceEntry, SourceRange,
};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAPPING: &str = "neptune.swiftui.observable-buttons/0.0.1";
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
    name: String,
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
        // The GSSH/2 `@state` block the interactions read and write.
        "state" | "state-key" => false,
        // Rendered as native accessibility modifiers (decision D14).
        "iden" => false,
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
    buttons: &mut Vec<ButtonFound>,
) -> Result<(), crate::Diagnostic> {
    let range = node_entry(entries, path).map(|entry| entry.range);
    if depth > MAX_DEPTH {
        return Err(reject(
            "observable buttons support at most 16 nested nodes",
            range,
        ));
    }
    *count += 1;
    if *count > MAX_NODES {
        return Err(reject("observable buttons support at most 32 nodes", range));
    }
    if cell.is_input || cell.kind == CellKind::Input {
        return Err(reject("observable buttons reject inputs", range));
    }
    if let Some(name) = unexpected_visual(cell) {
        return Err(reject(
            &format!("observable buttons reject property '{name}'"),
            range,
        ));
    }
    if cell.kind == CellKind::Button {
        if !cell.children.is_empty() {
            return Err(reject("observable buttons cannot contain children", range));
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
        buttons.push(ButtonFound {
            id: id.to_owned(),
            name: cell.name.clone(),
            label: label.clone(),
            path: path.to_vec(),
        });
        return Ok(());
    }
    if cell.kind != CellKind::Cell {
        return Err(reject("observable buttons admit cells and buttons", range));
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
        validate_node(child, &child_path, depth + 1, count, entries, buttons)?;
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
    actions: &mut Vec<ActionId>,
) -> bool {
    match lowered.module.node(id) {
        Some(Node::Text { content }) => {
            out.push(Item::Text(content.clone()));
            true
        }
        Some(Node::Button { label, on_activate }) => {
            actions.push(*on_activate);
            out.push(Item::Button(label.clone()));
            true
        }
        Some(Node::Column { children }) => children
            .iter()
            .all(|child| collect_ir(lowered, *child, out, actions)),
        _ => false,
    }
}

fn ir_matches(
    scene: &Scene,
    lowered: &Lowered,
    buttons: &[ButtonFound],
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
    let mut actions = Vec::new();
    let root = lowered
        .module
        .root
        .ok_or_else(|| reject("shared IR order differs from the scene", None))?;
    if !collect_ir(lowered, root, &mut actual, &mut actions) || actual != expected {
        return Err(reject("shared IR order differs from the scene", None));
    }
    if actions.len() != buttons.len() {
        return Err(reject("shared IR did not preserve the buttons", None));
    }
    let mut seen = BTreeSet::new();
    if actions.iter().any(|action| !seen.insert(action.0)) {
        return Err(reject("shared IR button actions are ambiguous", None));
    }
    let reference = goya_lang::compile_d1_interactions(scene)
        .map_err(|error| reject(&error.to_string(), None))?;
    if reference.len() != scene.interactions.len() {
        return Err(reject(
            "shared IR button action differs from the D1 interaction",
            None,
        ));
    }
    for (decl, compiled) in scene.interactions.iter().zip(reference) {
        let Some(index) = buttons
            .iter()
            .position(|button| button.name == decl.trigger_button)
        else {
            return Err(reject("action trigger is not an admitted button", None));
        };
        if actions[index] != compiled.trigger() {
            return Err(reject(
                "shared IR button action differs from the D1 interaction",
                None,
            ));
        }
    }
    for button in buttons {
        let preserved = lowered.table.values().any(|record| {
            record.role == Role::Button
                && record.semantic_id.as_deref() == Some(button.id.as_str())
                && matches!(
                    lowered.module.node(record.ir_node),
                    Some(Node::Button { label, .. }) if label == &button.label
                )
        });
        if !preserved {
            return Err(reject("shared IR did not preserve a button identity", None));
        }
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

/// Append `@iden` as native SwiftUI accessibility modifiers, mapped to the
/// declaration (decision D14).
fn emit_iden(
    out: &mut String,
    cell: &SceneCell,
    path: &[usize],
    entries: &[SourceEntry],
    declared_id: &str,
    mappings: &mut Vec<Mapping>,
) {
    let Some(iden) = &cell.iden else { return };
    let start = out.len();
    let container = cell.kind == CellKind::Cell && !cell.children.is_empty();
    out.push_str(&crate::swift::accessibility_modifiers(iden, container));
    if let Some(source) = crate::swift::iden_range(entries, path) {
        push_mapping(mappings, declared_id, source, start, out.len());
    }
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

fn dispatch_statement(method: &str) -> Result<String, crate::Diagnostic> {
    let Some(index) = method.strip_prefix("dispatch") else {
        return Err(reject(
            "state-owner action binding differs from the buttons",
            None,
        ));
    };
    let plain_index = !index.is_empty()
        && index.bytes().all(|byte| byte.is_ascii_digit())
        && (index.len() == 1 || !index.starts_with('0'));
    let input = format!("NeptuneInput{index}");
    if !plain_index
        || crate::swift::Identifier::new(method).is_none()
        || crate::swift::Identifier::new(&input).is_none()
    {
        return Err(reject(
            "state-owner action binding differs from the buttons",
            None,
        ));
    }
    Ok(format!("try store.{method}(input: {input}())\n"))
}

fn emit_button(
    out: &mut String,
    button: &SceneCell,
    path: &[usize],
    entries: &[SourceEntry],
    level: usize,
    bindings: &[(String, String)],
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
    let method = bindings
        .iter()
        .find(|(id, _)| id == declared_id)
        .map(|(_, method)| method.as_str())
        .ok_or_else(|| {
            reject(
                "state-owner action binding differs from the buttons",
                Some(source),
            )
        })?;
    indent(out, level);
    let start = out.len();
    let mapping_at = mappings.len();
    push_mapping(mappings, declared_id, source, start, start);
    out.push_str("Button(action: {\n");
    indent(out, level + 1);
    out.push_str("do {\n");
    indent(out, level + 2);
    out.push_str(&dispatch_statement(method)?);
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
    out.push_str(&text_literal(label));
    let text_end = out.len();
    out.push('\n');
    indent(out, level);
    out.push_str("})");
    emit_iden(out, button, path, entries, declared_id, mappings);
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
    bindings: &[(String, String)],
    mappings: &mut Vec<Mapping>,
) -> Result<(), crate::Diagnostic> {
    if cell.kind == CellKind::Button {
        return emit_button(out, cell, path, entries, level, bindings, mappings);
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
        emit_iden(out, cell, path, entries, declared_id, mappings);
        mappings[mapping_at].swift_end = out.len();
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
        emit_node(
            out,
            child,
            &child_path,
            entries,
            level + 1,
            bindings,
            mappings,
        )?;
        out.push('\n');
    }
    indent(out, level);
    out.push('}');
    emit_iden(out, cell, path, entries, declared_id, mappings);
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
    bindings: &[(String, String)],
    mappings: &mut Vec<Mapping>,
) -> Result<(String, Vec<serde_json::Value>), crate::Diagnostic> {
    let mut swift = String::from(owner_swift);
    swift.push_str("\n@MainActor\nstruct NeptuneRootView: View {\n    @StateObject private var store = NeptuneStore()\n    @State private var actionError: String? = nil\n\n    var body: some View {\n        VStack(alignment: .leading, spacing: 0) {\n");
    if is_virtual_root(scene) {
        for (index, child) in scene.root.children.iter().enumerate() {
            emit_node(&mut swift, child, &[index], entries, 3, bindings, mappings)?;
            swift.push('\n');
        }
    } else {
        emit_node(&mut swift, &scene.root, &[], entries, 3, bindings, mappings)?;
        swift.push('\n');
    }
    let readout = native_swift::append_readout(&mut swift, fields)?;
    swift.push_str("            if let actionError {\n                Text(verbatim: \"Action failed: \" + actionError)\n            }\n        }\n    }\n}\n\n@main\n@MainActor\nstruct NeptuneApp: App {\n    var body: some Scene {\n        WindowGroup {\n            NeptuneRootView()\n        }\n    }\n}\n");
    if swift.len() > MAX_OUTPUT_BYTES {
        return Err(reject("observable buttons preview exceeds 2 MiB", None));
    }
    Ok((swift, readout))
}

fn bind_actions(
    buttons: &[ButtonFound],
    owner: &crate::application_swift::Preview,
    entries: &[SourceEntry],
) -> Result<Vec<(String, String)>, crate::Diagnostic> {
    if owner.actions().len() != buttons.len() {
        return Err(reject(
            "state-owner action binding differs from the buttons",
            None,
        ));
    }
    for (index, action) in owner.actions().iter().enumerate() {
        if !action.inputs.is_empty() || action.method != format!("dispatch{index}") {
            return Err(reject(
                "state-owner action binding differs from the buttons",
                Some(action.source),
            ));
        }
    }
    let mut bindings = Vec::with_capacity(buttons.len());
    let mut seen = BTreeSet::new();
    for button in buttons {
        let action = owner
            .actions()
            .iter()
            .find(|action| action.trigger.declared_id == button.id)
            .ok_or_else(|| {
                reject(
                    "state-owner action binding differs from the buttons",
                    node_entry(entries, &button.path).map(|entry| entry.range),
                )
            })?;
        if !seen.insert(action.method.clone()) {
            return Err(reject(
                "state-owner action binding differs from the buttons",
                node_entry(entries, &button.path).map(|entry| entry.range),
            ));
        }
        bindings.push((button.id.clone(), action.method.clone()));
    }
    if seen.len() != owner.actions().len() {
        return Err(reject(
            "state-owner action binding differs from the buttons",
            None,
        ));
    }
    Ok(bindings)
}

/// Generate a source preview. Apple compilation and iOS admission are not run.
pub fn preview(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
) -> Result<Preview, crate::Diagnostic> {
    crate::validate_module(module)?;
    crate::inspect(source)?;
    let handoff = crate::admit_ios(source).map_err(|error| crate::Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
    })?;
    let parsed = handoff.parsed();
    if let Some(entry) = unsupported_entry(&parsed.entries) {
        let message = match entry.kind.as_str() {
            "property" => format!("observable buttons reject property '{}'", entry.name),
            "input" => "observable buttons reject inputs".to_owned(),
            _ => format!("observable buttons reject {}", entry.kind),
        };
        return Err(reject(&message, Some(entry.range)));
    }
    for interaction in &parsed.scene.interactions {
        if interaction.class != InteractionClass::D1 || !interaction.inputs.is_empty() {
            let range = parsed
                .entries
                .iter()
                .find(|entry| {
                    entry.kind == "interaction" && entry.semantic_id == interaction.authored_id
                })
                .map(|entry| entry.range);
            return Err(reject(
                "observable buttons require input-free D1 actions",
                range,
            ));
        }
    }
    let mut count = 0usize;
    let mut buttons = Vec::new();
    if is_virtual_root(&parsed.scene) {
        for (index, child) in parsed.scene.root.children.iter().enumerate() {
            validate_node(
                child,
                &[index],
                1,
                &mut count,
                &parsed.entries,
                &mut buttons,
            )?;
        }
    } else {
        validate_node(
            &parsed.scene.root,
            &[],
            1,
            &mut count,
            &parsed.entries,
            &mut buttons,
        )?;
    }
    if buttons.len() < 2 {
        return Err(reject(
            "observable buttons require at least two buttons",
            None,
        ));
    }
    if parsed.scene.interactions.len() != buttons.len() {
        return Err(reject(
            "observable buttons require one input-free D1 action per button",
            None,
        ));
    }
    let lowered = goya_lang::lower(&parsed.scene, 1.0, 1.0)
        .map_err(|error| reject(&error.to_string(), None))?;
    ir_matches(&parsed.scene, &lowered, &buttons)?;
    let owner = crate::application_swift::preview_observable(source, module, initial)?;
    let bindings = bind_actions(&buttons, &owner, &parsed.entries)?;
    let mut mappings = Vec::new();
    let (swift, readout) = emit_scene(
        &parsed.scene,
        &parsed.entries,
        owner.swift(),
        owner.state_fields(),
        &bindings,
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
    let mut actions = Vec::new();
    for action in owner.actions() {
        mapped.push(serde_json::json!({
            "semantic_id": {"module": module, "declared_id": action.id.declared_id},
            "source_range": source_range(action.source),
            "swift_range": source_range(action.swift)
        }));
        actions.push(serde_json::json!({
            "declared_id": action.id.declared_id,
            "method": action.method,
            "trigger": action.trigger.declared_id,
            "reference_program_hash": hex(&action.reference_program_hash)
        }));
    }
    Ok(Preview(serde_json::json!({
        "schema_version": "1",
        "operation": "preview-observable-buttons",
        "profile": "experimental",
        "mapping": MAPPING,
        "verification": "SOURCE_PREVIEW",
        "scope": "static_text_and_buttons",
        "module": module,
        "node_count": count,
        "button_count": buttons.len(),
        "source_sha256": hex(&parsed.source_digest),
        "initial_state_sha256": hex(&owner.initial_digest()),
        "initial_hash_contract": "NPT-D1-INIT/1",
        "swift_sha256": format!("{:x}", Sha256::digest(swift.as_bytes())),
        "swift": swift,
        "actions": actions,
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

    const SOURCE: &str = include_str!("../tests/observable-buttons.gscene");

    fn initial() -> BTreeMap<String, D1Value> {
        BTreeMap::from([
            ("count".into(), D1Value::Int(0)),
            ("label".into(), D1Value::Text("start".into())),
        ])
    }

    #[test]
    fn each_button_calls_its_own_action_before_the_readout() {
        let first = preview(SOURCE, "app/main.gscene", initial()).unwrap();
        assert_eq!(
            first.to_json(),
            preview(SOURCE, "app/main.gscene", initial())
                .unwrap()
                .to_json()
        );
        let swift = first.swift();
        let heading = swift.find("Text(verbatim: \"Counter\")").unwrap();
        let up = swift.find("Text(verbatim: \"Up\")").unwrap();
        let mark = swift.find("Text(verbatim: \"Mark\")").unwrap();
        let count = swift
            .find("Text(verbatim: String(store.state.s0))")
            .unwrap();
        let label = swift.find("Text(verbatim: store.state.s1)").unwrap();
        let revision = swift
            .find("Text(verbatim: String(store.state.revision))")
            .unwrap();
        assert!(heading < up && up < mark && mark < count && count < label && label < revision);
        let dispatch_up = swift.find("store.dispatch1(").unwrap();
        let dispatch_mark = swift.find("store.dispatch0(").unwrap();
        assert!(dispatch_up < up && up < dispatch_mark && dispatch_mark < mark);
        assert_eq!(swift.matches("try store.dispatch").count(), 2);
        assert!(!swift.contains(".frame("));
        assert!(!swift[count..].contains("dispatch"));
        let json = first.to_json();
        assert_eq!(json["node_count"], 3);
        assert_eq!(json["button_count"], 2);
        assert_eq!(json["mapping"], MAPPING);
        assert_eq!(json["actions"][0]["declared_id"], "action.mark");
        assert_eq!(json["actions"][0]["method"], "dispatch0");
        assert_eq!(json["actions"][0]["trigger"], "button.mark");
        assert_eq!(json["actions"][1]["declared_id"], "action.up");
        assert_eq!(json["actions"][1]["method"], "dispatch1");
        assert_eq!(json["actions"][1]["trigger"], "button.up");
        let mark_hash = json["actions"][0]["reference_program_hash"]
            .as_str()
            .unwrap();
        let up_hash = json["actions"][1]["reference_program_hash"]
            .as_str()
            .unwrap();
        assert_eq!(mark_hash.len(), 64);
        assert_ne!(mark_hash, up_hash);
        assert_eq!(json["stages"]["layout_fidelity"], "NOT_CLAIMED");
        assert_eq!(json["stages"]["state_readout"], "SOURCE_PREVIEW");
        assert_eq!(json["stages"]["ios_admission"], "NOT_RUN");
        assert_eq!(json["readout"][0]["source_key"], "count");
        assert_eq!(json["readout"][1]["source_key"], "label");
        assert_eq!(json["readout"][2]["role"], "revision");
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
        assert!(
            crate::native_swift::preview_observable(SOURCE, "app/main.gscene", initial()).is_err()
        );
        assert!(crate::observable_column::preview(SOURCE, "app/main.gscene", initial()).is_err());
        let parsed = goya_lang::parse_strict_gssh2(SOURCE).unwrap();
        let mut application = goya_sdk::D1Application::new(&parsed.scene, initial()).unwrap();
        application.dispatch("action.up", BTreeMap::new()).unwrap();
        assert_eq!(application.values().get("count"), Some(&D1Value::Int(1)));
        assert_eq!(
            application.values().get("label"),
            Some(&D1Value::Text("start".into()))
        );
        assert_eq!(application.revision(), 1);
        application
            .dispatch("action.mark", BTreeMap::new())
            .unwrap();
        assert_eq!(application.values().get("count"), Some(&D1Value::Int(1)));
        assert_eq!(
            application.values().get("label"),
            Some(&D1Value::Text("ok".into()))
        );
        assert_eq!(application.revision(), 2);
        assert!(swift.contains("s0: 0"));
        assert!(swift.contains("s1: \"start\""));
    }

    #[test]
    fn sibling_buttons_keep_identity_binding_and_closed_failures() {
        let siblings = r#"@ontology counter {
  @state { count: int = 0 session; label: text = "start" session; }
  @button up { id: "button.up" text: "Up" }
  @button mark { id: "button.mark" text: "Mark" }
  @interaction up {
    id: "action.up";
    trigger: button.up.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 1;
  }
  @interaction mark {
    id: "action.mark";
    trigger: button.mark.activate;
    class: D1;
    state { write label: text; }
    input { }
    set label = "ok";
  }
}
"#;
        let swift = preview(siblings, "app/main.gscene", initial())
            .unwrap()
            .swift()
            .to_owned();
        let up = swift.find("Text(verbatim: \"Up\")").unwrap();
        let mark = swift.find("Text(verbatim: \"Mark\")").unwrap();
        assert!(swift.find("store.dispatch1(").unwrap() < up);
        assert!(up < swift.find("store.dispatch0(").unwrap());
        assert!(swift.find("store.dispatch0(").unwrap() < mark);
        let colored = SOURCE.replace("text: \"Counter\"", "text: \"Counter\" fill: #ffffff");
        let error = preview(&colored, "app/main.gscene", initial()).unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(&colored[range.start..range.end], "fill: #ffffff");
        let blank = SOURCE.replace("text: \"Up\"", "text: \" \"");
        assert!(preview(&blank, "app/main.gscene", initial()).is_err());
        assert!(preview(
            include_str!("../tests/observable-column.gscene"),
            "app/main.gscene",
            initial()
        )
        .is_err());
        let with_input = SOURCE.replacen("input { }", "input { amount: int; }", 1);
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
