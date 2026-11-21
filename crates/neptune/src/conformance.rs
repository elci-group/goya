//! Host-side D1 fixture runner with optional generated Swift execution.
use crate::transition::{Expression, Operation, Transition};
use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

const MAX_TEXT_BYTES: usize = goya_executable::D1_MAX_TEXT_BYTES;
const MAX_SNAPSHOT_BYTES: usize = goya_executable::D1_MAX_SNAPSHOT_BYTES;
const MAX_REGISTER_BYTES: usize = goya_executable::D1_MAX_REGISTER_BYTES;

fn value_size(value: &Scalar) -> usize {
    match value {
        Scalar::Int(_) => goya_executable::D1_INT_VALUE_BYTES,
        Scalar::Text(text) => text.len(),
        Scalar::Bool(_) => goya_executable::D1_BOOL_VALUE_BYTES,
    }
}

fn map_size(values: &BTreeMap<String, Scalar>) -> Option<usize> {
    values.iter().try_fold(
        goya_executable::D1_SNAPSHOT_HEADER_BYTES,
        |size, (key, value)| {
            size.checked_add(goya_executable::D1_SNAPSHOT_ENTRY_BYTES)?
                .checked_add(key.len())?
                .checked_add(value_size(value))
        },
    )
}

fn map_error(values: &BTreeMap<String, Scalar>) -> Option<&'static str> {
    if values
        .values()
        .any(|value| matches!(value, Scalar::Text(text) if text.len() > MAX_TEXT_BYTES))
    {
        return Some("text_limit");
    }
    if map_size(values).is_none_or(|size| size > MAX_SNAPSHOT_BYTES) {
        return Some("snapshot_limit");
    }
    None
}

fn scalar_error(value: &Scalar) -> Option<&'static str> {
    matches!(value, Scalar::Text(text) if text.len() > MAX_TEXT_BYTES).then_some("text_limit")
}

fn register_cost(name: &str, value: &Scalar) -> Option<usize> {
    name.len().checked_add(value_size(value))
}

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

/// A completed, narrowly scoped host corpus result.
pub struct CorpusReport {
    pub passed: bool,
    pub json: serde_json::Value,
}

fn target_expression(
    expr: &Expression,
    state: &BTreeMap<String, Scalar>,
    inputs: &BTreeMap<String, Scalar>,
    registers: &mut BTreeMap<String, Scalar>,
    register_bytes: &mut usize,
) -> Result<Scalar, String> {
    let value = match &expr.operation {
        Operation::Constant(goya_lang::InteractionValue::Int(value)) => Scalar::Int(*value),
        Operation::Constant(goya_lang::InteractionValue::Text(value)) => {
            Scalar::Text(value.clone())
        }
        Operation::Constant(goya_lang::InteractionValue::Bool(value)) => Scalar::Bool(*value),
        Operation::State(key) => state
            .get(key)
            .cloned()
            .ok_or_else(|| "missing_state".to_owned())?,
        Operation::Input(key) => inputs
            .get(key)
            .cloned()
            .ok_or_else(|| "missing_input".to_owned())?,
        Operation::CheckedAdd(left, right) => {
            let (Scalar::Int(left), Scalar::Int(right)) = (
                target_expression(left, state, inputs, registers, register_bytes)?,
                target_expression(right, state, inputs, registers, register_bytes)?,
            ) else {
                return Err("type_mismatch".into());
            };
            i64::try_from(i128::from(left) + i128::from(right))
                .map(Scalar::Int)
                .map_err(|_| "arithmetic_overflow".to_owned())?
        }
    };
    if let Some(error) = scalar_error(&value) {
        return Err(error.into());
    }
    let register_name = format!("r{}", registers.len());
    let cost = register_cost(&register_name, &value).ok_or("register_limit")?;
    let total = register_bytes.checked_add(cost).ok_or("register_limit")?;
    if total > MAX_REGISTER_BYTES {
        return Err("register_limit".into());
    }
    registers.insert(register_name, value.clone());
    *register_bytes = total;
    Ok(value)
}

