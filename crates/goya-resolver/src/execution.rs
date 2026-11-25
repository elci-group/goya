//! D1 publication for an admitted scene.
//!
//! The publish sequence follows `GY-D1-APP/1`: the authored snapshot is
//! revision 0, one action reads exactly its declared keys, and writes land
//! together only after the program and the whole-state budget both succeed.
//! This crate does not call `goya-sdk`, because that facade links target
//! compilers. A dev-dependency test compares the two owners on the same scene.

use std::collections::{BTreeMap, BTreeSet};

use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use goya_lang::{
    compile_d1_interactions, compile_d2_interactions, CompiledInteraction, InputBinding,
    InteractionDecl, InteractionType, InteractionValue, KeyUse, Scene, SourceRange, StateDecl,
    StateLifetime,
};

use crate::diagnostics::{located, Diagnostic};

#[derive(Debug, Clone)]
pub(crate) struct BoundInput {
    pub name: String,
    pub field_id: String,
    pub value_type: InteractionType,
    /// The `key: type from "..."` declaration, when the frontend recorded one.
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone)]
pub(crate) struct ActionDesc {
    pub authored_id: String,
    pub name: String,
    pub trigger: String,
    pub class_name: &'static str,
    pub inputs: Vec<BoundInput>,
    pub writes: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct FieldSeed {
    pub id: String,
    pub text: String,
    pub max_chars: Option<i64>,
    /// Binding range in the source this seed was prepared from.
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone)]
pub(crate) struct StateFact {
    pub key: String,
    pub lifetime: StateLifetime,
}

#[derive(Clone)]
pub(crate) struct Owner {
    actions: BTreeMap<String, CompiledInteraction>,
    values: BTreeMap<String, D1Value>,
    revision: u64,
}

impl Owner {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn values(&self) -> &BTreeMap<String, D1Value> {
        &self.values
    }

    pub(crate) fn restore(
        &mut self,
        values: BTreeMap<String, D1Value>,
        revision: u64,
    ) -> Result<(), Diagnostic> {
        let checked = D1Snapshot::new(revision, values)
            .map_err(|error| Diagnostic::new("GY-RESOLVER-STATE", error.to_string()))?;
        self.values = checked.values().clone();
        self.revision = revision;
        Ok(())
    }

    pub(crate) fn dispatch(
        &mut self,
        id: &str,
        inputs: BTreeMap<String, D1Value>,
    ) -> Result<(), Diagnostic> {
        let (outcome, writes) = {
            let action = self.actions.get(id).ok_or_else(|| {
                Diagnostic::new("GY-RESOLVER-ACTION", format!("unknown action {id}"))
            })?;
            let touched: BTreeSet<&str> = action
                .state_reads()
                .iter()
                .chain(action.state_writes())
                .map(|key| key.name.as_str())
                .collect();
            let mut values = BTreeMap::new();
            for key in touched {
                values.insert(
                    key.to_owned(),
                    self.values.get(key).cloned().ok_or_else(|| {
                        Diagnostic::new(
                            "GY-RESOLVER-STATE",
                            format!("declared state key {key} is missing"),
                        )
                    })?,
                );
            }
            let writes: Vec<(String, InteractionType)> = action
                .state_writes()
                .iter()
                .map(|key| (key.name.clone(), key.value_type))
                .collect();
            let snapshot = D1Snapshot::new(self.revision, values)
                .map_err(|error| Diagnostic::new("GY-RESOLVER-STATE", error.to_string()))?;
            let input = InputSnapshot::new(inputs)
                .map_err(|error| Diagnostic::new("GY-RESOLVER-INPUT", error.to_string()))?;
            let outcome = action
                .execute(&snapshot, &input)
                .map_err(|error| Diagnostic::new("GY-RESOLVER-ACTION", error.to_string()))?;
            (outcome, writes)
        };
        let mut next = self.values.clone();
        for (name, value_type) in writes {
            let value = outcome.next_state.get(&name).cloned().ok_or_else(|| {
                Diagnostic::new("GY-RESOLVER-STATE", format!("action did not write {name}"))
            })?;
            if !value_matches(&value, value_type) {
                return Err(Diagnostic::new(
                    "GY-RESOLVER-STATE",
                    format!("write {name} has the wrong type"),
                ));
            }
            next.insert(name, value);
        }
        let checked = D1Snapshot::new(outcome.revision_after, next)
            .map_err(|error| Diagnostic::new("GY-RESOLVER-STATE", error.to_string()))?;
        self.values = checked.values().clone();
        self.revision = outcome.revision_after;
        Ok(())
    }
}

