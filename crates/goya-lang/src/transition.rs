//! Conservative lowering of authored single-button D1 and static-journal D2 interactions.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use goya_determinism::proofs::ExecutionProof;
use goya_determinism::{DeterminismClass, ProcessDecl};
use goya_executable::{
    CapabilityLabel, D1Instruction, D1Outcome, D1Program, D1Snapshot, D1Value, D2Instruction,
    D2Outcome, D2Program, EffectJournal, EffectResultType, InputSnapshot, StateContract,
};
use goya_ir::ActionId;

use crate::scene::{
    CellKind, EffectRequestDecl, EffectValueType, InteractionClass, InteractionDecl,
    InteractionExpr, InteractionType, InteractionValue, Scene,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    InvalidDeclaration,
    TriggerNotFound,
    TriggerAmbiguous,
    TriggerAlreadyBound,
    DuplicateInteractionName,
    InvalidSchema,
    UndeclaredRead,
    TypeMismatch,
    StateTypeMismatch,
    InputTypeMismatch,
    OutputTypeMismatch,
    AssignmentMismatch,
    UndeclaredEffect,
    CapabilityMismatch,
    DuplicateRequest,
    RequestResultMismatch,
    VmRejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledD2Interaction {
    name: String,
    trigger: ActionId,
    state_reads: Vec<crate::scene::TypedKey>,
    state_writes: Vec<crate::scene::TypedKey>,
    inputs: Vec<crate::scene::TypedKey>,
    declaration: ProcessDecl,
    proof: ExecutionProof,
    program: D2Program,
}

impl CompiledD2Interaction {
    pub fn name(&self) -> &str { &self.name }
    pub fn trigger(&self) -> ActionId { self.trigger }
    pub fn declaration(&self) -> &ProcessDecl { &self.declaration }
    pub fn proof(&self) -> &ExecutionProof { &self.proof }
    pub fn program_hash(&self) -> [u8; 32] { self.program.program_hash() }
    pub fn state_reads(&self) -> &[crate::scene::TypedKey] { &self.state_reads }
    pub fn state_writes(&self) -> &[crate::scene::TypedKey] { &self.state_writes }
    pub fn inputs(&self) -> &[crate::scene::TypedKey] { &self.inputs }

    /// Replay only caller supplied journal entries. This never invokes a host effect.
    pub fn execute(
        &self,
        state: &D1Snapshot,
        inputs: &InputSnapshot,
        journal: &EffectJournal,
    ) -> Result<D2Outcome, TransitionError> {
        validate_runtime_values(&self.state_reads, &self.state_writes, &self.inputs, state, inputs)?;
        let outcome = self.program.execute(state, inputs, journal)
            .map_err(|e| TransitionError::VmRejected(e.to_string()))?;
        validate_output_types(&self.state_writes, &outcome.next_state)?;
        Ok(outcome)
    }
}
impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TransitionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInteraction {
    name: String,
    trigger: ActionId,
    state_reads: Vec<crate::scene::TypedKey>,
    state_writes: Vec<crate::scene::TypedKey>,
    inputs: Vec<crate::scene::TypedKey>,
    program: D1Program,
}

impl CompiledInteraction {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn trigger(&self) -> ActionId {
        self.trigger
    }
    /// Stable hash of the verified D1 bytecode, without exposing the raw VM
    /// program or bypassing this interaction's typed execution checks.
    pub fn program_hash(&self) -> [u8; 32] {
        self.program.program_hash()
    }
    pub fn state_reads(&self) -> &[crate::scene::TypedKey] {
        &self.state_reads
    }
    pub fn state_writes(&self) -> &[crate::scene::TypedKey] {
        &self.state_writes
    }
    pub fn inputs(&self) -> &[crate::scene::TypedKey] {
        &self.inputs
    }

    /// Execute the transition while enforcing its immutable authored schema.
    /// The lower-level VM program is intentionally not exposed until a typed
    /// bridge can preserve these checks end to end.
    pub fn execute(
        &self,
        state: &D1Snapshot,
        inputs: &InputSnapshot,
    ) -> Result<D1Outcome, TransitionError> {
        validate_runtime_values(&self.state_reads, &self.state_writes, &self.inputs, state, inputs)?;
        let outcome = self
            .program
            .execute(state, inputs)
            .map_err(|e| TransitionError::VmRejected(e.to_string()))?;
        validate_output_types(&self.state_writes, &outcome.next_state)?;
        Ok(outcome)
    }
}

/// Compile every authored interaction to the existing D1 bytecode verifier.
/// Trigger numbering matches `lower`: buttons receive depth-first preorder IDs.
pub fn compile_d1_interactions(scene: &Scene) -> Result<Vec<CompiledInteraction>, TransitionError> {
    let triggers = validate_bindings(scene)?;
    scene.interactions.iter().zip(triggers).filter(|(decl, _)| decl.class == InteractionClass::D1).map(|(i, trigger)| {
            Ok(CompiledInteraction {
                name: i.name.clone(),
                trigger,
                state_reads: i.state_reads.clone(),
                state_writes: i.state_writes.clone(),
                inputs: i.inputs.clone(),
                program: compile_one(i)?,
            })
        }).collect()
}

pub fn compile_d2_interactions(scene: &Scene) -> Result<Vec<CompiledD2Interaction>, TransitionError> {
    let triggers = validate_bindings(scene)?;
    scene.interactions.iter().zip(triggers).filter(|(decl, _)| decl.class == InteractionClass::D2).map(|(i, trigger)| {
        let (declaration, proof, program) = compile_one_d2(i)?;
        Ok(CompiledD2Interaction { name: i.name.clone(), trigger, state_reads: i.state_reads.clone(), state_writes: i.state_writes.clone(), inputs: i.inputs.clone(), declaration, proof, program })
    }).collect()
}

fn validate_bindings(scene: &Scene) -> Result<Vec<ActionId>, TransitionError> {
    let mut buttons = Vec::<(String, ActionId)>::new();
    collect_buttons(&scene.root, &mut 0, &mut buttons);
    let mut names = BTreeSet::new();
    let mut bound_triggers = BTreeSet::new();
    scene.interactions.iter().map(|i| {
            if !names.insert(i.name.as_str()) {
                return Err(TransitionError::DuplicateInteractionName);
            }
            let matches: Vec<_> = buttons
                .iter()
                .filter(|(name, _)| name == &i.trigger_button)
                .collect();
            let trigger = match matches.as_slice() {
                [] => return Err(TransitionError::TriggerNotFound),
                [(_, id)] => *id,
                _ => return Err(TransitionError::TriggerAmbiguous),
            };
            if !bound_triggers.insert(trigger.0) {
                return Err(TransitionError::TriggerAlreadyBound);
            }
            Ok(trigger)
        })
        .collect()
}

fn collect_buttons(
    cell: &crate::scene::SceneCell,
    next: &mut u64,
    out: &mut Vec<(String, ActionId)>,
) {
    if cell.kind == CellKind::Button {
        out.push((cell.name.clone(), ActionId(*next)));
        *next += 1;
    } else {
        for child in &cell.children {
            collect_buttons(child, next, out);
        }
    }
}

fn compile_one(i: &InteractionDecl) -> Result<D1Program, TransitionError> {
    if i.class != InteractionClass::D1 || !i.declared_effects.is_empty() || !i.requests.is_empty() || i.name.is_empty() || i.name.len() > 128 || i.assignments.is_empty() {
        return Err(TransitionError::InvalidDeclaration);
    }
    let reads = key_types(&i.state_reads)?;
    let writes = key_types(&i.state_writes)?;
    let inputs = key_types(&i.inputs)?;
    for (key, ty) in &reads {
        if writes.get(key).is_some_and(|write_ty| write_ty != ty) {
            return Err(TransitionError::StateTypeMismatch);
        }
    }
    let mut instructions = Vec::new();
    let mut register = 0usize;
    let mut assigned = BTreeSet::new();
    let mut used_reads = BTreeSet::new();
    let mut used_inputs = BTreeSet::new();
    for assignment in &i.assignments {
        let expected = *writes
            .get(assignment.key.as_str())
            .ok_or(TransitionError::AssignmentMismatch)?;
        if !assigned.insert(assignment.key.as_str()) {
            return Err(TransitionError::AssignmentMismatch);
        }
        let (actual, src) = compile_expr(
            &assignment.value,
            &mut instructions,
            &mut register,
            &reads,
            &inputs,
            &mut used_reads,
            &mut used_inputs,
        )?;
        if actual != expected {
            return Err(TransitionError::TypeMismatch);
        }
        instructions.push(D1Instruction::WriteState {
            key: assignment.key.clone(),
            src,
        });
    }
    if assigned.len() != writes.len()
        || used_reads.len() != reads.len()
        || used_inputs.len() != inputs.len()
    {
        return Err(TransitionError::InvalidSchema);
    }
    let contract = StateContract::new(
        reads.keys().map(|k| (*k).to_string()),
        writes.keys().map(|k| (*k).to_string()),
        inputs.keys().map(|k| (*k).to_string()),
    )
    .map_err(|e| TransitionError::VmRejected(e.to_string()))?;
    let declaration = ProcessDecl {
        name: i.name.clone(),
        claims: DeterminismClass::StateDeterministic,
        declared_effects: vec![],
        state_contract: true,
    };
    let proof = ExecutionProof {
        process: i.name.clone(),
        class: DeterminismClass::StateDeterministic,
        compiler_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: 1,
        policy_version: 1,
        target: "goya-d1-vm-v1".to_string(),
    };
    D1Program::compile(&declaration, &proof, contract, instructions)
        .map_err(|e| TransitionError::VmRejected(e.to_string()))
}

fn compile_one_d2(i: &InteractionDecl) -> Result<(ProcessDecl, ExecutionProof, D2Program), TransitionError> {
    if i.class != InteractionClass::D2 || i.name.is_empty() || i.name.len() > 128
        || i.assignments.is_empty() || i.requests.is_empty() || i.declared_effects.is_empty() {
        return Err(TransitionError::InvalidDeclaration);
    }
    let reads = key_types(&i.state_reads)?;
    let writes = key_types(&i.state_writes)?;
    let inputs = key_types(&i.inputs)?;
    for (key, ty) in &reads {
        if writes.get(key).is_some_and(|write_ty| write_ty != ty) { return Err(TransitionError::StateTypeMismatch); }
    }
    let mut declared = BTreeSet::new();
    for effect in &i.declared_effects {
        if !declared.insert(effect.as_str()) { return Err(TransitionError::InvalidSchema); }
    }
    let mut requests = BTreeMap::new();
    let mut requested_effects = BTreeSet::new();
    for request in &i.requests {
        if request.name.is_empty() || request.name.len() > 128 || requests.insert(request.name.as_str(), request).is_some() {
            return Err(TransitionError::DuplicateRequest);
        }
        let label = CapabilityLabel::new(request.capability.clone()).map_err(|_| TransitionError::CapabilityMismatch)?;
        let kind = i.declared_effects.iter().find(|effect| effect.as_str() == label.as_str())
            .ok_or(TransitionError::UndeclaredEffect)?;
        if request.capability != kind.as_str() { return Err(TransitionError::CapabilityMismatch); }
        requested_effects.insert(kind.as_str());
    }
    if requested_effects.len() != declared.len() { return Err(TransitionError::InvalidSchema); }

    let mut instructions = Vec::new();
    let mut register = 0usize;
    let mut assigned = BTreeSet::new();
    let mut used_reads = BTreeSet::new();
    let mut used_inputs = BTreeSet::new();
    let mut used_requests = BTreeSet::new();
    for assignment in &i.assignments {
        let expected = *writes.get(assignment.key.as_str()).ok_or(TransitionError::AssignmentMismatch)?;
        if !assigned.insert(assignment.key.as_str()) { return Err(TransitionError::AssignmentMismatch); }
        let (actual, src) = compile_expr_d2(&assignment.value, &mut instructions, &mut register,
            &reads, &inputs, &requests, &mut used_reads, &mut used_inputs, &mut used_requests)?;
        if actual != expected { return Err(TransitionError::TypeMismatch); }
        instructions.push(D2Instruction::WriteState { key: assignment.key.clone(), src });
    }
    if assigned.len() != writes.len() || used_reads.len() != reads.len()
        || used_inputs.len() != inputs.len() || used_requests.len() != requests.len() {
        return Err(TransitionError::InvalidSchema);
    }
    let contract = StateContract::new(reads.keys().map(|k| (*k).to_string()), writes.keys().map(|k| (*k).to_string()), inputs.keys().map(|k| (*k).to_string()))
        .map_err(|e| TransitionError::VmRejected(e.to_string()))?;
    let declaration = ProcessDecl { name: i.name.clone(), claims: DeterminismClass::EffectDeterministic, declared_effects: i.declared_effects.clone(), state_contract: false };
    let proof = ExecutionProof { process: i.name.clone(), class: DeterminismClass::EffectDeterministic,
        compiler_version: env!("CARGO_PKG_VERSION").to_string(), schema_version: 1, policy_version: 1,
        target: "goya-d2-vm-v1".to_string() };
    let program = D2Program::compile(&declaration, &proof, contract, instructions)
        .map_err(|e| TransitionError::VmRejected(e.to_string()))?;
    Ok((declaration, proof, program))
}

fn compile_expr_d2(
    expr: &InteractionExpr, code: &mut Vec<D2Instruction>, next: &mut usize,
    reads: &BTreeMap<&str, InteractionType>, inputs: &BTreeMap<&str, InteractionType>,
    requests: &BTreeMap<&str, &EffectRequestDecl>, used_reads: &mut BTreeSet<String>,
    used_inputs: &mut BTreeSet<String>, used_requests: &mut BTreeSet<String>,
) -> Result<(InteractionType, String), TransitionError> {
    match expr {
        InteractionExpr::Literal(value) => {
            let (ty, value) = match value { InteractionValue::Int(v) => (InteractionType::Int, D1Value::Int(*v)), InteractionValue::Text(v) => (InteractionType::Text, D1Value::Text(v.clone())), InteractionValue::Bool(v) => (InteractionType::Bool, D1Value::Bool(*v)) };
            let reg = format!("r{next}"); *next += 1; code.push(D2Instruction::Constant { dst: reg.clone(), value }); Ok((ty, reg))
        }
        InteractionExpr::State(key) => {
            let ty = *reads.get(key.as_str()).ok_or(TransitionError::UndeclaredRead)?; used_reads.insert(key.clone());
            let reg = format!("r{next}"); *next += 1; code.push(D2Instruction::ReadState { dst: reg.clone(), key: key.clone() }); Ok((ty, reg))
        }
        InteractionExpr::Input(key) => {
            let ty = *inputs.get(key.as_str()).ok_or(TransitionError::UndeclaredRead)?; used_inputs.insert(key.clone());
            let reg = format!("r{next}"); *next += 1; code.push(D2Instruction::ReadInput { dst: reg.clone(), key: key.clone() }); Ok((ty, reg))
        }
        InteractionExpr::Effect(name) => {
            let request = requests.get(name.as_str()).ok_or(TransitionError::UndeclaredEffect)?;
            if !used_requests.insert(name.clone()) { return Err(TransitionError::DuplicateRequest); }
            let result_type = match request.result_type { EffectValueType::Int64 => EffectResultType::Int64, EffectValueType::Text => EffectResultType::Text, EffectValueType::Bool => EffectResultType::Bool };
            let value_type = match request.result_type { EffectValueType::Int64 => InteractionType::Int, EffectValueType::Text => InteractionType::Text, EffectValueType::Bool => InteractionType::Bool };
            let capability = CapabilityLabel::new(request.capability.clone()).map_err(|_| TransitionError::CapabilityMismatch)?;
            let reg = format!("r{next}"); *next += 1;
            code.push(D2Instruction::RequestEffect { dst: reg.clone(), capability, request: request.request_utf8.as_bytes().to_vec(), result_type });
            Ok((value_type, reg))
        }
        InteractionExpr::Add(a, b) => {
            let (ta, ra) = compile_expr_d2(a, code, next, reads, inputs, requests, used_reads, used_inputs, used_requests)?;
            let (tb, rb) = compile_expr_d2(b, code, next, reads, inputs, requests, used_reads, used_inputs, used_requests)?;
            if ta != InteractionType::Int || tb != InteractionType::Int { return Err(TransitionError::TypeMismatch); }
            let reg = format!("r{next}"); *next += 1; code.push(D2Instruction::AddInt { dst: reg.clone(), left: ra, right: rb }); Ok((InteractionType::Int, reg))
        }
    }
}

fn key_types(
    keys: &[crate::scene::TypedKey],
) -> Result<BTreeMap<&str, InteractionType>, TransitionError> {
    let mut out = BTreeMap::new();
    for key in keys {
        if key.name.is_empty()
            || key.name.len() > 256
            || out.insert(key.name.as_str(), key.value_type).is_some()
        {
            return Err(TransitionError::InvalidSchema);
        }
    }
    Ok(out)
}

fn typed_union<'a>(
    reads: &'a [crate::scene::TypedKey],
    writes: &'a [crate::scene::TypedKey],
) -> Result<BTreeMap<&'a str, InteractionType>, TransitionError> {
    let mut all = key_types(reads)?;
    for (key, ty) in key_types(writes)? {
        if all.get(key).is_some_and(|existing| *existing != ty) {
            return Err(TransitionError::StateTypeMismatch);
        }
        all.insert(key, ty);
    }
    Ok(all)
}

