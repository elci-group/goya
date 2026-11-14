//! Experimental target representation for Goya's bounded D1 transitions.
//! It is not connected to Swift emission or admitted for iOS builds.
use goya_lang::{
    InteractionClass, InteractionExpr, InteractionType, InteractionValue, SourceRange, TypedKey,
};
use std::collections::BTreeMap;

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
pub struct Assignment {
    pub key: String,
    pub value: Expression,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    pub value_type: InteractionType,
    pub operation: Operation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Constant(InteractionValue),
    State(String),
    Input(String),
    CheckedAdd(Box<Expression>, Box<Expression>),
}

fn rejected(message: impl Into<String>, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E2104",
        message: message.into(),
        range,
    }
}

/// Lower verified D1 declarations without inferring unsupported effect behavior.
/// Expressions read the original state/input snapshot; assignments are staged
/// and committed atomically only after every expression succeeds.
pub fn lower(source: &str) -> Result<Vec<Transition>, crate::Diagnostic> {
    crate::inspect(source)?;
    let parsed =
        goya_lang::parse_strict(source).map_err(|e| rejected(e.to_string(), Some(e.range)))?;
    if parsed
        .scene
        .interactions
        .iter()
        .any(|i| i.class != InteractionClass::D1)
    {
        let range = parsed
            .entries
            .iter()
            .find(|e| e.kind == "interaction")
            .map(|e| e.range);
        return Err(rejected(
            "D2 effects are not supported by this lowering",
            range,
        ));
    }
    let reference = goya_lang::compile_d1_interactions(&parsed.scene)
        .map_err(|e| rejected(e.to_string(), None))?;
    let mut output = Vec::new();
    for (decl, verified) in parsed.scene.interactions.iter().zip(reference) {
        let id = decl
            .authored_id
            .as_ref()
            .ok_or_else(|| rejected("interaction requires an authored ID", None))?;
        let range = parsed
            .entries
            .iter()
            .find(|e| e.kind == "interaction" && e.semantic_id.as_ref() == Some(id))
            .map(|e| e.range)
            .ok_or_else(|| rejected("interaction source range unavailable", None))?;
        let reads: BTreeMap<_, _> = decl
            .state_reads
            .iter()
            .map(|key| (key.name.as_str(), key.value_type))
            .collect();
        let inputs: BTreeMap<_, _> = decl
            .inputs
            .iter()
            .map(|key| (key.name.as_str(), key.value_type))
            .collect();
        let mut assignments = Vec::new();
        for assignment in &decl.assignments {
            let value = lower_expr(&assignment.value, &reads, &inputs)
                .map_err(|message| rejected(message, Some(range)))?;
            let expected = decl
                .state_writes
                .iter()
                .find(|key| key.name == assignment.key)
                .map(|key| key.value_type);
            if Some(value.value_type) != expected {
                return Err(rejected("assignment type mismatch", Some(range)));
            }
            assignments.push(Assignment {
                key: assignment.key.clone(),
                value,
            });
        }
        output.push(Transition {
            declared_id: id.clone(),
            source_range: range,
            trigger_name: decl.trigger_button.clone(),
            reference_program_hash: verified.program_hash(),
            state_reads: decl.state_reads.clone(),
            state_writes: decl.state_writes.clone(),
            inputs: decl.inputs.clone(),
            assignments,
        });
    }
    Ok(output)
}

