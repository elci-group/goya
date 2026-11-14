//! Shared, bounded D1 application state ownership for hosts.
//!
//! This API accepts a scene and an explicit initial state. It does not parse
//! a new source language, implement persistence, or dispatch D2 effects.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use goya_lang::{CompiledInteraction, InteractionClass, InteractionType, Scene};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1ApplicationError {
    UnsupportedEffect,
    InvalidInteractions(String),
    MissingActionIdentity,
    DuplicateActionIdentity(String),
    ConflictingStateType(String),
    StateSchemaMismatch,
    InvalidState(String),
    InvalidInput(String),
    UnknownAction(String),
    ActionFailed(String),
}

impl fmt::Display for D1ApplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for D1ApplicationError {}

fn matches_type(value: &D1Value, ty: InteractionType) -> bool {
    matches!((value, ty), (D1Value::Int(_), InteractionType::Int) | (D1Value::Text(_), InteractionType::Text) | (D1Value::Bool(_), InteractionType::Bool))
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
    pub fn new(scene: &Scene, initial: BTreeMap<String, D1Value>) -> Result<Self, D1ApplicationError> {
        if scene.interactions.iter().any(|decl| decl.class != InteractionClass::D1) {
            return Err(D1ApplicationError::UnsupportedEffect);
        }
        let compiled = goya_lang::compile_d1_interactions(scene).map_err(|error| D1ApplicationError::InvalidInteractions(error.to_string()))?;
        if compiled.len() != scene.interactions.len() { return Err(D1ApplicationError::InvalidInteractions("interaction count mismatch".into())); }
        let mut actions = BTreeMap::new();
        let mut schema = BTreeMap::new();
        for (decl, action) in scene.interactions.iter().zip(compiled) {
            let id = decl.authored_id.as_ref().filter(|id| !id.is_empty()).ok_or(D1ApplicationError::MissingActionIdentity)?;
            for key in action.state_reads().iter().chain(action.state_writes()) {
                if let Some(existing) = schema.insert(key.name.clone(), key.value_type) {
                    if existing != key.value_type { return Err(D1ApplicationError::ConflictingStateType(key.name.clone())); }
                }
            }
            if actions.insert(id.clone(), action).is_some() { return Err(D1ApplicationError::DuplicateActionIdentity(id.clone())); }
        }
        if initial.len() != schema.len() || schema.iter().any(|(key, ty)| initial.get(key).is_none_or(|value| !matches_type(value, *ty))) {
            return Err(D1ApplicationError::StateSchemaMismatch);
        }
        D1Snapshot::new(0, initial.clone()).map_err(|error| D1ApplicationError::InvalidState(error.to_string()))?;
        Ok(Self { actions, schema, values: initial, revision: 0 })
    }

    pub fn schema(&self) -> &BTreeMap<String, InteractionType> { &self.schema }
    pub fn values(&self) -> &BTreeMap<String, D1Value> { &self.values }
    pub fn revision(&self) -> u64 { self.revision }
    pub fn action_ids(&self) -> impl Iterator<Item = &str> { self.actions.keys().map(String::as_str) }

    /// Evaluate against an immutable snapshot; publish all writes together only
    /// when the action and the whole-state budget both succeed.
    pub fn dispatch(&mut self, id: &str, inputs: BTreeMap<String, D1Value>) -> Result<(), D1ApplicationError> {
        let action = self.actions.get(id).ok_or_else(|| D1ApplicationError::UnknownAction(id.into()))?;
        let touched: BTreeSet<_> = action.state_reads().iter().chain(action.state_writes()).map(|key| key.name.as_str()).collect();
        let mut values = BTreeMap::new();
        for key in touched {
            values.insert(key.to_owned(), self.values.get(key).cloned().ok_or(D1ApplicationError::StateSchemaMismatch)?);
        }
        let snapshot = D1Snapshot::new(self.revision, values).map_err(|error| D1ApplicationError::InvalidState(error.to_string()))?;
        let input = InputSnapshot::new(inputs).map_err(|error| D1ApplicationError::InvalidInput(error.to_string()))?;
        let outcome = action.execute(&snapshot, &input).map_err(|error| D1ApplicationError::ActionFailed(error.to_string()))?;
        let mut next = self.values.clone();
        for key in action.state_writes() {
            let value = outcome.next_state.get(&key.name).cloned().ok_or(D1ApplicationError::StateSchemaMismatch)?;
            if !matches_type(&value, key.value_type) { return Err(D1ApplicationError::StateSchemaMismatch); }
            next.insert(key.name.clone(), value);
        }
        D1Snapshot::new(outcome.revision_after, next.clone()).map_err(|error| D1ApplicationError::InvalidState(error.to_string()))?;
        self.values = next;
        self.revision = outcome.revision_after;
        Ok(())
    }
}