fn validate_runtime_values(
    reads: &[crate::scene::TypedKey],
    writes: &[crate::scene::TypedKey],
    declared_inputs: &[crate::scene::TypedKey],
    state: &D1Snapshot,
    inputs: &InputSnapshot,
) -> Result<(), TransitionError> {
    let state_schema = typed_union(reads, writes)?;
    if state.values().len() != state_schema.len()
        || state.values().iter().any(|(key, value)| {
            state_schema.get(key.as_str()).is_none_or(|ty| !value_has_type(value, *ty))
        }) { return Err(TransitionError::StateTypeMismatch); }
    if inputs.values().len() != declared_inputs.len()
        || inputs.values().iter().any(|(key, value)| {
            declared_inputs.iter().find(|item| item.name == *key)
                .is_none_or(|item| !value_has_type(value, item.value_type))
        }) { return Err(TransitionError::InputTypeMismatch); }
    Ok(())
}

fn value_has_type(value: &D1Value, ty: InteractionType) -> bool {
    matches!(
        (value, ty),
        (D1Value::Int(_), InteractionType::Int)
            | (D1Value::Text(_), InteractionType::Text)
            | (D1Value::Bool(_), InteractionType::Bool)
    )
}

fn validate_output_types(
    writes: &[crate::scene::TypedKey],
    next_state: &BTreeMap<String, D1Value>,
) -> Result<(), TransitionError> {
    if writes.iter().any(|key| {
        next_state
            .get(&key.name)
            .is_none_or(|value| !value_has_type(value, key.value_type))
    }) {
        return Err(TransitionError::OutputTypeMismatch);
    }
    Ok(())
}

