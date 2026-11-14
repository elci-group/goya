//! Experimental multi-action Swift state owner. This is not a UI or app build.
use crate::{
    application, swift,
    transition::SemanticId,
    transition_swift::{self, Field},
};
use goya_executable::D1Value;
use goya_lang::SourceRange;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const MAX_ACTIONS: usize = 64;
const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionMapping {
    id: SemanticId,
    trigger: SemanticId,
    method: String,
    source: SourceRange,
    trigger_source: SourceRange,
    reference_program_hash: [u8; 32],
    swift: SourceRange,
    inputs: Vec<Field>,
}

/// Source-only preview. Fields are private so validation cannot be forged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    swift: String,
    module: String,
    source_digest: [u8; 32],
    initial_digest: [u8; 32],
    state: Vec<Field>,
    actions: Vec<ActionMapping>,
}

fn type_name(ty: goya_lang::InteractionType) -> &'static str {
    match ty {
        goya_lang::InteractionType::Int => "int",
        goya_lang::InteractionType::Text => "text",
        goya_lang::InteractionType::Bool => "bool",
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Preview {
    pub fn swift(&self) -> &str {
        &self.swift
    }
    pub fn to_json(&self) -> serde_json::Value {
        fn fields(values: &[Field]) -> Vec<serde_json::Value> {
            values.iter().map(|field| serde_json::json!({"source_key":field.source_key,"swift_name":field.swift_name,"type":type_name(field.value_type)})).collect()
        }
        let actions: Vec<_> = self.actions.iter().map(|action| serde_json::json!({"semantic_id":{"module":action.id.module,"declared_id":action.id.declared_id},"trigger":{"module":action.trigger.module,"declared_id":action.trigger.declared_id},"method":action.method,"inputs":fields(&action.inputs),"source_range":{"start_byte":action.source.start,"end_byte":action.source.end},"trigger_source_range":{"start_byte":action.trigger_source.start,"end_byte":action.trigger_source.end},"swift_range":{"start_byte":action.swift.start,"end_byte":action.swift.end},"reference_program_hash":hex(&action.reference_program_hash),"expression_provenance":"action_range_only"})).collect();
        serde_json::json!({"schema_version":"1","operation":"preview-d1-owner","verification":"SOURCE_PREVIEW","scope":"state_only","swift":self.swift,"swift_sha256":format!("{:x}", Sha256::digest(self.swift.as_bytes())),"module":self.module,"source_sha256":hex(&self.source_digest),"initial_state_sha256":hex(&self.initial_digest),"initial_hash_contract":"NPT-D1-INIT/1","initial_source":"caller_supplied_transport","application_contract":goya_sdk::D1_APPLICATION_CONTRACT,"state_fields":fields(&self.state),"actions":actions,"stages":{"presentation_validation":"NOT_RUN","apple_validation":"NOT_RUN","runtime_limit_parity":"NOT_VERIFIED","ios_admission":"NOT_RUN"}})
    }
}

fn error(message: impl Into<String>, range: Option<SourceRange>) -> crate::Diagnostic {
    crate::Diagnostic {
        code: "NPT-E3102",
        message: message.into(),
        range,
    }
}

fn literal(value: &D1Value) -> String {
    match value {
        D1Value::Int(i64::MIN) => "Int64.min".into(),
        D1Value::Int(value) => value.to_string(),
        D1Value::Text(value) => swift::Expr::StringLiteral(value.clone()).render(),
        D1Value::Bool(value) => value.to_string(),
    }
}

// Length-prefixed UTF-8 keys and values in sorted schema order. Integer values
// are signed little-endian; strings are not normalized.
fn initial_digest(cells: &[application::StateCell]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"NPT-D1-INIT/1\0");
    hash.update((cells.len() as u64).to_le_bytes());
    for cell in cells {
        hash.update((cell.key.len() as u64).to_le_bytes());
        hash.update(cell.key.as_bytes());
        match &cell.initial {
            D1Value::Int(value) => { hash.update([0]); hash.update(value.to_le_bytes()); },
            D1Value::Text(value) => { hash.update([1]); hash.update((value.len() as u64).to_le_bytes()); hash.update(value.as_bytes()); },
            D1Value::Bool(value) => { hash.update([2, u8::from(*value)]); },
        }
    }
    hash.finalize().into()
}

