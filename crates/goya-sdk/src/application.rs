//! Shared, bounded D1 application state ownership for hosts.
//!
//! This API accepts a scene and an explicit initial state. It does not parse
//! a new source language, implement persistence, or dispatch D2 effects.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use goya_executable::{D1Error, D1Snapshot, D1Value, InputSnapshot};
use goya_lang::{CompiledInteraction, InteractionClass, InteractionType, Scene, TransitionError};

/// Version of the opt-in D1 application state ownership contract.
pub const D1_APPLICATION_CONTRACT: &str = "GY-D1-APP/1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1ApplicationError {
    UnsupportedEffect,
    InvalidInteractions(TransitionError),
    CompilerInvariant,
    MissingActionIdentity,
    DuplicateActionIdentity(String),
    ConflictingStateType(String),
    StateSchemaMismatch,
    InvalidState(D1Error),
    InvalidInput(D1Error),
    UnknownAction(String),
    ActionFailed(TransitionError),
}

impl fmt::Display for D1ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for D1ApplicationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidInteractions(error) | Self::ActionFailed(error) => Some(error),
            Self::InvalidState(error) | Self::InvalidInput(error) => Some(error),
            _ => None,
        }
    }
}

fn matches_type(value: &D1Value, ty: InteractionType) -> bool {
    matches!(
        (value, ty),
        (D1Value::Int(_), InteractionType::Int)
            | (D1Value::Text(_), InteractionType::Text)
            | (D1Value::Bool(_), InteractionType::Bool)
    )
}

/// One authoritative owner of explicitly initialized D1 state.
///
/// The caller must provide every state key used by the application's D1
/// interactions, with no extra keys or manufactured defaults. All actions
/// must agree on each key's type. The whole application state is bounded by
/// the Goya D1 snapshot limits, including after every successful action.
/// Create this from a scene already admitted by the calling frontend; this
/// does not assert that its visual or platform semantics are supported.
pub struct D1Application {
    actions: BTreeMap<String, CompiledInteraction>,
    schema: BTreeMap<String, InteractionType>,
    values: BTreeMap<String, D1Value>,
    revision: u64,
}

impl D1Application {
    pub fn new(
        scene: &Scene,
        initial: BTreeMap<String, D1Value>,
    ) -> Result<Self, D1ApplicationError> {
        Self::restore(scene, 0, initial)
    }

    /// Reconstruct an owner from an explicit snapshot after validating the
    /// current schema and D1 limits. Storage and migration are host duties.
    pub fn restore(
        scene: &Scene,
        revision: u64,
        initial: BTreeMap<String, D1Value>,
    ) -> Result<Self, D1ApplicationError> {
        if scene
            .interactions
            .iter()
            .any(|decl| decl.class != InteractionClass::D1)
        {
            return Err(D1ApplicationError::UnsupportedEffect);
        }
        let compiled = goya_lang::compile_d1_interactions(scene)
            .map_err(D1ApplicationError::InvalidInteractions)?;
        if compiled.len() != scene.interactions.len() {
            return Err(D1ApplicationError::CompilerInvariant);
        }
        let mut actions = BTreeMap::new();
        let mut schema = BTreeMap::new();
        for (decl, action) in scene.interactions.iter().zip(compiled) {
            let id = decl
                .authored_id
                .as_ref()
                .filter(|id| !id.is_empty())
                .ok_or(D1ApplicationError::MissingActionIdentity)?;
            for key in action.state_reads().iter().chain(action.state_writes()) {
                if let Some(existing) = schema.insert(key.name.clone(), key.value_type) {
                    if existing != key.value_type {
                        return Err(D1ApplicationError::ConflictingStateType(key.name.clone()));
                    }
                }
            }
            if actions.insert(id.clone(), action).is_some() {
                return Err(D1ApplicationError::DuplicateActionIdentity(id.clone()));
            }
        }
        if initial.len() != schema.len()
            || schema.iter().any(|(key, ty)| {
                initial
                    .get(key)
                    .is_none_or(|value| !matches_type(value, *ty))
            })
        {
            return Err(D1ApplicationError::StateSchemaMismatch);
        }
        let snapshot = D1Snapshot::new(revision, initial).map_err(D1ApplicationError::InvalidState)?;
        Ok(Self {
            actions,
            schema,
            values: snapshot.values().clone(),
            revision,
        })
    }