fn compile_expr(
    expr: &InteractionExpr,
    code: &mut Vec<D1Instruction>,
    next: &mut usize,
    reads: &BTreeMap<&str, InteractionType>,
    inputs: &BTreeMap<&str, InteractionType>,
    used_reads: &mut BTreeSet<String>,
    used_inputs: &mut BTreeSet<String>,
) -> Result<(InteractionType, String), TransitionError> {
    match expr {
        InteractionExpr::Literal(value) => {
            let (ty, value) = match value {
                InteractionValue::Int(v) => (InteractionType::Int, D1Value::Int(*v)),
                InteractionValue::Text(v) => (InteractionType::Text, D1Value::Text(v.clone())),
                InteractionValue::Bool(v) => (InteractionType::Bool, D1Value::Bool(*v)),
            };
            let reg = format!("r{next}");
            *next += 1;
            code.push(D1Instruction::Constant {
                dst: reg.clone(),
                value,
            });
            Ok((ty, reg))
        }
        InteractionExpr::State(key) => {
            let ty = *reads
                .get(key.as_str())
                .ok_or(TransitionError::UndeclaredRead)?;
            used_reads.insert(key.clone());
            let reg = format!("r{next}");
            *next += 1;
            code.push(D1Instruction::ReadState {
                dst: reg.clone(),
                key: key.clone(),
            });
            Ok((ty, reg))
        }
        InteractionExpr::Input(key) => {
            let ty = *inputs
                .get(key.as_str())
                .ok_or(TransitionError::UndeclaredRead)?;
            used_inputs.insert(key.clone());
            let reg = format!("r{next}");
            *next += 1;
            code.push(D1Instruction::ReadInput {
                dst: reg.clone(),
                key: key.clone(),
            });
            Ok((ty, reg))
        }
        InteractionExpr::Add(a, b) => {
            let (ta, ra) = compile_expr(a, code, next, reads, inputs, used_reads, used_inputs)?;
            let (tb, rb) = compile_expr(b, code, next, reads, inputs, used_reads, used_inputs)?;
            if ta != InteractionType::Int || tb != InteractionType::Int {
                return Err(TransitionError::TypeMismatch);
            }
            let reg = format!("r{next}");
            *next += 1;
            code.push(D1Instruction::AddInt {
                dst: reg.clone(),
                left: ra,
                right: rb,
            });
            Ok((InteractionType::Int, reg))
        }
        InteractionExpr::Effect(_) => Err(TransitionError::UndeclaredEffect),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{validate_output_types, InteractionType};
    use crate::{compile_d1_interactions, parse, TransitionError};
    use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
    use goya_ir::ActionId;

    const SOURCE: &str = "@ontology app { @button plus {} @interaction increment { trigger: button.plus.activate; class: D1; state { read count: int; write count: int; } input { amount: int; } set count = state.count + input.amount; } }";

    #[test]
    fn authored_interaction_compiles_binds_and_executes_d1() {
        let scene = parse(SOURCE).unwrap();
        let compiled = compile_d1_interactions(&scene).unwrap();
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].trigger(), ActionId(0));
        let state =
            D1Snapshot::new(4, BTreeMap::from([("count".into(), D1Value::Int(9))])).unwrap();
        let inputs =
            InputSnapshot::new(BTreeMap::from([("amount".into(), D1Value::Int(3))])).unwrap();
        let outcome = compiled[0].execute(&state, &inputs).unwrap();
        assert_eq!(outcome.next_state["count"], D1Value::Int(12));
        assert_eq!(outcome.revision_after, 5);
    }

    #[test]
    fn undeclared_state_reads_are_rejected() {
        let src = SOURCE.replace("read count: int;", "read other: int;");
        let scene = parse(&src).unwrap();
        assert_eq!(
            compile_d1_interactions(&scene),
            Err(TransitionError::UndeclaredRead)
        );
    }

    #[test]
    fn missing_trigger_and_d2_class_are_rejected() {
        let scene = parse(&SOURCE.replace("button.plus", "button.missing")).unwrap();
        assert_eq!(
            compile_d1_interactions(&scene),
            Err(TransitionError::TriggerNotFound)
        );
        assert!(parse(&SOURCE.replace("class: D1", "class: D2")).is_err());
    }

    #[test]
    fn mismatched_state_contract_types_are_rejected() {
        let src = SOURCE.replace("write count: int", "write count: text");
        let scene = parse(&src).unwrap();
        assert_eq!(
            compile_d1_interactions(&scene),
            Err(TransitionError::StateTypeMismatch)
        );
    }

    #[test]
    fn runtime_values_are_checked_against_authored_types() {
        let scene = parse(SOURCE).unwrap();
        let compiled = compile_d1_interactions(&scene).unwrap();
        let state = D1Snapshot::new(
            4,
            BTreeMap::from([("count".into(), D1Value::Text("bad".into()))]),
        )
        .unwrap();
        let inputs =
            InputSnapshot::new(BTreeMap::from([("amount".into(), D1Value::Int(3))])).unwrap();
        assert_eq!(
            compiled[0].execute(&state, &inputs),
            Err(TransitionError::StateTypeMismatch)
        );
    }

    #[test]
    fn trigger_ids_match_lowering_when_buttons_have_children() {
        let src = "@ontology app { @button outer { @button hidden {} } @button target {} @interaction pick { trigger: button.target.activate; class: D1; state { write count: int; } input { amount: int; } set count = input.amount; } }";
        let scene = parse(src).unwrap();
        let compiled = compile_d1_interactions(&scene).unwrap();
        assert_eq!(compiled[0].trigger(), ActionId(1));
    }

    #[test]
    fn duplicate_interaction_names_and_button_bindings_are_rejected() {
        let mut scene = parse(SOURCE).unwrap();
        scene.interactions.push(scene.interactions[0].clone());
        assert_eq!(
            compile_d1_interactions(&scene),
            Err(TransitionError::DuplicateInteractionName)
        );
        scene.interactions[1].name = "another".into();
        assert_eq!(
            compile_d1_interactions(&scene),
            Err(TransitionError::TriggerAlreadyBound)
        );
    }

    #[test]
    fn duplicate_button_names_are_ambiguous() {
        let src = SOURCE.replace("@button plus {}", "@button plus {} @button plus {}");
        let scene = parse(&src).unwrap();
        assert_eq!(
            compile_d1_interactions(&scene),
            Err(TransitionError::TriggerAmbiguous)
        );
    }

    #[test]
    fn output_types_are_checked() {
        let writes = [crate::scene::TypedKey {
            name: "count".into(),
            value_type: InteractionType::Int,
        }];
        let output = BTreeMap::from([("count".into(), D1Value::Text("wrong".into()))]);
        assert_eq!(
            validate_output_types(&writes, &output),
            Err(TransitionError::OutputTypeMismatch)
        );
    }

    #[test]
    fn identical_source_compiles_to_identical_trigger_and_hash() {
        let first_scene = parse(SOURCE).unwrap();
        let second_scene = parse(SOURCE).unwrap();
        let first = compile_d1_interactions(&first_scene).unwrap();
        let second = compile_d1_interactions(&second_scene).unwrap();
        assert_eq!(first[0].trigger(), second[0].trigger());
        assert_eq!(first[0].program_hash(), second[0].program_hash());
    }
}
