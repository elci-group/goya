//! Host traces for the observable button source previews.
//!
//! Activations run on `goya_sdk::D1Application`. A failed action keeps the
//! previous state and revision. The report names the generated button method
//! for each action. Swift is not executed.

use goya_executable::D1Value;
use goya_lang::{CellKind, SceneCell, TransitionError};
use goya_sdk::D1ApplicationError;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_ACTIVATIONS: usize = 32;

#[derive(Debug)]
pub struct Trace(serde_json::Value);

impl Trace {
    pub fn to_json(&self) -> serde_json::Value {
        self.0.clone()
    }
}

fn reject(message: &str) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E2104",
        message: message.into(),
        range: None,
    }
}

/// Parse an activation list. Each entry is an action id. Membership in the
/// admitted scene is checked by [`trace`].
pub fn parse_activations(value: &serde_json::Value) -> Result<Vec<String>, crate::Diagnostic> {
    let items = value
        .as_array()
        .ok_or_else(|| reject("activations must be an array of action ids"))?;
    if items.len() > MAX_ACTIVATIONS {
        return Err(reject("observable trace supports at most 32 activations"));
    }
    let mut actions = Vec::with_capacity(items.len());
    for item in items {
        let action = item
            .as_str()
            .filter(|action| !action.is_empty() && action.len() <= 128)
            .ok_or_else(|| reject("each activation must be an action id"))?;
        actions.push(action.to_owned());
    }
    Ok(actions)
}

fn host_error_class(error: &D1ApplicationError) -> &'static str {
    match error {
        D1ApplicationError::ActionFailed(TransitionError::VmRejected(message)) => {
            match message.as_str() {
                "integer addition overflow" => "arithmetic_overflow",
                "state revision counter exhausted" => "revision_overflow",
                "text state value exceeds the byte limit" => "text_too_large",
                "snapshot exceeds the byte limit" => "snapshot_too_large",
                "D1 registers exceed the byte limit" => "register_limit",
                _ => "action_failed",
            }
        }
        _ => "action_failed",
    }
}

fn state_json(values: &BTreeMap<String, D1Value>) -> Vec<serde_json::Value> {
    values
        .iter()
        .map(|(key, value)| {
            let (value_type, encoded) = match value {
                D1Value::Int(number) => ("int", serde_json::json!(number)),
                D1Value::Text(text) => ("text", serde_json::json!(text)),
                D1Value::Bool(flag) => ("bool", serde_json::json!(flag)),
            };
            serde_json::json!({"key": key, "type": value_type, "value": encoded})
        })
        .collect()
}

struct ActionBinding {
    method: String,
    trigger: String,
}

fn action_table(
    preview: &serde_json::Value,
) -> Result<BTreeMap<String, ActionBinding>, crate::Diagnostic> {
    let actions = preview["actions"]
        .as_array()
        .ok_or_else(|| reject("button preview did not preserve the action table"))?;
    let mut table = BTreeMap::new();
    for action in actions {
        let id = action["declared_id"]
            .as_str()
            .ok_or_else(|| reject("button preview did not preserve the action table"))?;
        let method = action["method"]
            .as_str()
            .ok_or_else(|| reject("button preview did not preserve the action table"))?;
        let trigger = action["trigger"]
            .as_str()
            .ok_or_else(|| reject("button preview did not preserve the action table"))?;
        if table
            .insert(
                id.to_owned(),
                ActionBinding {
                    method: method.to_owned(),
                    trigger: trigger.to_owned(),
                },
            )
            .is_some()
        {
            return Err(reject("button preview repeated an action id"));
        }
    }
    if table.is_empty() {
        return Err(reject("button preview did not preserve the action table"));
    }
    Ok(table)
}

fn swift_dispatches_match(
    swift: &str,
    table: &BTreeMap<String, ActionBinding>,
) -> Result<(), crate::Diagnostic> {
    for binding in table.values() {
        let call = format!("try store.{}(", binding.method);
        if swift.matches(&call).count() != 1 {
            return Err(reject(
                "generated button dispatch does not match the action table",
            ));
        }
    }
    let body = swift
        .find("var body")
        .map(|start| &swift[start..])
        .ok_or_else(|| reject("generated button view has no body"))?;
    let readout = body
        .rfind("})")
        .ok_or_else(|| reject("generated button view has no button"))?;
    let error_at = body
        .find("if let actionError")
        .ok_or_else(|| reject("generated button view has no action error"))?;
    if readout > error_at || body[readout..error_at].contains("store.dispatch") {
        return Err(reject("generated readout dispatches a button action"));
    }
    Ok(())
}