fn lower_expr(
    expr: &InteractionExpr,
    reads: &BTreeMap<&str, InteractionType>,
    inputs: &BTreeMap<&str, InteractionType>,
) -> Result<Expression, &'static str> {
    let (value_type, operation) = match expr {
        InteractionExpr::Literal(value) => (
            match value {
                InteractionValue::Int(_) => InteractionType::Int,
                InteractionValue::Text(_) => InteractionType::Text,
                InteractionValue::Bool(_) => InteractionType::Bool,
            },
            Operation::Constant(value.clone()),
        ),
        InteractionExpr::State(key) => (
            *reads.get(key.as_str()).ok_or("undeclared state read")?,
            Operation::State(key.clone()),
        ),
        InteractionExpr::Input(key) => (
            *inputs.get(key.as_str()).ok_or("undeclared input read")?,
            Operation::Input(key.clone()),
        ),
        InteractionExpr::Effect(_) => return Err("effects are not supported by D1 lowering"),
        InteractionExpr::Add(left, right) => {
            let left = lower_expr(left, reads, inputs)?;
            let right = lower_expr(right, reads, inputs)?;
            if left.value_type != InteractionType::Int || right.value_type != InteractionType::Int {
                return Err("addition requires Int64 operands");
            }
            (
                InteractionType::Int,
                Operation::CheckedAdd(Box::new(left), Box::new(right)),
            )
        }
    };
    Ok(Expression {
        value_type,
        operation,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_executable::{D1Snapshot, D1Value, InputSnapshot};

    const SOURCE: &str = "@ontology app { @button plus { id: \"button.plus\" } @interaction increment { id: \"action.increment\"; trigger: button.plus.activate; class: D1; state { read count: int; write count: int; write previous: int; } input { amount: int; } set count = state.count + input.amount; set previous = state.count; } }";

    // Independent, test-only target interpreter. It intentionally does not
    // share the D1 VM's instruction execution or overflow implementation.
    fn eval(
        expr: &Expression,
        state: &BTreeMap<String, D1Value>,
        inputs: &BTreeMap<String, D1Value>,
    ) -> Result<D1Value, ()> {
        match &expr.operation {
            Operation::Constant(InteractionValue::Int(v)) => Ok(D1Value::Int(*v)),
            Operation::Constant(InteractionValue::Text(v)) => Ok(D1Value::Text(v.clone())),
            Operation::Constant(InteractionValue::Bool(v)) => Ok(D1Value::Bool(*v)),
            Operation::State(key) => state.get(key).cloned().ok_or(()),
            Operation::Input(key) => inputs.get(key).cloned().ok_or(()),
            Operation::CheckedAdd(left, right) => {
                let (D1Value::Int(a), D1Value::Int(b)) =
                    (eval(left, state, inputs)?, eval(right, state, inputs)?)
                else {
                    return Err(());
                };
                let result = i128::from(a) + i128::from(b);
                Ok(D1Value::Int(i64::try_from(result).map_err(|_| ())?))
            }
        }
    }

    #[test]
    fn target_trace_matches_goya_snapshot_and_overflow_behavior() {
        let lowered = lower(SOURCE).unwrap();
        assert_eq!(lowered.len(), 1);
        let action = &lowered[0];
        assert_eq!(action.declared_id, "action.increment");
        assert!(SOURCE[action.source_range.start..action.source_range.end]
            .starts_with("@interaction increment"));
        let scene = goya_lang::parse_strict(SOURCE).unwrap().scene;
        let reference = goya_lang::compile_d1_interactions(&scene)
            .unwrap()
            .remove(0);
        assert_eq!(action.reference_program_hash, reference.program_hash());
        for (count, amount) in [(9, 3), (-8, 3), (i64::MAX, 1)] {
            let state = BTreeMap::from([
                ("count".into(), D1Value::Int(count)),
                ("previous".into(), D1Value::Int(0)),
            ]);
            let inputs = BTreeMap::from([("amount".into(), D1Value::Int(amount))]);
            let original = state.clone();
            let target = action
                .assignments
                .iter()
                .map(|a| eval(&a.value, &state, &inputs).map(|v| (a.key.clone(), v)))
                .collect::<Result<Vec<_>, _>>();
            let outcome = reference.execute(
                &D1Snapshot::new(7, state.clone()).unwrap(),
                &InputSnapshot::new(inputs).unwrap(),
            );
            match (target, outcome) {
                (Ok(writes), Ok(outcome)) => {
                    let mut next = state;
                    next.extend(writes);
                    assert_eq!(next, outcome.next_state);
                    assert_eq!(outcome.revision_after, 8);
                }
                (Err(()), Err(_)) => assert_eq!(state, original),
                other => panic!("target and reference disagree: {other:?}"),
            }
        }
    }
}
