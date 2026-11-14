//! Host-side D1 fixture runner. This does not execute generated Swift.
use crate::transition::{Expression, Operation, Transition};
use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const SCENE: &str = include_str!("../conformance/d1-increment.gscene");
const CASES: &str = include_str!("../conformance/d1-cases.json");
const TEXT_SCENE: &str = include_str!("../conformance/d1-text.gscene");
const TEXT_CASES: &str = include_str!("../conformance/d1-text-cases.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum Scalar {
    Int(i64),
    Text(String),
    Bool(bool),
}

impl Scalar {
    fn matches_type(&self, ty: goya_lang::InteractionType) -> bool {
        matches!(
            (self, ty),
            (Self::Int(_), goya_lang::InteractionType::Int)
                | (Self::Text(_), goya_lang::InteractionType::Text)
                | (Self::Bool(_), goya_lang::InteractionType::Bool)
        )
    }
    fn to_d1(&self) -> D1Value {
        match self {
            Self::Int(value) => D1Value::Int(*value),
            Self::Text(value) => D1Value::Text(value.clone()),
            Self::Bool(value) => D1Value::Bool(*value),
        }
    }
    fn from_d1(value: D1Value) -> Self {
        match value {
            D1Value::Int(value) => Self::Int(value),
            D1Value::Text(value) => Self::Text(value),
            D1Value::Bool(value) => Self::Bool(value),
        }
    }
}

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
    state: BTreeMap<String, Scalar>,
    inputs: BTreeMap<String, Scalar>,
    expected: Outcome,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