fn target_run(action: &Transition, case: &Case) -> Outcome {
    if let Some(error) = map_error(&case.state).or_else(|| map_error(&case.inputs)) {
        return Outcome::Error {
            error: error.into(),
        };
    }
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
    let combined = map_size(&case.state)
        .and_then(|state| map_size(&case.inputs).and_then(|inputs| state.checked_add(inputs)));
    if combined.is_none_or(|size| size > MAX_SNAPSHOT_BYTES) {
        return Outcome::Error {
            error: "snapshot_limit".into(),
        };
    }
    let mut registers = BTreeMap::new();
    let mut register_bytes = 0;
    let mut writes = Vec::new();
    for assignment in &action.assignments {
        match target_expression(
            &assignment.value,
            &case.state,
            &case.inputs,
            &mut registers,
            &mut register_bytes,
        ) {
            Ok(value) => writes.push((assignment.key.clone(), value)),
            Err(error) => return Outcome::Error { error },
        }
    }
    let mut state = case.state.clone();
    state.extend(writes);
    if let Some(error) = map_error(&state) {
        return Outcome::Error {
            error: error.into(),
        };
    }
    let Some(revision) = case.revision.checked_add(1) else {
        return Outcome::Error {
            error: "revision_overflow".into(),
        };
    };
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
    let snapshot = match D1Snapshot::new(case.revision, values) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Outcome::Error {
                error: normalize_d1_error(error)?,
            })
        }
    };
    let inputs = match InputSnapshot::new(inputs) {
        Ok(value) => value,
        Err(error) => {
            return Ok(Outcome::Error {
                error: normalize_d1_error(error)?,
            })
        }
    };
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
        Err(goya_lang::TransitionError::VmRejected(error)) => match normalize_vm_error(&error) {
            Some(error) => Ok(Outcome::Error {
                error: error.into(),
            }),
            None => Err(format!("unexpected reference error: {error}")),
        },
        Err(error) => Err(format!("unexpected reference error: {error}")),
    }
}

fn normalize_vm_error(error: &str) -> Option<&'static str> {
    match error {
        "text state value exceeds the byte limit" => Some("text_limit"),
        "snapshot exceeds the byte limit" => Some("snapshot_limit"),
        "D1 registers exceed the byte limit" => Some("register_limit"),
        _ => None,
    }
}

fn normalize_d1_error(error: goya_executable::D1Error) -> Result<String, String> {
    match error {
        goya_executable::D1Error::TextTooLarge => Ok("text_limit".into()),
        goya_executable::D1Error::SnapshotTooLarge => Ok("snapshot_limit".into()),
        goya_executable::D1Error::RegisterLimit => Ok("register_limit".into()),
        other => Err(format!("unexpected D1 construction error: {other}")),
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
        Err(D1ApplicationError::InvalidState(error))
        | Err(D1ApplicationError::InvalidInput(error)) => {
            return Ok(Outcome::Error {
                error: normalize_d1_error(error)?,
            });
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
            if let D1ApplicationError::InvalidState(error)
            | D1ApplicationError::InvalidInput(error) = error
            {
                return Ok(Outcome::Error {
                    error: normalize_d1_error(error)?,
                });
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
                D1ApplicationError::ActionFailed(goya_lang::TransitionError::VmRejected(
                    message,
                )) => match normalize_vm_error(&message) {
                    Some(error) => error,
                    None => return Err(format!("unexpected application error: {message}")),
                },
                other => return Err(format!("unexpected application error: {other}")),
            };
            Ok(Outcome::Error {
                error: normalized.into(),
            })
        }
    }
}

fn swift_arguments(
    fields: &[crate::transition_swift::Field],
    values: &BTreeMap<String, Scalar>,
) -> Option<Vec<String>> {
    if fields.len() != values.len() {
        return None;
    }
    fields
        .iter()
        .map(|field| {
            let value = values.get(&field.source_key)?;
            if !value.matches_type(field.value_type) {
                return None;
            }
            let literal = match value {
                Scalar::Int(value) => value.to_string(),
                Scalar::Bool(value) => value.to_string(),
                Scalar::Text(value) => crate::swift::Expr::StringLiteral(value.clone()).render(),
            };
            Some(format!("{}: {literal}", field.swift_name))
        })
        .collect()
}

