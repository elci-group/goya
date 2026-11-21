//! Experimental lowering of a scene's shared D1 state contract.
//! This does not validate views, generate an app, or admit an iOS build.
use crate::transition::Transition;
use goya_executable::D1Value;
use goya_lang::{InteractionType, SourceRange};
use std::collections::BTreeMap;

pub const CONTRACT: &str = "NPT-D1-STATE/1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCell {
    pub key: String,
    pub value_type: InteractionType,
    pub initial: D1Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateModel {
    module: String,
    source_digest: [u8; 32],
    cells: Vec<StateCell>,
    actions: Vec<Transition>,
}

impl StateModel {
    pub fn module(&self) -> &str {
        &self.module
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.source_digest
    }
    pub fn cells(&self) -> &[StateCell] {
        &self.cells
    }
    pub fn actions(&self) -> &[Transition] {
        &self.actions
    }
    /// Machine-readable state-only preview. No presentation or iOS validation
    /// is implied by this result.
    pub fn to_json(&self) -> serde_json::Value {
        fn type_name(ty: InteractionType) -> &'static str {
            match ty {
                InteractionType::Int => "int",
                InteractionType::Text => "text",
                InteractionType::Bool => "bool",
            }
        }
        fn value(value: &D1Value) -> serde_json::Value {
            match value {
                D1Value::Int(v) => serde_json::json!(v),
                D1Value::Text(v) => serde_json::json!(v),
                D1Value::Bool(v) => serde_json::json!(v),
            }
        }
        fn hex(bytes: &[u8]) -> String {
            bytes.iter().map(|byte| format!("{byte:02x}")).collect()
        }
        fn expression(expr: &crate::transition::Expression) -> serde_json::Value {
            use crate::transition::Operation;
            let operation = match &expr.operation {
                Operation::Constant(v) => match v {
                    goya_lang::InteractionValue::Int(v) => {
                        serde_json::json!({"kind":"constant","value":v})
                    }
                    goya_lang::InteractionValue::Text(v) => {
                        serde_json::json!({"kind":"constant","value":v})
                    }
                    goya_lang::InteractionValue::Bool(v) => {
                        serde_json::json!({"kind":"constant","value":v})
                    }
                },
                Operation::State(key) => serde_json::json!({"kind":"state","key":key}),
                Operation::Input(key) => serde_json::json!({"kind":"input","key":key}),
                Operation::CheckedAdd(left, right) => {
                    serde_json::json!({"kind":"checked_add","left":expression(left),"right":expression(right)})
                }
            };
            serde_json::json!({"type":type_name(expr.value_type),"operation":operation})
        }
        fn keys(keys: &[goya_lang::TypedKey]) -> Vec<serde_json::Value> {
            keys.iter()
                .map(|key| serde_json::json!({"key":key.name,"type":type_name(key.value_type)}))
                .collect()
        }
        let cells: Vec<_> = self.cells.iter().map(|cell| serde_json::json!({"key":cell.key,"type":type_name(cell.value_type),"initial":value(&cell.initial)})).collect();
        let actions: Vec<_> = self.actions.iter().map(|action| serde_json::json!({"semantic_id":{"module":action.semantic_id.module,"declared_id":action.semantic_id.declared_id},"source_range":{"start_byte":action.source_range.start,"end_byte":action.source_range.end},"trigger":{"module":action.trigger_id.module,"declared_id":action.trigger_id.declared_id},"trigger_source_range":{"start_byte":action.trigger_source_range.start,"end_byte":action.trigger_source_range.end},"reference_program_hash":hex(&action.reference_program_hash),"state_reads":keys(&action.state_reads),"state_writes":keys(&action.state_writes),"inputs":keys(&action.inputs),"assignments":action.assignments.iter().map(|assignment| serde_json::json!({"key":assignment.key,"value":expression(&assignment.value)})).collect::<Vec<_>>(),"effect":"state_deterministic","capabilities":[],"publication":"atomic_on_success","cancellation":"no_suspension","expression_provenance":"action_range_only"})).collect();
        serde_json::json!({"schema_version":"1","model_contract":CONTRACT,"application_contract":goya_sdk::D1_APPLICATION_CONTRACT,"profile":"experimental","scope":"state_only","module":self.module,"source_sha256":hex(&self.source_digest),"initial_revision":0,"state":cells,"actions":actions,"stages":{"state_validation":"PASS","presentation_validation":"NOT_RUN","apple_validation":"NOT_RUN","ios_admission":"NOT_RUN"}})
    }
}