fn sole_button(cell: &SceneCell) -> Result<&SceneCell, crate::Diagnostic> {
    fn walk<'a>(
        cell: &'a SceneCell,
        found: &mut Option<&'a SceneCell>,
    ) -> Result<(), crate::Diagnostic> {
        if cell.kind == CellKind::Button {
            if found.is_some() {
                return Err(reject("column preview did not preserve one button"));
            }
            *found = Some(cell);
            return Ok(());
        }
        for child in &cell.children {
            walk(child, found)?;
        }
        Ok(())
    }
    let mut found = None;
    walk(cell, &mut found)?;
    found.ok_or_else(|| reject("column preview did not preserve one button"))
}

fn column_action(
    source: &str,
    preview: &serde_json::Value,
) -> Result<(BTreeMap<String, ActionBinding>, serde_json::Value), crate::Diagnostic> {
    let handoff = crate::admit_ios(source).map_err(|error| crate::Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
    })?;
    let parsed = handoff.parsed();
    if parsed.scene.interactions.len() != 1 {
        return Err(reject("column preview did not preserve the action program"));
    }
    let interaction = &parsed.scene.interactions[0];
    let declared_id = interaction
        .authored_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| reject("column preview did not preserve the action program"))?;
    let button = sole_button(&parsed.scene.root)?;
    if button.name != interaction.trigger_button {
        return Err(reject("column preview action is not bound to its button"));
    }
    let trigger = button
        .authored_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| reject("column preview action is not bound to its button"))?;
    let program = preview["reference_program_hash"]
        .as_str()
        .filter(|hash| hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| reject("column preview did not preserve the action program"))?;
    let mut table = BTreeMap::new();
    table.insert(
        declared_id.to_owned(),
        ActionBinding {
            method: "dispatch0".to_owned(),
            trigger: trigger.to_owned(),
        },
    );
    let actions = serde_json::json!([{
        "declared_id": declared_id,
        "method": "dispatch0",
        "trigger": trigger,
        "reference_program_hash": program
    }]);
    Ok((table, actions))
}

struct TraceRequest<'a> {
    operation: &'a str,
    scope: &'a str,
    module: &'a str,
    source: &'a str,
    initial: BTreeMap<String, D1Value>,
    activations: &'a [String],
    preview: &'a serde_json::Value,
    table: &'a BTreeMap<String, ActionBinding>,
    actions: serde_json::Value,
}

fn finish(request: TraceRequest<'_>) -> Result<Trace, crate::Diagnostic> {
    let TraceRequest {
        operation,
        scope,
        module,
        source,
        initial,
        activations,
        preview,
        table,
        actions,
    } = request;
    let swift = preview["swift"]
        .as_str()
        .ok_or_else(|| reject("button preview did not preserve its Swift source"))?;
    swift_dispatches_match(swift, table)?;
    let swift_sha = format!("{:x}", Sha256::digest(swift.as_bytes()));
    if preview["swift_sha256"].as_str() != Some(swift_sha.as_str()) {
        return Err(reject(
            "button preview Swift hash does not match its source",
        ));
    }
    for action in activations {
        if !table.contains_key(action) {
            return Err(reject(&format!(
                "activation {action:?} is not an admitted action"
            )));
        }
    }
    let handoff = crate::admit_ios(source).map_err(|error| crate::Diagnostic {
        code: error.code,
        message: error.to_string(),
        range: (error.range.end > error.range.start).then_some(error.range),
    })?;
    let parsed = handoff.parsed();
    let mut application = goya_sdk::D1Application::new(&parsed.scene, initial)
        .map_err(|error| reject(&format!("host owner rejected the admitted scene: {error}")))?;
    let mut steps = vec![serde_json::json!({
        "index": 0,
        "kind": "initial",
        "status": "PASS",
        "revision": application.revision(),
        "state": state_json(application.values())
    })];
    for (offset, action) in activations.iter().enumerate() {
        let binding = &table[action];
        let prior_state = application.values().clone();
        let prior_revision = application.revision();
        match application.dispatch(action, BTreeMap::new()) {
            Ok(()) => steps.push(serde_json::json!({
                "index": offset + 1,
                "kind": "activation",
                "action": action,
                "trigger": binding.trigger,
                "method": binding.method,
                "status": "PASS",
                "revision": application.revision(),
                "state": state_json(application.values())
            })),
            Err(error) => {
                if application.values() != &prior_state || application.revision() != prior_revision
                {
                    return Err(reject("failed action published a new state"));
                }
                steps.push(serde_json::json!({
                    "index": offset + 1,
                    "kind": "activation",
                    "action": action,
                    "trigger": binding.trigger,
                    "method": binding.method,
                    "status": "FAIL",
                    "error_class": host_error_class(&error),
                    "revision": prior_revision,
                    "state": state_json(&prior_state)
                }));
            }
        }
    }
    Ok(Trace(serde_json::json!({
        "schema_version": "1",
        "operation": operation,
        "profile": "experimental",
        "verification": "HOST_TRACE",
        "scope": scope,
        "module": module,
        "preview_operation": preview["operation"],
        "preview_mapping": preview["mapping"],
        "preview_swift_sha256": swift_sha,
        "application_contract": goya_sdk::D1_APPLICATION_CONTRACT,
        "activation_count": activations.len(),
        "actions": actions,
        "steps": steps,
        "stages": {
            "host_trace": "PASS",
            "swift_execution": "NOT_RUN",
            "layout_fidelity": "NOT_CLAIMED",
            "apple_validation": "NOT_RUN",
            "ios_admission": "NOT_RUN"
        }
    })))
}

