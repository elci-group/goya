//! Experimental lowering of a scene's shared D1 state contract.
//! This does not validate views, generate an app, or admit an iOS build.
use std::collections::BTreeMap;
use goya_executable::D1Value;
use goya_lang::{InteractionType, SourceRange};
use crate::transition::Transition;

pub const CONTRACT: &str = "NPT-D1-STATE/1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateCell { pub key: String, pub value_type: InteractionType, pub initial: D1Value }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateModel {
    pub module: String,
    pub source_digest: [u8; 32],
    pub cells: Vec<StateCell>,
    pub actions: Vec<Transition>,
}

fn error(message: impl Into<String>, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic { code: "NPT-E3102", message: message.into(), range }
}

/// Validate explicit initial state using Goya's shared application owner and
/// lower every D1 action. The model covers state only; presentation and iOS
/// platform semantics remain unvalidated.
pub fn lower(source: &str, module: &str, initial: BTreeMap<String, D1Value>) -> Result<StateModel, crate::Diagnostic> {
    let mut actions = crate::transition::lower(source, module)?;
    let parsed = goya_lang::parse_strict(source).map_err(|e| error(e.to_string(), Some(e.range)))?;
    if actions.len() != parsed.scene.interactions.len() { return Err(error("not every interaction was lowered", None)); }
    let owner = goya_sdk::D1Application::new(&parsed.scene, initial).map_err(|e| {
        let range = match &e {
            goya_sdk::D1ApplicationError::ConflictingStateType(key) => parsed.scene.interactions.iter().rev().find(|action| action.state_reads.iter().chain(&action.state_writes).any(|item| item.name == *key)).and_then(|action| parsed.entries.iter().find(|entry| entry.kind == "interaction" && entry.semantic_id == action.authored_id)).map(|entry| entry.range),
            _ => None,
        };
        error(e.to_string(), range)
    })?;
    let mut cells = Vec::with_capacity(owner.schema().len());
    for (key, value_type) in owner.schema() {
        let value = owner.values().get(key).ok_or_else(|| error("validated state value is missing", None))?;
        cells.push(StateCell { key: key.clone(), value_type: *value_type, initial: value.clone() });
    }
    actions.sort_by(|left, right| left.semantic_id.declared_id.cmp(&right.semantic_id.declared_id));
    Ok(StateModel { module: module.to_owned(), source_digest: parsed.source_digest, cells, actions })
}