enum Outcome {
    Ok {
        revision: u64,
        state: BTreeMap<String, Scalar>,
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
    state: &BTreeMap<String, Scalar>,
    inputs: &BTreeMap<String, Scalar>,
) -> Result<Scalar, String> {
    match &expr.operation {
        Operation::Constant(goya_lang::InteractionValue::Int(value)) => Ok(Scalar::Int(*value)),
        Operation::Constant(goya_lang::InteractionValue::Text(value)) => {
            Ok(Scalar::Text(value.clone()))
        }
        Operation::Constant(goya_lang::InteractionValue::Bool(value)) => Ok(Scalar::Bool(*value)),
        Operation::State(key) => state
            .get(key)
            .cloned()
            .ok_or_else(|| "missing_state".into()),
        Operation::Input(key) => inputs
            .get(key)
            .cloned()
            .ok_or_else(|| "missing_input".into()),
        Operation::CheckedAdd(left, right) => {
            let (Scalar::Int(left), Scalar::Int(right)) = (
                target_expression(left, state, inputs)?,
                target_expression(right, state, inputs)?,
            ) else {
                return Err("type_mismatch".into());
            };
            i64::try_from(i128::from(left) + i128::from(right))
                .map(Scalar::Int)
                .map_err(|_| "arithmetic_overflow".into())
        }
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
    if action
        .state_reads
        .iter()
        .chain(&action.state_writes)
        .any(|key| {
            case.state
                .get(&key.name)
                .is_none_or(|value| !value.matches_type(key.value_type))
        })
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
    if action.inputs.iter().any(|key| {
        case.inputs
            .get(&key.name)
            .is_none_or(|value| !value.matches_type(key.value_type))
    }) {
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
        .map(|(k, v)| (k.clone(), v.to_d1()))
        .collect();
    let inputs = case
        .inputs
        .iter()
        .map(|(k, v)| (k.clone(), v.to_d1()))
        .collect();
    let snapshot = D1Snapshot::new(case.revision, values).map_err(|e| e.to_string())?;
    let inputs = InputSnapshot::new(inputs).map_err(|e| e.to_string())?;
    match action.execute(&snapshot, &inputs) {
        Ok(outcome) => {
            let mut state = BTreeMap::new();
            for (key, value) in outcome.next_state {
                state.insert(key, Scalar::from_d1(value));
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

fn application_run(
    scene: &goya_lang::Scene,
    action_id: &str,
    case: &Case,
) -> Result<Outcome, String> {
    use goya_sdk::D1ApplicationError;
    let initial = case
        .state
        .iter()
        .map(|(key, value)| (key.clone(), value.to_d1()))
        .collect();
    let mut app = match goya_sdk::D1Application::restore(scene, case.revision, initial) {
        Ok(app) => app,
        Err(D1ApplicationError::StateSchemaMismatch) => {
            return Ok(Outcome::Error {
                error: "state_schema".into(),
            })
        }
        Err(error) => {
            return Err(format!(
                "unexpected application initialization error: {error}"
            ))
        }
    };
    let before = app.values().clone();
    let revision_before = app.revision();
    let inputs = case
        .inputs
        .iter()
        .map(|(key, value)| (key.clone(), value.to_d1()))
        .collect();
    match app.dispatch(action_id, inputs) {
        Ok(()) => Ok(Outcome::Ok {
            revision: app.revision(),
            state: app
                .values()
                .iter()
                .map(|(key, value)| (key.clone(), Scalar::from_d1(value.clone())))
                .collect(),
        }),
        Err(error) => {
            if app.values() != &before || app.revision() != revision_before {
                return Err("application changed state after a failed action".into());
            }
            let normalized = match error {
                D1ApplicationError::ActionFailed(goya_lang::TransitionError::InputTypeMismatch) => {
                    "input_schema"
                }
                D1ApplicationError::ActionFailed(goya_lang::TransitionError::VmRejected(
                    message,
                )) if message == "integer addition overflow" => "arithmetic_overflow",
                D1ApplicationError::ActionFailed(goya_lang::TransitionError::VmRejected(
                    message,
                )) if message == "state revision counter exhausted" => "revision_overflow",
                other => return Err(format!("unexpected application error: {other}")),
            };
            Ok(Outcome::Error {
                error: normalized.into(),
            })
        }
    }
}

/// Run the pinned, hand-authored scalar corpus against the reference VM,
/// shared application owner, and target representation.
pub fn run_d1_corpus() -> Result<CorpusReport, String> {
    let reports = [
        run_fixture(SCENE, CASES)?,
        run_fixture(TEXT_SCENE, TEXT_CASES)?,
    ];
    let passed = reports.iter().all(|report| report.passed);
    let reference_passed = reports
        .iter()
        .all(|report| report.json["stages"]["reference_executor"] == "PASS");
    let target_passed = reports
        .iter()
        .all(|report| report.json["stages"]["target_ir"] == "PASS");
    let application_passed = reports
        .iter()
        .all(|report| report.json["stages"]["application_owner"] == "PASS");
    let mut cases = Vec::new();
    for report in &reports {
        let fixture = report.json["fixture"]
            .as_str()
            .ok_or("missing fixture ID")?;
        for case in report.json["cases"]
            .as_array()
            .ok_or("missing fixture cases")?
        {
            let mut case = case.clone();
            case.as_object_mut()
                .ok_or("invalid case report")?
                .insert("fixture".into(), serde_json::json!(fixture));
            cases.push(case);
        }
    }
    Ok(CorpusReport {
        passed,
        json: serde_json::json!({"schema_version":"2", "suite":"d1-scalar-corpus", "status":if passed {"PASS"} else {"FAIL"}, "scope":"host-side scalar application owner and target IR", "application_contract":goya_sdk::D1_APPLICATION_CONTRACT, "fixtures":reports.iter().map(|report| &report.json).collect::<Vec<_>>(), "cases":cases, "stages":{"reference_executor":if reference_passed {"PASS"} else {"FAIL"}, "application_owner":if application_passed {"PASS"} else {"FAIL"}, "target_ir":if target_passed {"PASS"} else {"FAIL"}, "generated_swift":"BLOCKED", "apple_typecheck":"BLOCKED", "simulator":"BLOCKED", "physical_device":"BLOCKED", "ios_admission":"NOT_RUN"}}),
    })
}

fn run_fixture(scene_source: &str, case_source: &str) -> Result<CorpusReport, String> {
    let corpus: Corpus = serde_json::from_str(case_source).map_err(|e| e.to_string())?;
    if corpus.schema_version != "1" || corpus.cases.is_empty() || corpus.cases.len() > 128 {
        return Err("unsupported or empty corpus".into());
    }
    let scene = goya_lang::parse_strict(scene_source)
        .map_err(|e| e.to_string())?
        .scene;
    let mut reference = goya_lang::compile_d1_interactions(&scene).map_err(|e| e.to_string())?;
    let mut target =
        crate::transition::lower(scene_source, &corpus.module).map_err(|e| e.message)?;
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
    let mut application_passed = true;
    let mut seen = std::collections::BTreeSet::new();
    for case in &corpus.cases {
        if case.id.is_empty() || !seen.insert(&case.id) {
            return Err("invalid or duplicate case ID".into());
        }
        let actual_reference = reference_run(&reference, case)?;
        let actual_target = target_run(&target, case);
        let actual_application = application_run(&scene, &corpus.action_id, case)?;
        let reference_case = actual_reference == case.expected;
        let target_case = actual_target == case.expected;
        let application_case = actual_application == case.expected;
        reference_passed &= reference_case;
        target_passed &= target_case;
        application_passed &= application_case;
        let case_passed = reference_case && target_case && application_case;
        passed &= case_passed;
        cases.push(serde_json::json!({"id":case.id,"status":if case_passed {"PASS"} else {"FAIL"},"expected":case.expected,"reference":actual_reference,"application_owner":actual_application,"target_ir":actual_target}));
    }
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    Ok(CorpusReport {
        passed,
        json: serde_json::json!({"schema_version":"1", "suite":"d1-fixture", "status":if passed {"PASS"} else {"FAIL"}, "scope":"host-side scalar application owner and target IR", "fixture":corpus.fixture, "scene_sha256":digest(scene_source.as_bytes()), "cases_sha256":digest(case_source.as_bytes()), "goya_d1_vm_version":goya_executable::D1_VM_VERSION, "reference_program_hash":reference.program_hash().iter().map(|b| format!("{b:02x}")).collect::<String>(), "stages":{"reference_executor":if reference_passed {"PASS"} else {"FAIL"}, "application_owner":if application_passed {"PASS"} else {"FAIL"}, "target_ir":if target_passed {"PASS"} else {"FAIL"}, "generated_swift":"BLOCKED", "apple_typecheck":"BLOCKED", "simulator":"BLOCKED", "physical_device":"BLOCKED", "ios_admission":"NOT_RUN"}, "cases":cases}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incorrect_expected_trace_cannot_pass() {
        let changed = CASES.replacen("\"count\":12", "\"count\":13", 1);
        assert_ne!(changed, CASES);
        let report = run_fixture(SCENE, &changed).unwrap();
        assert!(!report.passed);
        assert_eq!(report.json["cases"][0]["status"], "FAIL");
        assert_eq!(report.json["stages"]["reference_executor"], "FAIL");
        assert_eq!(report.json["stages"]["target_ir"], "FAIL");
        assert_eq!(report.json["stages"]["apple_typecheck"], "BLOCKED");
    }

    #[test]
    fn composed_and_decomposed_strings_are_not_treated_as_identical() {
        let mut cases: serde_json::Value = serde_json::from_str(TEXT_CASES).unwrap();
        cases["cases"][0]["expected"]["state"]["prior"] = serde_json::json!("é");
        let report = run_fixture(TEXT_SCENE, &cases.to_string()).unwrap();
        assert!(!report.passed);
        assert_eq!(report.json["cases"][0]["status"], "FAIL");
        assert_eq!(report.json["stages"]["reference_executor"], "FAIL");
        assert_eq!(report.json["stages"]["target_ir"], "FAIL");
    }
}
