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
    pub module: String,
    pub source_digest: [u8; 32],
    pub cells: Vec<StateCell>,
    pub actions: Vec<Transition>,
}

fn error(message: impl Into<String>, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E3102",
        message: message.into(),
        range,
    }
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
    let parsed =
        goya_lang::parse_strict(source).map_err(|e| error(e.to_string(), Some(e.range)))?;
    if actions.len() != parsed.scene.interactions.len() {
        return Err(error("not every interaction was lowered", None));
    }
    let owner = goya_sdk::D1Application::new(&parsed.scene, initial).map_err(|e| {
        let range = match &e {
            goya_sdk::D1ApplicationError::ConflictingStateType(key) => parsed
                .scene
                .interactions
                .iter()
                .rev()
                .find(|action| {
                    action
                        .state_reads
                        .iter()
                        .chain(&action.state_writes)
                        .any(|item| item.name == *key)
                })
                .and_then(|action| {
                    parsed.entries.iter().find(|entry| {
                        entry.kind == "interaction" && entry.semantic_id == action.authored_id
                    })
                })
                .map(|entry| entry.range),
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
        assert!(diagnostic.range.is_some());
        let mut oversized = initial();
        oversized.insert(
            "name".into(),
            D1Value::Text("x".repeat(goya_executable::D1_MAX_TEXT_BYTES + 1)),
        );
        assert!(lower(SOURCE, "app/main.gscene", oversized).is_err());
    }
}