fn run_swift_fixture(
    swift: &Path,
    source: &str,
    corpus: &Corpus,
) -> Result<Vec<Option<Outcome>>, String> {
    let preview = crate::transition_swift::preview(source, &corpus.module)
        .map_err(|diagnostic| diagnostic.message)?;
    let mut script = format!("import Foundation\n{}", preview.swift);
    script.push_str(
        r#"
func runCase(_ state: NeptuneState, _ input: NeptuneInput) throws {
    let outcome: [String: Any]
    do {
        let next = try NeptuneTransition.apply(state: state, input: input)
        outcome = ["status": "ok", "revision": next.revision, "state": [
"#,
    );
    for field in &preview.state_fields {
        let key = crate::swift::Expr::StringLiteral(field.source_key.clone()).render();
        let _ = writeln!(script, "            {key}: next.{},", field.swift_name);
    }
    if preview.state_fields.is_empty() {
        script.push_str("            :\n");
    }
    script.push_str(
        r#"        ] as [String: Any]]
    } catch let error as NeptuneTransitionError {
        let code: String
        switch error {
        case .arithmeticOverflow: code = "arithmetic_overflow"
        case .revisionOverflow: code = "revision_overflow"
        case .textTooLarge: code = "text_limit"
        case .snapshotTooLarge: code = "snapshot_limit"
        case .registerLimit: code = "register_limit"
        }
        outcome = ["status": "error", "error": code]
    }
    let encoded = try JSONSerialization.data(withJSONObject: outcome)
    print(String(decoding: encoded, as: UTF8.self))
}
"#,
    );
    let mut runnable = Vec::new();
    for case in &corpus.cases {
        // The typed transition cannot represent a missing or wrong-type field.
        let arguments = swift_arguments(&preview.state_fields, &case.state)
            .zip(swift_arguments(&preview.input_fields, &case.inputs));
        runnable.push(arguments.is_some());
        if let Some((mut state, input)) = arguments {
            state.insert(0, format!("revision: {}", case.revision));
            let _ = writeln!(
                script,
                "try runCase(NeptuneState({}), NeptuneInput({}))",
                state.join(", "),
                input.join(", ")
            );
        }
    }
    if !runnable.iter().any(|case| *case) {
        return Err(format!("{} has no typed Swift cases", corpus.fixture));
    }
    let mut child = Command::new(swift)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", swift.display()))?;
    let input = child.stdin.take().ok_or("Swift stdin unavailable")?;
    let (output, written) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || (&input).write_all(script.as_bytes()));
        (child.wait_with_output(), writer.join())
    });
    let output = output.map_err(|error| format!("could not wait for Swift: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Swift fixture {} exited with {}: {}",
            corpus.fixture,
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    written
        .map_err(|_| "Swift input thread failed")?
        .map_err(|error| format!("could not send fixture to Swift: {error}"))?;
    let stdout = std::str::from_utf8(&output.stdout).map_err(|error| error.to_string())?;
    let mut lines = stdout.lines();
    let outcomes = runnable
        .into_iter()
        .map(|run| {
            if !run {
                return Ok(None);
            }
            let line = lines.next().ok_or("missing generated Swift result")?;
            serde_json::from_str(line)
                .map(Some)
                .map_err(|error| format!("invalid generated Swift result: {error}"))
        })
        .collect::<Result<_, String>>()?;
    if lines.next().is_some() {
        return Err("unexpected extra generated Swift result".into());
    }
    Ok(outcomes)
}

/// Run the pinned, hand-authored scalar corpus against the reference VM,
/// shared application owner, and target representation.
pub fn run_d1_corpus() -> Result<CorpusReport, String> {
    run_corpus(None)
}

/// Also execute the generated, SwiftUI-free transition for typed cases.
pub fn run_d1_corpus_with_swift(swift: &Path) -> Result<CorpusReport, String> {
    run_corpus(Some(swift))
}

fn run_corpus(swift: Option<&Path>) -> Result<CorpusReport, String> {
    let reports = [
        run_fixture_with_swift(SCENE, CASES, swift)?,
        run_fixture_with_swift(TEXT_SCENE, TEXT_CASES, swift)?,
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
    let swift_status = if swift.is_none() {
        "BLOCKED"
    } else if reports
        .iter()
        .all(|report| report.json["stages"]["generated_swift"] == "PASS")
    {
        "PASS"
    } else {
        "FAIL"
    };
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
        json: serde_json::json!({"schema_version":"2", "suite":"d1-scalar-corpus", "status":if passed {"PASS"} else {"FAIL"}, "scope":corpus_scope(swift), "application_contract":goya_sdk::D1_APPLICATION_CONTRACT, "fixtures":reports.iter().map(|report| &report.json).collect::<Vec<_>>(), "cases":cases, "stages":{"reference_executor":if reference_passed {"PASS"} else {"FAIL"}, "application_owner":if application_passed {"PASS"} else {"FAIL"}, "target_ir":if target_passed {"PASS"} else {"FAIL"}, "generated_swift":swift_status, "apple_typecheck":"BLOCKED", "simulator":"BLOCKED", "physical_device":"BLOCKED", "ios_admission":"NOT_RUN"}}),
    })
}

fn corpus_scope(swift: Option<&Path>) -> &'static str {
    if swift.is_some() {
        "host-side scalar application owner, target IR, and typed Swift transition"
    } else {
        "host-side scalar application owner and target IR"
    }
}