/// Run the host activation trace for two or more buttons. Swift execution,
/// Apple compilation, and iOS admission are not run.
pub fn trace(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
    activations: &[String],
) -> Result<Trace, crate::Diagnostic> {
    let preview = crate::observable_buttons::preview(source, module, initial.clone())?;
    let preview = preview.to_json();
    let table = action_table(&preview)?;
    let actions = preview["actions"].clone();
    finish(TraceRequest {
        operation: "trace-observable-buttons",
        scope: "button_activation_trace",
        module,
        source,
        initial,
        activations,
        preview: &preview,
        table: &table,
        actions,
    })
}

/// Run the host activation trace for exactly one button. Swift execution,
/// Apple compilation, and iOS admission are not run.
pub fn trace_column(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
    activations: &[String],
) -> Result<Trace, crate::Diagnostic> {
    let preview = crate::observable_column::preview(source, module, initial.clone())?;
    let preview = preview.to_json();
    let (table, actions) = column_action(source, &preview)?;
    finish(TraceRequest {
        operation: "trace-observable-column",
        scope: "single_button_activation_trace",
        module,
        source,
        initial,
        activations,
        preview: &preview,
        table: &table,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = include_str!("../tests/observable-buttons.gscene");

    fn initial() -> BTreeMap<String, D1Value> {
        BTreeMap::from([
            ("count".into(), D1Value::Int(0)),
            ("label".into(), D1Value::Text("start".into())),
        ])
    }

    #[test]
    fn activations_follow_action_identity_and_are_deterministic() {
        let activations = ["action.up", "action.mark", "action.up"]
            .map(str::to_owned)
            .to_vec();
        let first = trace(SOURCE, "app/main.gscene", initial(), &activations).unwrap();
        assert_eq!(
            first.to_json(),
            trace(SOURCE, "app/main.gscene", initial(), &activations)
                .unwrap()
                .to_json()
        );
        let json = first.to_json();
        assert_eq!(json["verification"], "HOST_TRACE");
        assert_eq!(json["scope"], "button_activation_trace");
        assert_eq!(json["preview_operation"], "preview-observable-buttons");
        assert_eq!(json["activation_count"], 3);
        assert_eq!(json["stages"]["host_trace"], "PASS");
        assert_eq!(json["stages"]["swift_execution"], "NOT_RUN");
        assert_eq!(json["stages"]["ios_admission"], "NOT_RUN");
        let preview = crate::observable_buttons::preview(SOURCE, "app/main.gscene", initial())
            .unwrap()
            .to_json();
        assert_eq!(json["preview_swift_sha256"], preview["swift_sha256"]);
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0]["kind"], "initial");
        assert_eq!(steps[0]["revision"], 0);
        assert_eq!(steps[0]["state"][0]["value"], 0);
        assert_eq!(steps[1]["action"], "action.up");
        assert_eq!(steps[1]["method"], "dispatch1");
        assert_eq!(steps[1]["trigger"], "button.up");
        assert_eq!(steps[1]["status"], "PASS");
        assert_eq!(steps[1]["revision"], 1);
        assert_eq!(steps[1]["state"][0]["key"], "count");
        assert_eq!(steps[1]["state"][0]["value"], 1);
        assert_eq!(steps[1]["state"][1]["value"], "start");
        assert_eq!(steps[2]["action"], "action.mark");
        assert_eq!(steps[2]["method"], "dispatch0");
        assert_eq!(steps[2]["status"], "PASS");
        assert_eq!(steps[2]["state"][1]["value"], "ok");
        assert_eq!(steps[2]["revision"], 2);
        assert_eq!(steps[3]["status"], "PASS");
        assert_eq!(steps[3]["revision"], 3);
        assert_eq!(steps[3]["state"][0]["value"], 2);
        assert_eq!(steps[3]["state"][1]["value"], "ok");
        let empty = trace(SOURCE, "app/main.gscene", initial(), &[]).unwrap();
        assert_eq!(empty.to_json()["steps"].as_array().unwrap().len(), 1);
        assert_eq!(empty.to_json()["steps"][0]["revision"], 0);
    }

    #[test]
    fn failed_activation_keeps_the_previous_state() {
        let source = r#"@ontology counter {
  @state { count: int = 0 session; label: text = "start" session; }
  @cell screen {
    id: "screen"
    text: "Counter"
    @button up { id: "button.up" text: "Up" }
    @button save { id: "button.save" text: "Save" }
  }
  @interaction save {
    id: "action.save";
    trigger: button.save.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 0;
  }
  @interaction up {
    id: "action.up";
    trigger: button.up.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 1;
  }
}
"#;
        let full = BTreeMap::from([("count".into(), D1Value::Int(i64::MAX))]);
        let activations = ["action.save", "action.up", "action.save"]
            .map(str::to_owned)
            .to_vec();
        let json = trace(source, "app/main.gscene", full, &activations)
            .unwrap()
            .to_json();
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps[1]["method"], "dispatch0");
        assert_eq!(steps[1]["status"], "PASS");
        assert_eq!(steps[1]["revision"], 1);
        assert_eq!(steps[1]["state"][0]["value"], i64::MAX);
        assert_eq!(steps[2]["action"], "action.up");
        assert_eq!(steps[2]["method"], "dispatch1");
        assert_eq!(steps[2]["status"], "FAIL");
        assert_eq!(steps[2]["error_class"], "arithmetic_overflow");
        assert_eq!(steps[2]["revision"], 1);
        assert_eq!(steps[2]["state"][0]["value"], i64::MAX);
        assert_eq!(steps[3]["status"], "PASS");
        assert_eq!(steps[3]["revision"], 2);
        assert_eq!(steps[3]["state"][0]["value"], i64::MAX);
        assert!(trace(
            source,
            "app/main.gscene",
            BTreeMap::from([("count".into(), D1Value::Int(i64::MAX))]),
            &["action.missing".into()]
        )
        .is_err());
        let repeated = vec![serde_json::json!("action.up"); 33];
        assert!(parse_activations(&serde_json::Value::Array(repeated)).is_err());
        assert!(parse_activations(&serde_json::json!([""])).is_err());
        assert!(trace(
            include_str!("../tests/observable-column.gscene"),
            "app/main.gscene",
            BTreeMap::from([("count".into(), D1Value::Int(0))]),
            &[]
        )
        .is_err());
        assert_eq!(
            trace(SOURCE, "../app.gscene", initial(), &[])
                .unwrap_err()
                .code,
            "NPT-E1006"
        );
    }

    #[test]
    fn column_activations_follow_the_single_button() {
        let source = include_str!("../tests/observable-column.gscene");
        let state = BTreeMap::from([("count".into(), D1Value::Int(0))]);
        let activations = ["action.increment", "action.increment"]
            .map(str::to_owned)
            .to_vec();
        let first = trace_column(source, "app/main.gscene", state.clone(), &activations).unwrap();
        assert_eq!(
            first.to_json(),
            trace_column(source, "app/main.gscene", state.clone(), &activations)
                .unwrap()
                .to_json()
        );
        let json = first.to_json();
        assert_eq!(json["operation"], "trace-observable-column");
        assert_eq!(json["verification"], "HOST_TRACE");
        assert_eq!(json["scope"], "single_button_activation_trace");
        assert_eq!(json["preview_operation"], "preview-observable-column");
        assert_eq!(json["activation_count"], 2);
        assert_eq!(json["stages"]["host_trace"], "PASS");
        assert_eq!(json["stages"]["swift_execution"], "NOT_RUN");
        assert_eq!(json["stages"]["ios_admission"], "NOT_RUN");
        let preview = crate::observable_column::preview(source, "app/main.gscene", state.clone())
            .unwrap()
            .to_json();
        assert_eq!(json["preview_swift_sha256"], preview["swift_sha256"]);
        assert_eq!(json["actions"][0]["declared_id"], "action.increment");
        assert_eq!(json["actions"][0]["method"], "dispatch0");
        assert_eq!(json["actions"][0]["trigger"], "button.increment");
        assert_eq!(
            json["actions"][0]["reference_program_hash"],
            preview["reference_program_hash"]
        );
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0]["kind"], "initial");
        assert_eq!(steps[0]["revision"], 0);
        assert_eq!(steps[1]["action"], "action.increment");
        assert_eq!(steps[1]["method"], "dispatch0");
        assert_eq!(steps[1]["trigger"], "button.increment");
        assert_eq!(steps[1]["status"], "PASS");
        assert_eq!(steps[1]["revision"], 1);
        assert_eq!(steps[1]["state"][0]["value"], 1);
        assert_eq!(steps[2]["status"], "PASS");
        assert_eq!(steps[2]["method"], "dispatch0");
        assert_eq!(steps[2]["revision"], 2);
        assert_eq!(steps[2]["state"][0]["key"], "count");
        assert_eq!(steps[2]["state"][0]["value"], 2);
        let empty = trace_column(source, "app/main.gscene", state, &[]).unwrap();
        assert_eq!(empty.to_json()["steps"].as_array().unwrap().len(), 1);
        assert_eq!(empty.to_json()["steps"][0]["revision"], 0);
    }

    #[test]
    fn column_overflow_keeps_the_previous_state() {
        let source = r#"@ontology counter {
  @state { count: int = 0 session; }
  @cell screen {
    id: "screen"
    text: "Counter"
    @button up { id: "button.up" text: "Up" }
  }
  @interaction up {
    id: "action.up";
    trigger: button.up.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 1;
  }
}
"#;
        let full = BTreeMap::from([("count".into(), D1Value::Int(i64::MAX))]);
        let activations = ["action.up", "action.up"].map(str::to_owned).to_vec();
        let json = trace_column(source, "app/main.gscene", full, &activations)
            .unwrap()
            .to_json();
        let steps = json["steps"].as_array().unwrap();
        assert_eq!(steps[1]["method"], "dispatch0");
        assert_eq!(steps[1]["status"], "FAIL");
        assert_eq!(steps[1]["error_class"], "arithmetic_overflow");
        assert_eq!(steps[1]["revision"], 0);
        assert_eq!(steps[1]["state"][0]["value"], i64::MAX);
        assert_eq!(steps[2]["method"], "dispatch0");
        assert_eq!(steps[2]["status"], "FAIL");
        assert_eq!(steps[2]["error_class"], "arithmetic_overflow");
        assert_eq!(steps[2]["revision"], 0);
        assert_eq!(steps[2]["state"][0]["value"], i64::MAX);
        assert!(trace_column(
            include_str!("../tests/observable-buttons.gscene"),
            "app/main.gscene",
            BTreeMap::from([
                ("count".into(), D1Value::Int(0)),
                ("label".into(), D1Value::Text("start".into())),
            ]),
            &[]
        )
        .is_err());
        assert!(trace_column(
            include_str!("../tests/observable-column.gscene"),
            "app/main.gscene",
            BTreeMap::from([("count".into(), D1Value::Int(0))]),
            &["action.missing".into()]
        )
        .is_err());
        assert_eq!(
            trace_column(
                include_str!("../tests/observable-column.gscene"),
                "../app.gscene",
                BTreeMap::from([("count".into(), D1Value::Int(0))]),
                &[]
            )
            .unwrap_err()
            .code,
            "NPT-E1006"
        );
    }
}
