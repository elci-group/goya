//! Experimental target representation for Goya's bounded D1 transitions.
//! It is not connected to Swift emission or admitted for iOS builds.
use std::collections::BTreeMap;
use goya_lang::{InteractionClass, InteractionExpr, InteractionType, InteractionValue, SourceRange, TypedKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub declared_id: String,
    pub source_range: SourceRange,
    pub trigger_name: String,
    pub reference_program_hash: [u8; 32],
    pub state_reads: Vec<TypedKey>,
    pub state_writes: Vec<TypedKey>,
    pub inputs: Vec<TypedKey>,
    pub assignments: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment { pub key: String, pub value: Expression }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression { pub value_type: InteractionType, pub operation: Operation }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Constant(InteractionValue), State(String), Input(String),
    CheckedAdd(Box<Expression>, Box<Expression>),
}

fn rejected(message: impl Into<String>, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic { code: "NPT-E2104", message: message.into(), range }
}

/// Lower verified D1 declarations without inferring unsupported effect behavior.
/// Expressions read the original state/input snapshot; assignments are staged
/// and committed atomically only after every expression succeeds.
pub fn lower(source: &str) -> Result<Vec<Transition>, crate::Diagnostic> {
    crate::inspect(source)?;
    let parsed = goya_lang::parse_strict(source).map_err(|e| rejected(e.to_string(), Some(e.range)))?;
    if parsed.scene.interactions.iter().any(|i| i.class != InteractionClass::D1) {
        let range = parsed.entries.iter().find(|e| e.kind == "interaction").map(|e| e.range);
        return Err(rejected("D2 effects are not supported by this lowering", range));
    }
    let reference = goya_lang::compile_d1_interactions(&parsed.scene).map_err(|e| rejected(e.to_string(), None))?;
    let mut output = Vec::new();
    for (decl, verified) in parsed.scene.interactions.iter().zip(reference) {
        let id = decl.authored_id.as_ref().ok_or_else(|| rejected("interaction requires an authored ID", None))?;
        let range = parsed.entries.iter().find(|e| e.kind == "interaction" && e.semantic_id.as_ref() == Some(id)).map(|e| e.range).ok_or_else(|| rejected("interaction source range unavailable", None))?;
        let reads: BTreeMap<_, _> = decl.state_reads.iter().map(|key| (key.name.as_str(), key.value_type)).collect();
        let inputs: BTreeMap<_, _> = decl.inputs.iter().map(|key| (key.name.as_str(), key.value_type)).collect();
        let mut assignments = Vec::new();
        for assignment in &decl.assignments {
            let value = lower_expr(&assignment.value, &reads, &inputs).map_err(|message| rejected(message, Some(range)))?;
            let expected = decl.state_writes.iter().find(|key| key.name == assignment.key).map(|key| key.value_type);
            if Some(value.value_type) != expected { return Err(rejected("assignment type mismatch", Some(range))); }
            assignments.push(Assignment { key: assignment.key.clone(), value });
        }
        output.push(Transition { declared_id: id.clone(), source_range: range, trigger_name: decl.trigger_button.clone(), reference_program_hash: verified.program_hash(), state_reads: decl.state_reads.clone(), state_writes: decl.state_writes.clone(), inputs: decl.inputs.clone(), assignments });
    }
    Ok(output)
}

fn lower_expr(expr: &InteractionExpr, reads: &BTreeMap<&str, InteractionType>, inputs: &BTreeMap<&str, InteractionType>) -> Result<Expression, &'static str> {
    let (value_type, operation) = match expr {
        InteractionExpr::Literal(value) => (match value { InteractionValue::Int(_) => InteractionType::Int, InteractionValue::Text(_) => InteractionType::Text, InteractionValue::Bool(_) => InteractionType::Bool }, Operation::Constant(value.clone())),
        InteractionExpr::State(key) => (*reads.get(key.as_str()).ok_or("undeclared state read")?, Operation::State(key.clone())),
        InteractionExpr::Input(key) => (*inputs.get(key.as_str()).ok_or("undeclared input read")?, Operation::Input(key.clone())),
        InteractionExpr::Effect(_) => return Err("effects are not supported by D1 lowering"),
        InteractionExpr::Add(left, right) => {
            let left = lower_expr(left, reads, inputs)?;
            let right = lower_expr(right, reads, inputs)?;
            if left.value_type != InteractionType::Int || right.value_type != InteractionType::Int { return Err("addition requires Int64 operands"); }
            (InteractionType::Int, Operation::CheckedAdd(Box::new(left), Box::new(right)))
        }
    };
    Ok(Expression { value_type, operation })
}