fn run_fixture_with_swift(
    scene_source: &str,
    case_source: &str,
    swift: Option<&Path>,
) -> Result<CorpusReport, String> {
    let corpus: Corpus = serde_json::from_str(case_source).map_err(|e| e.to_string())?;
    if corpus.schema_version != "1" || corpus.cases.is_empty() || corpus.cases.len() > 128 {
        return Err("unsupported or empty corpus".into());
    }
    let handoff = goya_lang::handoff::admit_v3(scene_source).map_err(|e| e.to_string())?;
    let scene = &handoff.parsed().scene;
    let mut reference = goya_lang::compile_d1_interactions(scene).map_err(|e| e.to_string())?;
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
    let mut swift_passed = true;
    let swift_outcomes = swift
        .map(|swift| run_swift_fixture(swift, scene_source, &corpus))
        .transpose()?;
    let mut seen = std::collections::BTreeSet::new();
    for (index, case) in corpus.cases.iter().enumerate() {
        if case.id.is_empty() || !seen.insert(&case.id) {
            return Err("invalid or duplicate case ID".into());
        }
        let actual_reference = reference_run(&reference, case)?;
        let actual_target = target_run(&target, case);
        let actual_application = application_run(scene, &corpus.action_id, case)?;
        let reference_case = actual_reference == case.expected;
        let target_case = actual_target == case.expected;
        let application_case = actual_application == case.expected;
        reference_passed &= reference_case;
        target_passed &= target_case;
        application_passed &= application_case;
        let actual_swift = swift_outcomes
            .as_ref()
            .and_then(|outcomes| outcomes[index].as_ref());
        let swift_case = actual_swift.is_none_or(|actual| *actual == case.expected);
        swift_passed &= swift_case;
        let case_passed = reference_case && target_case && application_case && swift_case;
        passed &= case_passed;
        let mut case_data = serde_json::json!({"id":case.id,"status":if case_passed {"PASS"} else {"FAIL"},"expected":case.expected,"reference":actual_reference,"application_owner":actual_application,"target_ir":actual_target});
        if swift.is_some() {
            case_data["generated_swift"] = actual_swift.map_or_else(
                || serde_json::json!({"status":"NOT_APPLICABLE","reason":"case cannot be represented by the typed transition"}),
                |actual| serde_json::json!({"status":if swift_case {"PASS"} else {"FAIL"}, "outcome":actual}),
            );
        }
        cases.push(case_data);
    }
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let swift_status = match (swift, swift_passed) {
        (None, _) => "BLOCKED",
        (Some(_), true) => "PASS",
        (Some(_), false) => "FAIL",
    };
    Ok(CorpusReport {
        passed,
        json: serde_json::json!({"schema_version":"1", "suite":"d1-fixture", "status":if passed {"PASS"} else {"FAIL"}, "scope":corpus_scope(swift), "fixture":corpus.fixture, "scene_sha256":digest(scene_source.as_bytes()), "cases_sha256":digest(case_source.as_bytes()), "goya_d1_vm_version":goya_executable::D1_VM_VERSION, "reference_program_hash":reference.program_hash().iter().map(|b| format!("{b:02x}")).collect::<String>(), "stages":{"reference_executor":if reference_passed {"PASS"} else {"FAIL"}, "application_owner":if application_passed {"PASS"} else {"FAIL"}, "target_ir":if target_passed {"PASS"} else {"FAIL"}, "generated_swift":swift_status, "apple_typecheck":"BLOCKED", "simulator":"BLOCKED", "physical_device":"BLOCKED", "ios_admission":"NOT_RUN"}, "cases":cases}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_fixture(scene_source: &str, case_source: &str) -> Result<CorpusReport, String> {
        run_fixture_with_swift(scene_source, case_source, None)
    }

    #[test]
    fn byte_accounting_uses_utf8_and_exact_snapshot_boundaries() {
        let exact_text = Scalar::Text("é".repeat(MAX_TEXT_BYTES / 2));
        assert_eq!(scalar_error(&exact_text), None);
        let over_text = Scalar::Text(format!(
            "{}é",
            match &exact_text {
                Scalar::Text(s) => s,
                _ => unreachable!(),
            }
        ));
        assert_eq!(scalar_error(&over_text), Some("text_limit"));

        let mut exact = BTreeMap::new();
        for i in 0..16 {
            exact.insert(format!("k{i}"), Scalar::Text("x".repeat(64_000)));
        }
        assert!(map_size(&exact).unwrap() < MAX_SNAPSHOT_BYTES);
        let mut over = exact;
        over.insert("k16".into(), Scalar::Text("x".repeat(64_000)));
        assert_eq!(map_error(&over), Some("snapshot_limit"));
    }

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