    pub fn schema(&self) -> &BTreeMap<String, InteractionType> {
        &self.schema
    }
    pub fn values(&self) -> &BTreeMap<String, D1Value> {
        &self.values
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn action_ids(&self) -> impl Iterator<Item = &str> {
        self.actions.keys().map(String::as_str)
    }

    /// Evaluate against an immutable snapshot; publish all writes together only
    /// when the action and the whole-state budget both succeed.
    pub fn dispatch(
        &mut self,
        id: &str,
        inputs: BTreeMap<String, D1Value>,
    ) -> Result<(), D1ApplicationError> {
        let action = self
            .actions
            .get(id)
            .ok_or_else(|| D1ApplicationError::UnknownAction(id.into()))?;
        let touched: BTreeSet<_> = action
            .state_reads()
            .iter()
            .chain(action.state_writes())
            .map(|key| key.name.as_str())
            .collect();
        let mut values = BTreeMap::new();
        for key in touched {
            values.insert(
                key.to_owned(),
                self.values
                    .get(key)
                    .cloned()
                    .ok_or(D1ApplicationError::StateSchemaMismatch)?,
            );
        }
        let snapshot =
            D1Snapshot::new(self.revision, values).map_err(D1ApplicationError::InvalidState)?;
        let input = InputSnapshot::new(inputs).map_err(D1ApplicationError::InvalidInput)?;
        let outcome = action
            .execute(&snapshot, &input)
            .map_err(D1ApplicationError::ActionFailed)?;
        let mut next = self.values.clone();
        for key in action.state_writes() {
            let value = outcome
                .next_state
                .get(&key.name)
                .cloned()
                .ok_or(D1ApplicationError::StateSchemaMismatch)?;
            if !matches_type(&value, key.value_type) {
                return Err(D1ApplicationError::StateSchemaMismatch);
            }
            next.insert(key.name.clone(), value);
        }
        let snapshot = D1Snapshot::new(outcome.revision_after, next).map_err(D1ApplicationError::InvalidState)?;
        self.values = snapshot.values().clone();
        self.revision = outcome.revision_after;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "@ontology app { @cell root { id: \"root\" @button inc { id: \"button.inc\" } @button rename { id: \"button.rename\" } } @interaction add { id: \"action.add\"; trigger: button.inc.activate; class: D1; state { read count: int; write count: int; } input { amount: int; } set count = state.count + input.amount; } @interaction rename { id: \"action.rename\"; trigger: button.rename.activate; class: D1; state { write name: text; } input { value: text; } set name = input.value; } }";

    fn scene() -> Scene {
        goya_lang::parse_strict(SOURCE).unwrap().scene
    }
    fn initial(count: i64) -> BTreeMap<String, D1Value> {
        BTreeMap::from([
            ("count".into(), D1Value::Int(count)),
            ("name".into(), D1Value::Text("before".into())),
        ])
    }

    #[test]
    fn actions_share_one_state_and_preserve_unrelated_cells() {
        let mut app = D1Application::new(&scene(), initial(4)).unwrap();
        assert_eq!(
            app.action_ids().collect::<Vec<_>>(),
            ["action.add", "action.rename"]
        );
        app.dispatch(
            "action.add",
            BTreeMap::from([("amount".into(), D1Value::Int(3))]),
        )
        .unwrap();
        assert_eq!(app.revision(), 1);
        assert_eq!(app.values()["count"], D1Value::Int(7));
        assert_eq!(app.values()["name"], D1Value::Text("before".into()));
        app.dispatch(
            "action.rename",
            BTreeMap::from([("value".into(), D1Value::Text("e\u{301}".into()))]),
        )
        .unwrap();
        assert_eq!(app.revision(), 2);
        assert_eq!(app.values()["count"], D1Value::Int(7));
        assert_eq!(app.values()["name"], D1Value::Text("e\u{301}".into()));
    }

    #[test]
    fn invalid_initial_values_and_failed_actions_do_not_create_defaults_or_commit() {
        assert!(matches!(
            D1Application::new(&scene(), BTreeMap::new()),
            Err(D1ApplicationError::StateSchemaMismatch)
        ));
        let mut extra = initial(0);
        extra.insert("extra".into(), D1Value::Bool(true));
        assert!(matches!(
            D1Application::new(&scene(), extra),
            Err(D1ApplicationError::StateSchemaMismatch)
        ));
        let mut app = D1Application::new(&scene(), initial(i64::MAX)).unwrap();
        let before = app.values().clone();
        assert!(matches!(
            app.dispatch(
                "action.add",
                BTreeMap::from([("amount".into(), D1Value::Int(1))])
            ),
            Err(D1ApplicationError::ActionFailed(
                TransitionError::VmRejected(_)
            ))
        ));
        assert!(matches!(
            app.dispatch(
                "action.add",
                BTreeMap::from([("amount".into(), D1Value::Bool(true))])
            ),
            Err(D1ApplicationError::ActionFailed(
                TransitionError::InputTypeMismatch
            ))
        ));
        assert!(matches!(
            app.dispatch("unknown", BTreeMap::new()),
            Err(D1ApplicationError::UnknownAction(_))
        ));
        assert_eq!(app.values(), &before);
        assert_eq!(app.revision(), 0);
    }

    #[test]
    fn restore_validates_the_complete_snapshot_and_preserves_revision() {
        let mut app = D1Application::restore(&scene(), 41, initial(5)).unwrap();
        app.dispatch(
            "action.add",
            BTreeMap::from([("amount".into(), D1Value::Int(2))]),
        )
        .unwrap();
        assert_eq!(app.revision(), 42);
        assert_eq!(app.values()["count"], D1Value::Int(7));
        assert!(matches!(
            D1Application::restore(&scene(), 8, BTreeMap::new()),
            Err(D1ApplicationError::StateSchemaMismatch)
        ));
        let mut exhausted = D1Application::restore(&scene(), u64::MAX, initial(5)).unwrap();
        let before = exhausted.values().clone();
        assert!(matches!(
            exhausted.dispatch(
                "action.add",
                BTreeMap::from([("amount".into(), D1Value::Int(2))])
            ),
            Err(D1ApplicationError::ActionFailed(
                TransitionError::VmRejected(_)
            ))
        ));
        assert_eq!(exhausted.values(), &before);
        assert_eq!(exhausted.revision(), u64::MAX);
    }

    #[test]
    fn incompatible_action_schemas_are_rejected() {
        let source = SOURCE
            .replace("write name: text", "write count: text")
            .replace("set name = input.value", "set count = input.value");
        let scene = goya_lang::parse_strict(&source).unwrap().scene;
        assert!(
            matches!(D1Application::new(&scene, initial(0)), Err(D1ApplicationError::ConflictingStateType(key)) if key == "count")
        );
    }

    #[test]
    fn action_identities_and_effects_fail_closed() {
        let missing = SOURCE.replace("id: \"action.add\";", "");
        assert!(matches!(
            D1Application::new(&goya_lang::parse(&missing).unwrap(), initial(0)),
            Err(D1ApplicationError::MissingActionIdentity)
        ));
        let duplicate = SOURCE.replace("id: \"action.rename\";", "id: \"action.add\";");
        assert!(matches!(
            D1Application::new(&goya_lang::parse(&duplicate).unwrap(), initial(0)),
            Err(D1ApplicationError::DuplicateActionIdentity(_))
        ));
        let d2 = "@ontology app { @button go { id: \"button.go\" } @interaction fetch { id: \"action.fetch\"; trigger: button.go.activate; class: D2; effects { network; } requests { lookup: effect network capability \"network\" request \"static request\" result int64; } state { write count: int; } input { } set count = effect.lookup; } }";
        let scene = goya_lang::parse_strict(d2).unwrap().scene;
        assert!(matches!(
            D1Application::new(&scene, BTreeMap::new()),
            Err(D1ApplicationError::UnsupportedEffect)
        ));
    }

    #[test]
    fn global_state_budget_rejects_an_otherwise_valid_action_atomically() {
        let mut buttons = String::new();
        let mut interactions = String::new();
        let mut initial = BTreeMap::new();
        for index in 0..18 {
            let key = if index == 17 {
                "target".to_string()
            } else {
                format!("key{index}")
            };
            buttons.push_str(&format!("@button b{index} {{ id: \"button.{index}\" }} "));
            interactions.push_str(&format!("@interaction a{index} {{ id: \"action.{index}\"; trigger: button.b{index}.activate; class: D1; state {{ write {key}: text; }} input {{ value: text; }} set {key} = input.value; }} "));
            initial.insert(
                key,
                D1Value::Text(if index == 17 {
                    String::new()
                } else {
                    "x".repeat(60000)
                }),
            );
        }
        let source =
            format!("@ontology app {{ @cell root {{ id: \"root\" {buttons} }} {interactions} }}");
        let scene = goya_lang::parse_strict(&source).unwrap().scene;
        let mut app = D1Application::new(&scene, initial).unwrap();
        assert!(matches!(
            app.dispatch(
                "action.17",
                BTreeMap::from([("value".into(), D1Value::Text("y".repeat(60000)))])
            ),
            Err(D1ApplicationError::InvalidState(_))
        ));
        assert_eq!(app.values()["target"], D1Value::Text(String::new()));
        assert_eq!(app.revision(), 0);
    }
}