/// Parse transport data as `[key, type, value]` triples. Duplicate keys,
/// unknown types, implicit defaults and extra tuple elements are rejected.
pub fn parse_initial(
    value: &serde_json::Value,
) -> Result<BTreeMap<String, D1Value>, crate::Diagnostic> {
    let items = value.as_array().ok_or_else(|| {
        error(
            "initial state must be an array of [key, type, value] triples",
            None,
        )
    })?;
    if items.len() > 256 {
        return Err(error("initial state exceeds 256 entries", None));
    }
    let mut result = BTreeMap::new();
    for item in items {
        let tuple = item
            .as_array()
            .filter(|tuple| tuple.len() == 3)
            .ok_or_else(|| {
                error(
                    "initial state entries must have exactly three elements",
                    None,
                )
            })?;
        let key = tuple[0]
            .as_str()
            .filter(|key| !key.is_empty() && key.len() <= 128)
            .ok_or_else(|| error("invalid initial state key", None))?;
        let typed = match tuple[1].as_str() {
            Some("int") => D1Value::Int(
                tuple[2]
                    .as_i64()
                    .ok_or_else(|| error("integer initial value must be Int64", None))?,
            ),
            Some("text") => {
                let text = tuple[2]
                    .as_str()
                    .ok_or_else(|| error("text initial value must be a string", None))?;
                if text.len() > goya_executable::D1_MAX_TEXT_BYTES {
                    return Err(error("text initial value exceeds D1 limit", None));
                }
                D1Value::Text(text.to_owned())
            }
            Some("bool") => D1Value::Bool(
                tuple[2]
                    .as_bool()
                    .ok_or_else(|| error("boolean initial value must be true or false", None))?,
            ),
            _ => return Err(error("unknown initial state type", None)),
        };
        if result.insert(key.to_owned(), typed).is_some() {
            return Err(error(format!("duplicate initial state key {key:?}"), None));
        }
    }
    Ok(result)
}

fn error(message: impl Into<String>, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E3102",
        message: message.into(),
        range,
    }
}

fn conflicting_state_range(parsed: &goya_lang::ParsedScene, key: &str) -> Option<SourceRange> {
    let mut seen = None;
    for action in &parsed.scene.interactions {
        for item in action
            .state_reads
            .iter()
            .chain(&action.state_writes)
            .filter(|item| item.name == key)
        {
            if seen.is_some_and(|ty| ty != item.value_type) {
                return parsed
                    .entries
                    .iter()
                    .find(|entry| {
                        entry.kind == "interaction" && entry.semantic_id == action.authored_id
                    })
                    .map(|entry| entry.range);
            }
            seen = Some(item.value_type);
        }
    }
    None
}