pub(crate) struct Prepared {
    pub owner: Owner,
    pub rest: BTreeMap<String, InteractionValue>,
    pub facts: Vec<StateFact>,
    pub actions: Vec<ActionDesc>,
    pub fields: Vec<FieldSeed>,
    pub refusal: Option<String>,
}

pub(crate) fn prepare(
    scene: &Scene,
    state: &[StateDecl],
    bindings: &[InputBinding],
    input_keys: &[KeyUse],
    state_uses: &[KeyUse],
) -> Result<Prepared, Diagnostic> {
    let d1 = compile_d1_interactions(scene).map_err(transition)?;
    let d2 = compile_d2_interactions(scene).map_err(transition)?;
    let mut actions = Vec::new();
    let mut programs = BTreeMap::new();
    let mut schema = BTreeMap::<String, InteractionType>::new();
    for (decl, program) in scene
        .interactions
        .iter()
        .filter(|decl| decl.class == goya_lang::InteractionClass::D1)
        .zip(d1)
    {
        let id = require_id(decl)?;
        for key in program.state_reads().iter().chain(program.state_writes()) {
            if let Some(existing) = schema.insert(key.name.clone(), key.value_type) {
                if existing != key.value_type {
                    return Err(located(
                        "GY-RESOLVER-SCHEMA",
                        format!("state key {} is declared with two types", key.name),
                        key_range(state_uses, &decl.name, &key.name),
                    ));
                }
            }
        }
        if programs.insert(id.to_owned(), program).is_some() {
            return Err(Diagnostic::new(
                "GY-RESOLVER-ACTION-ID",
                format!("duplicate action id {id}"),
            ));
        }
        actions.push(describe(decl, "D1", bindings, input_keys)?);
    }
    let mut d2_ids = Vec::new();
    for decl in scene
        .interactions
        .iter()
        .filter(|decl| decl.class == goya_lang::InteractionClass::D2)
    {
        let id = require_id(decl)?;
        if actions.iter().any(|action| action.authored_id == id)
            || d2_ids.iter().any(|have| have == id)
        {
            return Err(Diagnostic::new(
                "GY-RESOLVER-ACTION-ID",
                format!("duplicate action id {id}"),
            ));
        }
        d2_ids.push(id.to_owned());
        actions.push(describe(decl, "D2", bindings, input_keys)?);
    }
    if d2.len() != d2_ids.len() {
        return Err(Diagnostic::new(
            "GY-RESOLVER-TRANSITION",
            "D2 compilation did not match the authored interactions",
        ));
    }
    let refusal = if d2_ids.is_empty() {
        None
    } else {
        Some(format!(
            "D2 interactions ({}) are compiled and are not executed; live host effects are denied",
            d2_ids.join(", ")
        ))
    };
    let materialised = materialise(state, &schema, state_uses)?;
    let owner = Owner {
        actions: programs,
        values: materialised.values,
        revision: 0,
    };
    let fields = field_seeds(scene, &actions)?;
    Ok(Prepared {
        owner,
        rest: materialised.rest,
        facts: materialised.facts,
        actions,
        fields,
        refusal,
    })
}

struct Materialised {
    values: BTreeMap<String, D1Value>,
    rest: BTreeMap<String, InteractionValue>,
    facts: Vec<StateFact>,
}

fn materialise(
    state: &[StateDecl],
    schema: &BTreeMap<String, InteractionType>,
    state_uses: &[KeyUse],
) -> Result<Materialised, Diagnostic> {
    let mut initial = BTreeMap::new();
    let mut rest = BTreeMap::new();
    let mut facts = Vec::new();
    for decl in state {
        facts.push(StateFact {
            key: decl.key.clone(),
            lifetime: decl.lifetime,
        });
        if let Some(ty) = schema.get(&decl.key) {
            if !literal_matches(&decl.initial, *ty) {
                return Err(Diagnostic::new(
                    "GY-RESOLVER-INITIAL",
                    format!("initial value of {} does not match its type", decl.key),
                )
                .with_range(decl.range));
            }
            initial.insert(decl.key.clone(), to_d1(&decl.initial));
        } else {
            rest.insert(decl.key.clone(), decl.initial.clone());
        }
    }
    for key in schema.keys() {
        if !initial.contains_key(key) {
            let range = state_uses
                .iter()
                .find(|item| item.key == *key)
                .map(|item| item.range);
            return Err(located(
                "GY-RESOLVER-INITIAL",
                format!("state key {key} has no authored initial value"),
                range,
            ));
        }
    }
    let snapshot = D1Snapshot::new(0, initial)
        .map_err(|error| Diagnostic::new("GY-RESOLVER-STATE", error.to_string()))?;
    Ok(Materialised {
        values: snapshot.values().clone(),
        rest,
        facts,
    })
}

