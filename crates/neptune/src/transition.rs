//! Experimental target representation for Goya's bounded D1 transitions.
//! It is not connected to Swift emission or admitted for iOS builds.
use goya_lang::{
    CellKind, InteractionClass, InteractionExpr, InteractionType, InteractionValue, SceneCell,
    SourceRange, TypedKey,
};
use std::collections::BTreeMap;

/// Version of the experimental in-memory target contract.
pub const CONTRACT: &str = "NPT-D1-IR/1";

/// A stable source identity. Both parts are needed to identify a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticId {
    pub module: String,
    pub declared_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub semantic_id: SemanticId,
    pub source_range: SourceRange,
    pub source_digest: [u8; 32],
    pub trigger_name: String,
    pub trigger_id: SemanticId,
    pub trigger_source_range: SourceRange,
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
pub fn lower(source: &str, module: &str) -> Result<Vec<Transition>, crate::Diagnostic> {
    crate::validate_module(module)?;
    crate::inspect(source)?;
    let handoff = goya_lang::handoff::admit_v3(source)
        .map_err(|e| rejected(e.to_string(), Some(e.range)))?;
    lower_admitted(handoff.parsed(), module)
}

pub(crate) fn lower_admitted(
    parsed: &goya_lang::ParsedScene,
    module: &str,
) -> Result<Vec<Transition>, crate::Diagnostic> {
    if let Some(unsupported) = parsed
        .scene
        .interactions
        .iter()
        .find(|i| i.class != InteractionClass::D1)
    {
        let range = parsed
            .entries
            .iter()
            .find(|e| e.kind == "interaction" && e.semantic_id == unsupported.authored_id)
            .map(|e| e.range);
        return Err(rejected(
            "D2 effects are not supported by this lowering",
            range,
        ));
    }
    let reference = goya_lang::compile_d1_interactions(&parsed.scene)
        .map_err(|e| rejected(e.to_string(), None))?;
    if reference.len() != parsed.scene.interactions.len() {
        return Err(rejected("Goya interaction count mismatch", None));
    }
    let mut buttons = Vec::new();
    collect_buttons(&parsed.scene.root, &mut buttons);
    let mut output = Vec::new();
    for (decl, verified) in parsed.scene.interactions.iter().zip(reference) {
        let id = decl
            .authored_id
            .as_ref()
            .ok_or_else(|| rejected("interaction requires an authored ID", None))?;
        let origin = parsed
            .entries
            .iter()
            .find(|e| e.kind == "interaction" && e.semantic_id.as_ref() == Some(id))
            .ok_or_else(|| rejected("interaction source range unavailable", None))?;
        let range = origin.range;
        let index = usize::try_from(verified.trigger().0)
            .map_err(|_| rejected("trigger index out of range", Some(range)))?;
        let trigger = buttons
            .get(index)
            .filter(|button| button.name == decl.trigger_button)
            .ok_or_else(|| rejected("Goya trigger mapping mismatch", Some(range)))?;
        let trigger_id = trigger
            .authored_id
            .as_ref()
            .ok_or_else(|| rejected("button requires an authored ID", Some(range)))?;
        let trigger_origin = parsed
            .entries
            .iter()
            .find(|e| e.kind == "button" && e.semantic_id.as_ref() == Some(trigger_id))
            .ok_or_else(|| rejected("button source range unavailable", Some(range)))?;
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
            semantic_id: SemanticId {
                module: module.to_owned(),
                declared_id: id.clone(),
            },
            source_range: range,
            source_digest: origin.source_digest,
            trigger_name: decl.trigger_button.clone(),
            trigger_id: SemanticId {
                module: module.to_owned(),
                declared_id: trigger_id.clone(),
            },
            trigger_source_range: trigger_origin.range,
            reference_program_hash: verified.program_hash(),
            state_reads: decl.state_reads.clone(),
            state_writes: decl.state_writes.clone(),
            inputs: decl.inputs.clone(),
            assignments,
        });
    }
    Ok(output)
}

// Mirrors the Goya binding walk, which stops descending at buttons.
fn collect_buttons<'a>(cell: &'a SceneCell, output: &mut Vec<&'a SceneCell>) {
    if cell.kind == CellKind::Button {
        output.push(cell);
    } else {
        for child in &cell.children {
            collect_buttons(child, output);
        }
    }
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

    const SOURCE: &str = "@ontology app { @state { count: int = 0 session; previous: int = 0 session; } @button plus { id: \"button.plus\" } @input amountField { id: \"input.amount\" } @interaction increment { id: \"action.increment\"; trigger: button.plus.activate; class: D1; state { read count: int; write count: int; write previous: int; } input { amount: int from \"input.amount\"; } set count = state.count + input.amount; set previous = state.count; } }";

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
        let lowered = lower(SOURCE, "app/main.gscene").unwrap();
        assert_eq!(lowered.len(), 1);
        let action = &lowered[0];
        assert_eq!(
            action.semantic_id,
            SemanticId {
                module: "app/main.gscene".into(),
                declared_id: "action.increment".into()
            }
        );
        assert_eq!(
            action.trigger_id,
            SemanticId {
                module: "app/main.gscene".into(),
                declared_id: "button.plus".into()
            }
        );
        assert!(
            SOURCE[action.trigger_source_range.start..action.trigger_source_range.end]
                .starts_with("@button plus")
        );
        assert!(SOURCE[action.source_range.start..action.source_range.end]
            .starts_with("@interaction increment"));
        let handoff = goya_lang::handoff::admit_v3(SOURCE).unwrap();
        let scene = &handoff.parsed().scene;
        let reference = goya_lang::compile_d1_interactions(scene)
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

    #[test]
    fn trigger_identity_survives_an_unrelated_node_with_the_same_name() {
        let with_cell = SOURCE.replace(
            "@button plus",
            "@cell plus { id: \"cell.plus\" } @button plus",
        );
        let action = lower(&with_cell, "app/main.gscene").unwrap().remove(0);
        assert_eq!(action.trigger_id.declared_id, "button.plus");
        assert!(
            with_cell[action.trigger_source_range.start..action.trigger_source_range.end]
                .starts_with("@button plus")
        );
        assert_ne!(
            action.source_digest,
            lower(SOURCE, "app/main.gscene").unwrap()[0].source_digest
        );
        assert!(lower(SOURCE, "../main.gscene").is_err());
    }

    #[test]
    fn d2_rejection_points_to_the_unsupported_interaction() {
        let source = "@ontology app { @state { count: int = 0 session; } @button plus { id: \"button.plus\" } @button go { id: \"button.go\" } @input amountField { id: \"input.amount\" } @interaction increment { id: \"action.increment\"; trigger: button.plus.activate; class: D1; state { read count: int; write count: int; } input { amount: int from \"input.amount\"; } set count = state.count + input.amount; } @interaction fetch { id: \"action.fetch\"; trigger: button.go.activate; class: D2; effects { network; } requests { lookup: effect network capability \"network\" request \"static request\" result int64; } state { write count: int; } input { } set count = effect.lookup; } }";
        let error = lower(source, "app/main.gscene").unwrap_err();
        assert_eq!(error.code, "NPT-E2104");
        let range = error.range.unwrap();
        assert!(source[range.start..range.end].starts_with("@interaction fetch"));
    }
}