/// Generate a single main-actor state owner for a validated D1 state model.
/// No view, persistence adapter, Apple validation, or release admission is run.
pub fn preview(
    source: &str,
    module: &str,
    initial: BTreeMap<String, D1Value>,
) -> Result<Preview, crate::Diagnostic> {
    if goya_executable::D1_VM_VERSION != 1 || goya_lang::LANG_CONTRACT != "GY-LANG/1" {
        return Err(error("unsupported Goya language or D1 VM version", None));
    }
    let model = application::lower(source, module, initial)?;
    if model.actions().is_empty() || model.actions().len() > MAX_ACTIONS {
        return Err(error(
            "state-owner preview requires 1 to 64 D1 actions",
            None,
        ));
    }
    let state: Vec<Field> = model
        .cells()
        .iter()
        .enumerate()
        .map(|(i, cell)| Field {
            source_key: cell.key.clone(),
            swift_name: format!("s{i}"),
            value_type: cell.value_type,
        })
        .collect();
    let state_indexes: BTreeMap<_, _> = state
        .iter()
        .enumerate()
        .map(|(i, field)| (field.source_key.clone(), i))
        .collect();
    let mut out = String::from("import SwiftUI\n\nenum NeptuneTransitionError: Error {\n    case arithmeticOverflow\n    case revisionOverflow\n    case textTooLarge\n    case snapshotTooLarge\n    case registerLimit\n}\n\nstruct NeptuneState {\n    var revision: UInt64\n");
    for field in &state {
        let _ = writeln!(
            out,
            "    var {}: {}",
            field.swift_name,
            transition_swift::swift_type(field.value_type)
        );
    }
    out.push_str("}\n\n");
    let mut action_inputs = Vec::new();
    for (index, action) in model.actions().iter().enumerate() {
        let ordered: BTreeMap<_, _> = action
            .inputs
            .iter()
            .map(|key| (key.name.clone(), key.value_type))
            .collect();
        let fields: Vec<Field> = ordered
            .into_iter()
            .enumerate()
            .map(|(i, (key, ty))| Field {
                source_key: key,
                swift_name: format!("i{i}"),
                value_type: ty,
            })
            .collect();
        let _ = writeln!(out, "struct NeptuneInput{index} {{");
        for field in &fields {
            let _ = writeln!(
                out,
                "    var {}: {}",
                field.swift_name,
                transition_swift::swift_type(field.value_type)
            );
        }
        out.push_str("}\n\n");
        action_inputs.push(fields);
    }
    out.push_str("@MainActor\nfinal class NeptuneStore {\n    private(set) var state: NeptuneState\n\n    init() {\n        self.state = NeptuneState(revision: 0");
    for (index, cell) in model.cells().iter().enumerate() {
        let _ = write!(out, ", s{index}: {}", literal(&cell.initial));
    }
    out.push_str(")\n    }\n\n    private func checkedAdd(_ lhs: Int64, _ rhs: Int64) throws -> Int64 {\n        let (value, overflow) = lhs.addingReportingOverflow(rhs)\n        guard !overflow else { throw NeptuneTransitionError.arithmeticOverflow }\n        return value\n    }\n");
    let mut mappings = Vec::new();
    for (index, action) in model.actions().iter().enumerate() {
        let start = out.len();
        let _ = write!(
            out,
            "\n    func dispatch{index}(input: NeptuneInput{index}) throws {{\n"
        );
        let touched: BTreeSet<_> = action
            .state_reads
            .iter()
            .chain(&action.state_writes)
            .map(|key| key.name.as_str())
            .collect();
        let touched_fields: Vec<_> = state
            .iter()
            .filter(|field| touched.contains(field.source_key.as_str()))
            .cloned()
            .collect();
        if touched_fields.len() != touched.len() {
            return Err(error(
                "action references unknown application state",
                Some(action.source_range),
            ));
        }
        let inputs = &action_inputs[index];
        let input_indexes: BTreeMap<_, _> = inputs
            .iter()
            .enumerate()
            .map(|(i, field)| (field.source_key.clone(), i))
            .collect();
        transition_swift::emit_snapshot_check(&mut out, "state", "state", &touched_fields);
        transition_swift::emit_snapshot_check(&mut out, "input", "input", inputs);
        let _ = writeln!(out, "        guard stateBytes + inputBytes <= {} else {{ throw NeptuneTransitionError.snapshotTooLarge }}", goya_executable::D1_MAX_SNAPSHOT_BYTES);
        out.push_str("        var registerBytes = 0\n");
        let mut next_register = 0usize;
        let mut writes = Vec::new();
        for (assignment_index, assignment) in action.assignments.iter().enumerate() {
            let slot = *state_indexes.get(&assignment.key).ok_or_else(|| {
                error(
                    "assignment has no application state slot",
                    Some(action.source_range),
                )
            })?;
            let expr = transition_swift::convert(&assignment.value, &state_indexes, &input_indexes)
                .map_err(|e| error(e, Some(action.source_range)))?;
            let register = expr.emit_register(&mut out, &mut next_register);
            let _ = writeln!(
                out,
                "        let value{assignment_index}: {} = r{register}",
                transition_swift::swift_type(assignment.value.value_type)
            );
            writes.push(slot);
        }
        out.push_str("        var next = state\n");
        for (assignment_index, slot) in writes.iter().enumerate() {
            let _ = writeln!(out, "        next.s{slot} = value{assignment_index}");
        }
        transition_swift::emit_snapshot_check(&mut out, "next", "projectedNext", &touched_fields);
        out.push_str("        let (revision, overflow) = state.revision.addingReportingOverflow(1)\n        guard !overflow else { throw NeptuneTransitionError.revisionOverflow }\n        next.revision = revision\n");
        transition_swift::emit_snapshot_check(&mut out, "next", "globalNext", &state);
        out.push_str("        state = next\n    }\n");
        mappings.push(ActionMapping {
            id: action.semantic_id.clone(),
            trigger: action.trigger_id.clone(),
            method: format!("dispatch{index}"),
            source: action.source_range,
            trigger_source: action.trigger_source_range,
            reference_program_hash: action.reference_program_hash,
            swift: SourceRange {
                start,
                end: out.len(),
            },
            inputs: inputs.clone(),
        });
        if out.len() > MAX_OUTPUT_BYTES {
            return Err(error(
                "generated state-owner source exceeds 2 MiB",
                Some(action.source_range),
            ));
        }
    }
    out.push_str("}\n");
    if out.len() > MAX_OUTPUT_BYTES {
        return Err(error("generated state-owner source exceeds 2 MiB", None));
    }
    Ok(Preview {
        swift: out,
        module: model.module().to_owned(),
        source_digest: model.source_digest(),
        initial_digest: initial_digest(model.cells()),
        state,
        actions: mappings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "@ontology app { @cell root { id: \"root\" @button inc { id: \"button.inc\" } @button rename { id: \"button.rename\" } } @interaction rename { id: \"action.rename\"; trigger: button.rename.activate; class: D1; state { write name: text; } input { value: text; } set name = input.value; } @interaction add { id: \"action.add\"; trigger: button.inc.activate; class: D1; state { read count: int; write count: int; } input { amount: int; } set count = state.count + input.amount; } }";
    fn initial() -> BTreeMap<String, D1Value> {
        BTreeMap::from([
            ("count".into(), D1Value::Int(4)),
            ("name".into(), D1Value::Text("e\u{301}".into())),
        ])
    }

    #[test]
    fn multiple_actions_share_one_main_actor_owner_and_keep_snapshot_limits() {
        let output = preview(SOURCE, "app/main.gscene", initial()).unwrap();
        assert_eq!(
            output,
            preview(SOURCE, "app/main.gscene", initial()).unwrap()
        );
        let swift = output.swift();
        assert!(swift.contains("@MainActor\nfinal class NeptuneStore"));
        assert!(swift.contains("self.state = NeptuneState(revision: 0, s0: 4, s1: \"e\u{301}\")"));
        assert_eq!(
            output
                .actions
                .iter()
                .map(|a| a.id.declared_id.as_str())
                .collect::<Vec<_>>(),
            ["action.add", "action.rename"]
        );
        assert_eq!(
            output
                .state
                .iter()
                .map(|f| f.source_key.as_str())
                .collect::<Vec<_>>(),
            ["count", "name"]
        );
        for action in &output.actions {
            let method = &swift[action.swift.start..action.swift.end];
            assert!(method.contains(&format!("func {}(", action.method)));
            assert!(
                method.find("projectedNextBytes").unwrap()
                    < method
                        .find("state.revision.addingReportingOverflow")
                        .unwrap()
            );
            assert!(
                method
                    .find("state.revision.addingReportingOverflow")
                    .unwrap()
                    < method.find("globalNextBytes").unwrap()
            );
            assert!(method.find("globalNextBytes").unwrap() < method.find("state = next").unwrap());
        }
        assert!(swift.contains("let stateBytes = 29"));
        assert!(swift.contains("let stateBytes = 20 + state.s1.utf8.count"));
        let report = output.to_json();
        assert_eq!(report["scope"], "state_only");
        assert_eq!(report["stages"]["apple_validation"], "NOT_RUN");
        assert_eq!(report["stages"]["runtime_limit_parity"], "NOT_VERIFIED");
        assert_eq!(report["initial_hash_contract"], "NPT-D1-INIT/1");
        assert_eq!(report["actions"][0]["reference_program_hash"].as_str().unwrap().len(), 64);
        assert_eq!(report["swift_sha256"], format!("{:x}", Sha256::digest(swift.as_bytes())));
        let scene = goya_lang::parse_strict(SOURCE).unwrap().scene;
        let mut reference = goya_sdk::D1Application::new(&scene, initial()).unwrap();
        reference
            .dispatch(
                "action.add",
                BTreeMap::from([("amount".into(), D1Value::Int(3))]),
            )
            .unwrap();
        reference
            .dispatch(
                "action.rename",
                BTreeMap::from([("value".into(), D1Value::Text("done".into()))]),
            )
            .unwrap();
        assert_eq!(reference.revision(), 2);
        assert_eq!(reference.values()["count"], D1Value::Int(7));
        assert_eq!(reference.values()["name"], D1Value::Text("done".into()));
    }

    #[test]
    fn rejects_unvalidated_initial_state_and_escapes_literals() {
        assert!(preview(SOURCE, "app/main.gscene", BTreeMap::new()).is_err());
        let mut values = initial();
        values.insert("name".into(), D1Value::Text("\\(unsafe)\n".into()));
        let output = preview(SOURCE, "app/main.gscene", values).unwrap();
        assert!(output.swift().contains("s1: \"\\\\(unsafe)\\n\""));
        assert_ne!(output.to_json()["initial_state_sha256"], preview(SOURCE, "app/main.gscene", initial()).unwrap().to_json()["initial_state_sha256"]);
    }
}