/// Validate explicit initial state using Goya's shared application owner and
/// lower every D1 action. The model covers state only; presentation and iOS
/// platform semantics remain unvalidated.
pub fn lower(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
) -> Result<StateModel, crate::Diagnostic> {
    let mut actions = crate::transition::lower(source, module)?;
    let handoff = goya_lang::handoff::admit_v3(source)
        .map_err(|e| error(e.to_string(), Some(e.range)))?;
    let parsed = handoff.parsed();
    if actions.len() != parsed.scene.interactions.len() {
        return Err(error("not every interaction was lowered", None));
    }
    let owner = goya_sdk::D1Application::new(&parsed.scene, initial).map_err(|e| {
        let range = match &e {
            goya_sdk::D1ApplicationError::ConflictingStateType(key) => {
                conflicting_state_range(&parsed, key)
            }
            _ => None,
        };
        error(e.to_string(), range)
    })?;
    let mut cells = Vec::with_capacity(owner.schema().len());
    for (key, value_type) in owner.schema() {
        let value = owner
            .values()
            .get(key)
            .ok_or_else(|| error("validated state value is missing", None))?;
        cells.push(StateCell {
            key: key.clone(),
            value_type: *value_type,
            initial: value.clone(),
        });
    }
    actions.sort_by(|left, right| {
        left.semantic_id
            .declared_id
            .cmp(&right.semantic_id.declared_id)
    });
    Ok(StateModel {
        module: module.to_owned(),
        source_digest: parsed.source_digest,
        cells,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "@ontology app { @cell root { id: \"root\" @button inc { id: \"button.inc\" } @button rename { id: \"button.rename\" } } @interaction rename { id: \"action.rename\"; trigger: button.rename.activate; class: D1; state { write name: text; } input { value: text; } set name = input.value; } @interaction add { id: \"action.add\"; trigger: button.inc.activate; class: D1; state { read count: int; write count: int; } input { amount: int; } set count = state.count + input.amount; } }";
    fn initial() -> BTreeMap<String, D1Value> {
        BTreeMap::from([
            ("name".into(), D1Value::Text("e\u{301}".into())),
            ("count".into(), D1Value::Int(4)),
        ])
    }

    #[test]
    fn lowers_all_actions_and_explicit_values_deterministically() {
        let model = lower(SOURCE, "app/main.gscene", initial()).unwrap();
        assert_eq!(model, lower(SOURCE, "app/main.gscene", initial()).unwrap());
        assert_eq!(
            model
                .cells
                .iter()
                .map(|cell| cell.key.as_str())
                .collect::<Vec<_>>(),
            ["count", "name"]
        );
        assert_eq!(model.cells[1].initial, D1Value::Text("e\u{301}".into()));
        assert_eq!(
            model
                .actions
                .iter()
                .map(|action| action.semantic_id.declared_id.as_str())
                .collect::<Vec<_>>(),
            ["action.add", "action.rename"]
        );
        assert!(
            SOURCE[model.actions[0].source_range.start..model.actions[0].source_range.end]
                .starts_with("@interaction add")
        );
        assert_eq!(model.actions[0].trigger_id.declared_id, "button.inc");
    }

    #[test]
    fn rejects_incomplete_extra_mistyped_and_incompatible_state() {
        assert_eq!(
            lower(SOURCE, "app/main.gscene", BTreeMap::new())
                .unwrap_err()
                .code,
            "NPT-E3102"
        );
        let mut extra = initial();
        extra.insert("extra".into(), D1Value::Int(0));
        assert!(lower(SOURCE, "app/main.gscene", extra).is_err());
        let mut wrong = initial();
        wrong.insert("count".into(), D1Value::Bool(false));
        assert!(lower(SOURCE, "app/main.gscene", wrong).is_err());
        let conflict = SOURCE
            .replace("write name: text", "write count: text")
            .replace("set name = input.value", "set count = input.value");
        let diagnostic = lower(&conflict, "app/main.gscene", initial()).unwrap_err();
        assert_eq!(diagnostic.code, "NPT-E3102");
        let range = diagnostic.range.unwrap();
        assert!(conflict[range.start..range.end].starts_with("@interaction add"));
        let with_button = conflict.replace(
            "@button inc",
            "@button later { id: \"button.later\" } @button inc",
        );
        let with_later = format!("{} @interaction later {{ id: \"action.later\"; trigger: button.later.activate; class: D1; state {{ write count: int; }} input {{ value: int; }} set count = input.value; }} }}", &with_button[..with_button.len() - 1]);
        let diagnostic = lower(&with_later, "app/main.gscene", initial()).unwrap_err();
        let range = diagnostic.range.unwrap();
        assert!(with_later[range.start..range.end].starts_with("@interaction add"));
        let mut oversized = initial();
        oversized.insert(
            "name".into(),
            D1Value::Text("x".repeat(goya_executable::D1_MAX_TEXT_BYTES + 1)),
        );
        assert!(lower(SOURCE, "app/main.gscene", oversized).is_err());
    }

    #[test]
    fn initial_transport_requires_exact_types_and_unique_keys() {
        let valid = serde_json::json!([
            ["count", "int", -1],
            ["title", "text", "é"],
            ["enabled", "bool", true]
        ]);
        let values = parse_initial(&valid).unwrap();
        assert_eq!(values["count"], D1Value::Int(-1));
        for invalid in [
            serde_json::json!([["a", "int", 1], ["a", "int", 2]]),
            serde_json::json!([["a", "int", true]]),
            serde_json::json!([["a", "int", 1.5]]),
            serde_json::json!([["a", "bool", null]]),
            serde_json::json!([["a", "text", "x", "extra"]]),
            serde_json::json!([["a", "unknown", 0]]),
            serde_json::json!({"a": 1}),
        ] {
            assert!(parse_initial(&invalid).is_err(), "{invalid}");
        }
        assert!(parse_initial(&serde_json::json!([[
            "a",
            "text",
            "x".repeat(goya_executable::D1_MAX_TEXT_BYTES + 1)
        ]]))
        .is_err());
    }

    #[test]
    fn serialized_model_contains_typed_action_bodies_and_explicit_scope() {
        let value = lower(SOURCE, "app/main.gscene", initial())
            .unwrap()
            .to_json();
        assert_eq!(value["scope"], "state_only");
        assert_eq!(value["stages"]["presentation_validation"], "NOT_RUN");
        assert_eq!(
            value["actions"][0]["assignments"][0]["value"]["operation"]["kind"],
            "checked_add"
        );
        assert_eq!(value["actions"][0]["publication"], "atomic_on_success");
        assert_eq!(value["actions"][0]["capabilities"], serde_json::json!([]));
        assert_eq!(
            value["actions"][0]["assignments"][0]["value"]["type"],
            "int"
        );
        assert_eq!(
            value["actions"][1]["assignments"][0]["value"]["operation"]["kind"],
            "input"
        );
    }
}
