//! Host-side D1 fixture runner. This does not execute generated Swift.
use crate::transition::{Expression, Operation, Transition};
use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const SCENE: &str = include_str!("../conformance/d1-increment.gscene");
const CASES: &str = include_str!("../conformance/d1-cases.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    schema_version: String,
    fixture: String,
    module: String,
    action_id: String,
    trigger_id: String,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    revision: u64,
    state: BTreeMap<String, i64>,
    inputs: BTreeMap<String, i64>,
    expected: Outcome,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
enum Outcome {
    Ok {
        revision: u64,
        state: BTreeMap<String, i64>,
    },
    Error {
        error: String,
    },
}

/// A completed, narrowly scoped corpus result. No Swift or device gate is run.
pub struct CorpusReport {
    pub passed: bool,
    pub json: serde_json::Value,
}

fn target_expression(
    expr: &Expression,
    state: &BTreeMap<String, i64>,
    inputs: &BTreeMap<String, i64>,
) -> Result<i64, String> {
    match &expr.operation {
        Operation::Constant(goya_lang::InteractionValue::Int(value)) => Ok(*value),
        Operation::State(key) => state
            .get(key)
            .copied()
            .ok_or_else(|| "missing_state".into()),
        Operation::Input(key) => inputs
            .get(key)
            .copied()
            .ok_or_else(|| "missing_input".into()),
        Operation::CheckedAdd(left, right) => {
            let left = i128::from(target_expression(left, state, inputs)?);
            let right = i128::from(target_expression(right, state, inputs)?);
            i64::try_from(left + right).map_err(|_| "arithmetic_overflow".into())
        }
        _ => Err("unsupported_fixture_type".into()),
    }
}

fn target_run(action: &Transition, case: &Case) -> Outcome {
    let expected_state: std::collections::BTreeSet<_> = action
        .state_reads
        .iter()
        .chain(&action.state_writes)
        .map(|key| key.name.as_str())
        .collect();
    let expected_inputs: std::collections::BTreeSet<_> =
        action.inputs.iter().map(|key| key.name.as_str()).collect();
    if case
        .state
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>()
        != expected_state
    {
        return Outcome::Error {
            error: "state_schema".into(),
        };
    }
    if case
        .inputs
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>()
        != expected_inputs
    {
        return Outcome::Error {
            error: "input_schema".into(),
        };
    }
    let mut writes = Vec::new();
    for assignment in &action.assignments {
        match target_expression(&assignment.value, &case.state, &case.inputs) {
            Ok(value) => writes.push((assignment.key.clone(), value)),
            Err(error) => return Outcome::Error { error },
        }
    }
    let Some(revision) = case.revision.checked_add(1) else {
        return Outcome::Error {
            error: "revision_overflow".into(),
        };
    };
    let mut state = case.state.clone();
    state.extend(writes);
    Outcome::Ok { revision, state }
}

fn reference_run(action: &goya_lang::CompiledInteraction, case: &Case) -> Result<Outcome, String> {
    let values = case
        .state
        .iter()
        .map(|(k, v)| (k.clone(), D1Value::Int(*v)))
        .collect();
    let inputs = case
        .inputs
        .iter()
        .map(|(k, v)| (k.clone(), D1Value::Int(*v)))
        .collect();
    let snapshot = D1Snapshot::new(case.revision, values).map_err(|e| e.to_string())?;
    let inputs = InputSnapshot::new(inputs).map_err(|e| e.to_string())?;
    match action.execute(&snapshot, &inputs) {
        Ok(outcome) => {
            let mut state = BTreeMap::new();
            for (key, value) in outcome.next_state {
                match value {
                    D1Value::Int(value) => {
                        state.insert(key, value);
                    }
                    _ => return Err("reference produced a non-integer value".into()),
                }
            }
            Ok(Outcome::Ok {
                revision: outcome.revision_after,
                state,
            })
        }
        Err(goya_lang::TransitionError::VmRejected(error))
            if error == "integer addition overflow" =>
        {
            Ok(Outcome::Error {
                error: "arithmetic_overflow".into(),
            })
        }
        Err(goya_lang::TransitionError::VmRejected(error))
            if error == "state revision counter exhausted" =>
        {
            Ok(Outcome::Error {
                error: "revision_overflow".into(),
            })
        }
        Err(goya_lang::TransitionError::StateTypeMismatch) => Ok(Outcome::Error {
            error: "state_schema".into(),
        }),
        Err(goya_lang::TransitionError::InputTypeMismatch) => Ok(Outcome::Error {
            error: "input_schema".into(),
        }),
        Err(error) => Err(format!("unexpected reference error: {error}")),
    }
}

/// Run the pinned, hand-authored integer corpus against both implementations.
pub fn run_d1_corpus() -> Result<CorpusReport, String> {
    let corpus: Corpus = serde_json::from_str(CASES).map_err(|e| e.to_string())?;
    if corpus.schema_version != "1" || corpus.cases.is_empty() || corpus.cases.len() > 128 {
        return Err("unsupported or empty corpus".into());
    }
    let scene = goya_lang::parse_strict(SCENE)
        .map_err(|e| e.to_string())?
        .scene;
    let mut reference = goya_lang::compile_d1_interactions(&scene).map_err(|e| e.to_string())?;
    let mut target = crate::transition::lower(SCENE, &corpus.module).map_err(|e| e.message)?;
    if reference.len() != 1 || target.len() != 1 {
        return Err("corpus requires one D1 interaction".into());
    }
    let reference = reference.remove(0);
    let target = target.remove(0);
    if target.reference_program_hash != reference.program_hash()
        || target.semantic_id.declared_id != corpus.action_id
        || target.trigger_id.declared_id != corpus.trigger_id
        || target.semantic_id.module != corpus.module
        || target.trigger_id.module != corpus.module
    {
        return Err("target identity or reference program hash mismatch".into());
    }
    let mut cases = Vec::new();
    let mut passed = true;
    let mut reference_passed = true;
    let mut target_passed = true;
    let mut seen = std::collections::BTreeSet::new();
    for case in &corpus.cases {
        if case.id.is_empty() || !seen.insert(&case.id) {
            return Err("invalid or duplicate case ID".into());
        }
        let actual_reference = reference_run(&reference, case)?;
        let actual_target = target_run(&target, case);
        let reference_case = actual_reference == case.expected;
        let target_case = actual_target == case.expected;
        reference_passed &= reference_case;
        target_passed &= target_case;
        let case_passed = reference_case && target_case;
        passed &= case_passed;
        cases.push(serde_json::json!({"id":case.id,"status":if case_passed {"PASS"} else {"FAIL"},"expected":case.expected,"reference":actual_reference,"target_ir":actual_target}));
    }
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    Ok(CorpusReport {
        passed,
        json: serde_json::json!({"schema_version":"1", "suite":"d1-integer-corpus", "status":if passed {"PASS"} else {"FAIL"}, "scope":"host-side integer target IR only", "fixture":corpus.fixture, "scene_sha256":digest(SCENE.as_bytes()), "cases_sha256":digest(CASES.as_bytes()), "goya_d1_vm_version":goya_executable::D1_VM_VERSION, "reference_program_hash":reference.program_hash().iter().map(|b| format!("{b:02x}")).collect::<String>(), "stages":{"reference_executor":if reference_passed {"PASS"} else {"FAIL"}, "target_ir":if target_passed {"PASS"} else {"FAIL"}, "generated_swift":"BLOCKED", "apple_typecheck":"BLOCKED", "simulator":"BLOCKED", "physical_device":"BLOCKED", "ios_admission":"NOT_RUN"}, "cases":cases}),
    })
}