fn describe(
    decl: &InteractionDecl,
    class_name: &'static str,
    bindings: &[InputBinding],
    input_keys: &[KeyUse],
) -> Result<ActionDesc, Diagnostic> {
    let mut inputs = Vec::new();
    for input in &decl.inputs {
        let Some(binding) = bindings
            .iter()
            .find(|binding| binding.interaction == decl.name && binding.input == input.name)
        else {
            return Err(located(
                "GY-RESOLVER-UNBOUND",
                format!("input {}.{} is not bound to a field", decl.name, input.name),
                key_range(input_keys, &decl.name, &input.name),
            ));
        };
        inputs.push(BoundInput {
            name: input.name.clone(),
            field_id: binding.field_id.clone(),
            value_type: input.value_type,
            range: Some(binding.range),
        });
    }
    Ok(ActionDesc {
        authored_id: require_id(decl)?.to_owned(),
        name: decl.name.clone(),
        trigger: decl.trigger_button.clone(),
        class_name,
        inputs,
        writes: decl
            .state_writes
            .iter()
            .map(|key| key.name.clone())
            .collect(),
    })
}

fn field_seeds(scene: &Scene, actions: &[ActionDesc]) -> Result<Vec<FieldSeed>, Diagnostic> {
    let mut seeds = Vec::<FieldSeed>::new();
    for action in actions {
        for input in &action.inputs {
            if seeds.iter().any(|seed| seed.id == input.field_id) {
                continue;
            }
            let Some(cell) = find_cell(&scene.root, &input.field_id) else {
                return Err(located(
                    "GY-RESOLVER-BINDING",
                    format!(
                        "input field {} is not an element in this scene",
                        input.field_id
                    ),
                    input.range,
                ));
            };
            seeds.push(FieldSeed {
                id: input.field_id.clone(),
                text: cell.text.clone().unwrap_or_default(),
                max_chars: cell.max_length,
                range: input.range,
            });
        }
    }
    Ok(seeds)
}

fn find_cell<'a>(cell: &'a goya_lang::SceneCell, id: &str) -> Option<&'a goya_lang::SceneCell> {
    if cell.authored_id.as_deref() == Some(id) {
        return Some(cell);
    }
    cell.children.iter().find_map(|child| find_cell(child, id))
}

fn require_id(decl: &InteractionDecl) -> Result<&str, Diagnostic> {
    decl.authored_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            Diagnostic::new(
                "GY-RESOLVER-ACTION-ID",
                format!("interaction {} has no authored id", decl.name),
            )
        })
}

fn key_range(uses: &[KeyUse], interaction: &str, key: &str) -> Option<SourceRange> {
    uses.iter()
        .find(|item| item.interaction == interaction && item.key == key)
        .map(|item| item.range)
}

fn transition(error: goya_lang::TransitionError) -> Diagnostic {
    Diagnostic::new("GY-RESOLVER-TRANSITION", error.to_string())
}

pub(crate) fn to_d1(value: &InteractionValue) -> D1Value {
    match value {
        InteractionValue::Int(value) => D1Value::Int(*value),
        InteractionValue::Text(value) => D1Value::Text(value.clone()),
        InteractionValue::Bool(value) => D1Value::Bool(*value),
    }
}

pub(crate) fn from_d1(value: &D1Value) -> InteractionValue {
    match value {
        D1Value::Int(value) => InteractionValue::Int(*value),
        D1Value::Text(value) => InteractionValue::Text(value.clone()),
        D1Value::Bool(value) => InteractionValue::Bool(*value),
    }
}

fn literal_matches(value: &InteractionValue, ty: InteractionType) -> bool {
    matches!(
        (value, ty),
        (InteractionValue::Int(_), InteractionType::Int)
            | (InteractionValue::Text(_), InteractionType::Text)
            | (InteractionValue::Bool(_), InteractionType::Bool)
    )
}

fn value_matches(value: &D1Value, ty: InteractionType) -> bool {
    matches!(
        (value, ty),
        (D1Value::Int(_), InteractionType::Int)
            | (D1Value::Text(_), InteractionType::Text)
            | (D1Value::Bool(_), InteractionType::Bool)
    )
}
