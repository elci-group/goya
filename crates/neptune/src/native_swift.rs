//! Experimental, source-only native preview for one input-free D1 button.
use crate::swift::{Argument, Expr, Identifier};
use goya_executable::D1Value;
use goya_lang::{CellKind, InteractionClass, InteractionType, SourceRange};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

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
fn reject(message: &str, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E2104",
        message: message.into(),
        range,
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn verbatim(value: &str) -> String {
    Expr::Call {
        callee: Identifier::new("Text").expect("fixed identifier"),
        arguments: vec![Argument {
            label: Some(Identifier::new("verbatim").expect("fixed identifier")),
            value: Expr::StringLiteral(value.to_owned()),
        }],
    }
    .render()
}

fn value_text(name: &str, value_type: InteractionType) -> Result<String, crate::Diagnostic> {
    if Identifier::new(name).is_none() {
        return Err(reject("state field name is not a Swift identifier", None));
    }
    Ok(match value_type {
        InteractionType::Int => format!("Text(verbatim: String(store.state.{name}))"),
        InteractionType::Text => format!("Text(verbatim: store.state.{name})"),
        InteractionType::Bool => {
            format!("Text(verbatim: store.state.{name} ? \"true\" : \"false\")")
        }
    })
}

fn push_readout(swift: &mut String, expression: &str) -> (usize, usize) {
    swift.push_str("            ");
    let start = swift.len();
    swift.push_str(expression);
    let end = swift.len();
    swift.push('\n');
    (start, end)
}

pub(crate) fn append_readout(
    swift: &mut String,
    fields: &[crate::transition_swift::Field],
) -> Result<Vec<serde_json::Value>, crate::Diagnostic> {
    let mut rows = Vec::new();
    for field in fields {
        push_readout(swift, &verbatim(&field.source_key));
        let expression = value_text(&field.swift_name, field.value_type)?;
        let (start, end) = push_readout(swift, &expression);
        rows.push(serde_json::json!({
            "role": "state",
            "source_key": field.source_key,
            "swift_name": field.swift_name,
            "type": match field.value_type {
                InteractionType::Int => "int",
                InteractionType::Text => "text",
                InteractionType::Bool => "bool",
            },
            "encoding": match field.value_type {
                InteractionType::Int => "decimal",
                InteractionType::Text => "utf8",
                InteractionType::Bool => "true-false",
            },
            "swift_range": {"start_byte": start, "end_byte": end}
        }));
    }
    push_readout(swift, &verbatim("revision"));
    let (start, end) = push_readout(swift, "Text(verbatim: String(store.state.revision))");
    rows.push(serde_json::json!({
        "role": "revision",
        "source_key": "revision",
        "swift_name": "revision",
        "type": "uint64",
        "encoding": "decimal",
        "swift_range": {"start_byte": start, "end_byte": end}
    }));
    Ok(rows)
}

/// Generate a source preview, without Apple compilation or iOS admission.
pub fn preview(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
) -> Result<Preview, crate::Diagnostic> {
    render(source, module, initial, false)
}

/// Same button admission as [`preview`], with a live readout of admitted state.
///
/// State cells render in UTF-8 key order, then the revision. The view reads
/// the published state and dispatches only from the button action. Apple
/// compilation and iOS admission are not run.
pub fn preview_observable(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
) -> Result<Preview, crate::Diagnostic> {
    render(source, module, initial, true)
}

fn render(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
    readout: bool,
) -> Result<Preview, crate::Diagnostic> {
    crate::validate_module(module)?;
    crate::inspect(source)?;
    let handoff = goya_lang::handoff::admit_v3(source)
        .map_err(|e| reject(&e.to_string(), Some(e.range)))?;
    let parsed = handoff.parsed();
    let root = &parsed.scene.root;
    let button = parsed
        .entries
        .iter()
        .find(|e| e.kind == "button" && e.scene_path.as_deref() == Some(&[]));
    let text_entry = parsed
        .entries
        .iter()
        .find(|e| e.kind == "property" && e.name == "text" && e.scene_path.as_deref() == Some(&[]));
    let unsupported = parsed.entries.iter().find(|e| match e.kind.as_str() {
        "ontology" => false,
        "button" => e.scene_path.as_deref() != Some(&[]),
        "property" => {
            e.scene_path.as_deref() != Some(&[]) || !matches!(e.name.as_str(), "id" | "text")
        }
        "interaction" => false,
        _ => true,
    });
    if root.kind != CellKind::Button
        || !root.children.is_empty()
        || parsed.scene.interactions.len() != 1
        || parsed.entries.len() != 5
        || button.is_none()
        || text_entry.is_none()
        || unsupported.is_some()
    {
        return Err(reject(
            "native preview requires one root button with only id and text, and one D1 interaction",
            unsupported.or(button).map(|e| e.range),
        ));
    }
    let button = button.expect("validated");
    let text_entry = text_entry.expect("validated");
    let label = root
        .text
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            reject(
                "button needs a nonempty accessible label",
                Some(text_entry.range),
            )
        })?;
    let id = root
        .authored_id
        .as_ref()
        .ok_or_else(|| reject("button requires an authored identity", Some(button.range)))?;
    let interaction = &parsed.scene.interactions[0];
    let action_range = parsed
        .entries
        .iter()
        .find(|e| e.kind == "interaction")
        .map(|e| e.range);
    if interaction.class != InteractionClass::D1 || !interaction.inputs.is_empty() {
        return Err(reject(
            "native button preview requires an input-free D1 action",
            action_range,
        ));
    }
    let ir = goya_lang::lower(&parsed.scene, 1.0, 1.0)
        .map_err(|e| reject(&e.to_string(), Some(button.range)))?;
    let reference = goya_lang::compile_d1_interactions(&parsed.scene)
        .map_err(|e| reject(&e.to_string(), action_range))?;
    let bound = matches!(
        ir.module.root.and_then(|r| ir.module.node(r)),
        Some(goya_ir::Node::Button {
            label: actual,
            on_activate,
        }) if actual == label && reference.len() == 1 && *on_activate == reference[0].trigger()
    );
    if !bound {
        return Err(reject(
            "shared IR button or action binding differs from source",
            Some(button.range),
        ));
    }
    let owner = crate::application_swift::preview_observable(source, module, initial)?;
    if owner.actions().len() != 1
        || owner.actions()[0].trigger.declared_id != *id
        || !owner.actions()[0].inputs.is_empty()
        || owner.actions()[0].method != "dispatch0"
    {
        return Err(reject(
            "state-owner action binding differs from button",
            Some(button.range),
        ));
    }
    let action = &owner.actions()[0];
    let mut swift = owner.swift().to_owned();
    swift.push_str("\n@MainActor\nstruct NeptuneRootView: View {\n    @StateObject private var store = NeptuneStore()\n    @State private var actionError: String? = nil\n\n    var body: some View {\n        VStack {\n");
    let button_start = swift.len();
    swift.push_str("            Button(action: {\n                do {\n                    try store.dispatch0(input: NeptuneInput0())\n                    actionError = nil\n                } catch {\n                    actionError = String(describing: error)\n                }\n            }, label: {\n                ");
    let text_start = swift.len();
    swift.push_str(&verbatim(label));
    let text_end = swift.len();
    swift.push_str("\n            })\n");
    if !readout {
        swift.push_str("            .frame(maxWidth: .infinity, maxHeight: .infinity)\n");
    }
    let button_end = swift.len();
    let readout_rows = if readout {
        append_readout(&mut swift, owner.state_fields())?
    } else {
        Vec::new()
    };
    swift.push_str("            if let actionError {\n                Text(verbatim: \"Action failed: \" + actionError)\n            }\n        }\n    }\n}\n\n@main\n@MainActor\nstruct NeptuneApp: App {\n    var body: some Scene {\n        WindowGroup {\n            NeptuneRootView()\n        }\n    }\n}\n");
    if swift.len() > 2 * 1024 * 1024 {
        return Err(reject("native preview exceeds 2 MiB", None));
    }
    let source_range = |r: SourceRange| serde_json::json!({"start_byte":r.start,"end_byte":r.end});
    let swift_range = |start, end| serde_json::json!({"start_byte":start,"end_byte":end});
    let semantic =
        |declared_id: &str| serde_json::json!({"module":module,"declared_id":declared_id});
    let mut result = serde_json::json!({
        "schema_version":"1", "operation": if readout { "preview-observable-button" } else { "preview-native-button" }, "profile":"experimental", "verification":"SOURCE_PREVIEW", "scope": if readout { "single_button_readout" } else { "single_button" },
        "module":module, "source_sha256":hex(&parsed.source_digest), "initial_state_sha256":hex(&owner.initial_digest()), "initial_hash_contract":"NPT-D1-INIT/1",
        "swift_sha256":format!("{:x}", Sha256::digest(swift.as_bytes())), "swift":swift,
        "reference_program_hash":hex(&action.reference_program_hash),
        "source_map":{"schema_version":"2","mappings":[
            {"semantic_id":semantic(id),"source_range":source_range(button.range),"swift_range":swift_range(button_start,button_end)},
            {"semantic_id":semantic(id),"source_range":source_range(text_entry.range),"swift_range":swift_range(text_start,text_end)},
            {"semantic_id":semantic(&action.id.declared_id),"source_range":source_range(action.source),"swift_range":source_range(action.swift)}
        ]},
        "capabilities":{"schema_version":"1","profile":"experimental","frameworks":["SwiftUI","Combine"],"minimum_os":null,"entitlements":[],"usage_description_keys":[],"privacy_declarations":"owner_review_required","test_status":"apple_validation_not_run","fallback":"reject"},
        "stages":{"goya_frontend":"PASS","runtime_limit_parity":"NOT_VERIFIED","apple_validation":"NOT_RUN","ios_admission":"NOT_RUN"}
    });
    if readout {
        result["readout"] = serde_json::Value::Array(readout_rows);
        result["stages"]["state_readout"] = serde_json::json!("SOURCE_PREVIEW");
        result["stages"]["accessibility"] = serde_json::json!("NOT_VERIFIED");
    }
    Ok(Preview(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = include_str!("../tests/native-button.gscene");
    fn initial() -> BTreeMap<String, D1Value> {
        BTreeMap::from([("count".into(), D1Value::Int(0))])
    }
    #[test]
    fn native_button_maps_action_and_label_deterministically() {
        let first = preview(SOURCE, "app/main.gscene", initial()).unwrap();
        assert_eq!(
            first.to_json(),
            preview(SOURCE, "app/main.gscene", initial())
                .unwrap()
                .to_json()
        );
        let swift = first.swift();
        assert!(swift.contains("@Published private(set) var state"));
        assert!(swift.contains("@StateObject private var store = NeptuneStore()"));
        assert!(swift.contains("try store.dispatch0(input: NeptuneInput0())"));
        assert!(swift.contains("Text(verbatim: \"Increment\")"));
        assert!(swift.contains("Text(verbatim: \"Action failed: \" + actionError)"));
        assert!(swift.contains(".frame(maxWidth: .infinity, maxHeight: .infinity)"));
        assert!(!swift.contains("store.state"));
        assert!(!swift.contains("try?"));
        let json = first.to_json();
        assert_eq!(json["stages"]["apple_validation"], "NOT_RUN");
        for mapping in json["source_map"]["mappings"].as_array().unwrap() {
            let source = &mapping["source_range"];
            let generated = &mapping["swift_range"];
            assert!(SOURCE
                .get(
                    source["start_byte"].as_u64().unwrap() as usize
                        ..source["end_byte"].as_u64().unwrap() as usize
                )
                .is_some());
            assert!(swift
                .get(
                    generated["start_byte"].as_u64().unwrap() as usize
                        ..generated["end_byte"].as_u64().unwrap() as usize
                )
                .is_some());
        }
        let escaped = SOURCE.replace("Increment", "é\\(unsafe)");
        assert!(preview(&escaped, "app/main.gscene", initial())
            .unwrap()
            .swift()
            .contains("é\\\\(unsafe)"));
    }
    #[test]
    fn unsupported_presentation_and_inputs_fail_closed() {
        for source in [
            SOURCE.replace("text: \"Increment\"", "text: \" \""),
            SOURCE.replace("text: \"Increment\"", "text: \"Increment\" fill: #ffffff"),
            SOURCE.replace("input { }", "input { amount: int; }"),
            SOURCE.replace("@button increment", "@input increment"),
        ] {
            assert!(
                preview(&source, "app/main.gscene", initial()).is_err(),
                "{source}"
            );
            assert!(
                preview_observable(&source, "app/main.gscene", initial()).is_err(),
                "{source}"
            );
        }
        assert!(preview(SOURCE, "app/main.gscene", BTreeMap::new()).is_err());
        let colored = SOURCE.replace("text: \"Increment\"", "text: \"Increment\" fill: #ffffff");
        let error = preview(&colored, "app/main.gscene", initial()).unwrap_err();
        let range = error.range.unwrap();
        assert_eq!(&colored[range.start..range.end], "fill: #ffffff");
    }
    #[test]
    fn observable_readout_follows_key_order_and_dispatches_only_from_the_button() {
        let source = r#"@ontology counter {
  @button increment { id: "button.increment" text: "Increment" }
  @interaction increment {
    id: "action.increment";
    trigger: button.increment.activate;
    class: D1;
    state { read count: int; write count: int; write label: text; write ready: bool; }
    input { }
    set count = state.count + 1;
    set label = "ok";
    set ready = true;
  }
}
"#;
        let values = BTreeMap::from([
            ("count".into(), D1Value::Int(0)),
            ("label".into(), D1Value::Text("é".into())),
            ("ready".into(), D1Value::Bool(false)),
        ]);
        let first = preview_observable(source, "app/main.gscene", values.clone()).unwrap();
        assert_eq!(
            first.to_json(),
            preview_observable(source, "app/main.gscene", values.clone())
                .unwrap()
                .to_json()
        );
        let swift = first.swift();
        assert!(!swift.contains(".frame("));
        assert_eq!(swift.matches("store.dispatch0").count(), 1);
        let count = swift
            .find("Text(verbatim: String(store.state.s0))")
            .unwrap();
        let label = swift.find("Text(verbatim: store.state.s1)").unwrap();
        let ready = swift
            .find("Text(verbatim: store.state.s2 ? \"true\" : \"false\")")
            .unwrap();
        let revision = swift
            .find("Text(verbatim: String(store.state.revision))")
            .unwrap();
        assert!(count < label && label < ready && ready < revision);
        let body = &swift[swift.find("var body").unwrap()..];
        let readout = &body[body.find("})").unwrap()..body.find("if let actionError").unwrap()];
        assert!(!readout.contains("dispatch"));
        let json = first.to_json();
        assert_eq!(json["operation"], "preview-observable-button");
        assert_eq!(json["scope"], "single_button_readout");
        assert_eq!(json["stages"]["state_readout"], "SOURCE_PREVIEW");
        assert_eq!(json["stages"]["apple_validation"], "NOT_RUN");
        assert_eq!(json["stages"]["ios_admission"], "NOT_RUN");
        let rows = json["readout"].as_array().unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row["source_key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["count", "label", "ready", "revision"]
        );
        assert_eq!(rows[1]["encoding"], "utf8");
        assert_eq!(rows[2]["encoding"], "true-false");
        assert_eq!(rows[3]["encoding"], "decimal");
        for row in rows {
            let generated = &row["swift_range"];
            let slice = swift
                .get(
                    generated["start_byte"].as_u64().unwrap() as usize
                        ..generated["end_byte"].as_u64().unwrap() as usize,
                )
                .unwrap();
            assert!(slice.contains("store.state"));
        }
        let plain = preview(source, "app/main.gscene", values).unwrap();
        assert!(!plain.swift().contains("store.state"));
        assert_eq!(plain.to_json()["scope"], "single_button");
        assert!(plain.to_json().get("readout").is_none());
    }
}
