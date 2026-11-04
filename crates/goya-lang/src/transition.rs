//! Conservative lowering of authored single-button D1 interactions.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use goya_determinism::proofs::ExecutionProof;
use goya_determinism::{DeterminismClass, ProcessDecl};
use goya_executable::{D1Instruction, D1Program, D1Value, StateContract};
use goya_ir::ActionId;

use crate::scene::{CellKind, InteractionDecl, InteractionExpr, InteractionType, InteractionValue, Scene};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    InvalidDeclaration,
    TriggerNotFound,
    TriggerAmbiguous,
    InvalidSchema,
    UndeclaredRead,
    TypeMismatch,
    AssignmentMismatch,
    VmRejected(String),
}
impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{self:?}") }
}
impl std::error::Error for TransitionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledInteraction {
    pub name: String,
    pub trigger: ActionId,
    pub program: D1Program,
}

/// Compile every authored interaction to the existing D1 bytecode verifier.
/// Trigger numbering matches `lower`: buttons receive depth-first preorder IDs.
pub fn compile_d1_interactions(scene: &Scene) -> Result<Vec<CompiledInteraction>, TransitionError> {
    let mut buttons = Vec::<(String, ActionId)>::new();
    collect_buttons(&scene.root, &mut 0, &mut buttons);
    scene.interactions.iter().map(|i| {
        let matches: Vec<_> = buttons.iter().filter(|(name, _)| name == &i.trigger_button).collect();
        let trigger = match matches.as_slice() {
            [] => return Err(TransitionError::TriggerNotFound),
            [(_, id)] => *id,
            _ => return Err(TransitionError::TriggerAmbiguous),
        };
        Ok(CompiledInteraction { name: i.name.clone(), trigger, program: compile_one(i)? })
    }).collect()
}

fn collect_buttons(cell: &crate::scene::SceneCell, next: &mut u64, out: &mut Vec<(String, ActionId)>) {
    if cell.kind == CellKind::Button { out.push((cell.name.clone(), ActionId(*next))); *next += 1; }
    for child in &cell.children { collect_buttons(child, next, out); }
}

fn compile_one(i: &InteractionDecl) -> Result<D1Program, TransitionError> {
    if i.name.is_empty() || i.name.len() > 128 || i.assignments.is_empty() { return Err(TransitionError::InvalidDeclaration); }
    let reads = key_types(&i.state_reads)?;
    let writes = key_types(&i.state_writes)?;
    let inputs = key_types(&i.inputs)?;
    let mut instructions = Vec::new();
    let mut register = 0usize;
    let mut assigned = BTreeSet::new();
    let mut used_reads = BTreeSet::new();
    let mut used_inputs = BTreeSet::new();
    for assignment in &i.assignments {
        let expected = *writes.get(&assignment.key).ok_or(TransitionError::AssignmentMismatch)?;
        if !assigned.insert(assignment.key.as_str()) { return Err(TransitionError::AssignmentMismatch); }
        let (actual, src) = compile_expr(&assignment.value, &mut instructions, &mut register, &reads, &inputs, &mut used_reads, &mut used_inputs)?;
        if actual != expected { return Err(TransitionError::TypeMismatch); }
        instructions.push(D1Instruction::WriteState { key: assignment.key.clone(), src });
    }
    if assigned.len() != writes.len() || used_reads.len() != reads.len() || used_inputs.len() != inputs.len() {
        return Err(TransitionError::InvalidSchema);
    }
    let contract = StateContract::new(reads.keys().map(|k| (*k).to_string()), writes.keys().map(|k| (*k).to_string()), inputs.keys().map(|k| (*k).to_string()))
        .map_err(|e| TransitionError::VmRejected(e.to_string()))?;
    let declaration = ProcessDecl { name: i.name.clone(), claims: DeterminismClass::StateDeterministic, declared_effects: vec![], state_contract: true };
    let proof = ExecutionProof { process: i.name.clone(), class: DeterminismClass::StateDeterministic, compiler_version: env!("CARGO_PKG_VERSION").to_string(), schema_version: 1, policy_version: 1, target: "goya-d1-vm-v1".to_string() };
    D1Program::compile(&declaration, &proof, contract, instructions).map_err(|e| TransitionError::VmRejected(e.to_string()))
}

fn key_types(keys: &[crate::scene::TypedKey]) -> Result<BTreeMap<&str, InteractionType>, TransitionError> {
    let mut out = BTreeMap::new();
    for key in keys {
        if key.name.is_empty() || key.name.len() > 256 || out.insert(key.name.as_str(), key.value_type).is_some() { return Err(TransitionError::InvalidSchema); }
    }
    Ok(out)
}

fn compile_expr(
    expr: &InteractionExpr, code: &mut Vec<D1Instruction>, next: &mut usize,
    reads: &BTreeMap<&str, InteractionType>, inputs: &BTreeMap<&str, InteractionType>,
    used_reads: &mut BTreeSet<String>, used_inputs: &mut BTreeSet<String>,
) -> Result<(InteractionType, String), TransitionError> {
    let dst = || format!("r{}", *next);
    match expr {
        InteractionExpr::Literal(value) => {
            let (ty, value) = match value { InteractionValue::Int(v) => (InteractionType::Int, D1Value::Int(*v)), InteractionValue::Text(v) => (InteractionType::Text, D1Value::Text(v.clone())), InteractionValue::Bool(v) => (InteractionType::Bool, D1Value::Bool(*v)) };
            let reg = dst(); *next += 1; code.push(D1Instruction::Constant { dst: reg.clone(), value }); Ok((ty, reg))
        }
        InteractionExpr::State(key) => {
            let ty = *reads.get(key.as_str()).ok_or(TransitionError::UndeclaredRead)?;
            used_reads.insert(key.clone()); let reg = dst(); *next += 1; code.push(D1Instruction::ReadState { dst: reg.clone(), key: key.clone() }); Ok((ty, reg))
        }
        InteractionExpr::Input(key) => {
            let ty = *inputs.get(key.as_str()).ok_or(TransitionError::UndeclaredRead)?;
            used_inputs.insert(key.clone()); let reg = dst(); *next += 1; code.push(D1Instruction::ReadInput { dst: reg.clone(), key: key.clone() }); Ok((ty, reg))
        }
        InteractionExpr::Add(a, b) => {
            let (ta, ra) = compile_expr(a, code, next, reads, inputs, used_reads, used_inputs)?;
            let (tb, rb) = compile_expr(b, code, next, reads, inputs, used_reads, used_inputs)?;
            if ta != InteractionType::Int || tb != InteractionType::Int { return Err(TransitionError::TypeMismatch); }
            let reg = dst(); *next += 1; code.push(D1Instruction::AddInt { dst: reg.clone(), left: ra, right: rb }); Ok((InteractionType::Int, reg))
        }
    }
}
