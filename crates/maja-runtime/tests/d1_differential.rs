//! Differential test: the generated JavaScript D1 runtime against Goya's
//! canonical D1 executor, case by case, under Node.
//!
//! Node is required. Set `MAJA_SKIP_NODE=1` to skip explicitly when it is
//! unavailable; the skip is printed, never silent.

use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use goya_lang::{handoff, InteractionType};
use maja_runtime::{emit_d1_runtime, D1RuntimeError};
use std::collections::{BTreeMap, BTreeSet};
use std::{fs, process::Command};

const SOURCE: &str = r#"@ontology diff {
    @state {
        count: int = 0 session;
        total: int = 0 session;
        label: text = "" session;
        flag: bool = false session;
    }
    @cell root {
        id: "root";
        @input amount { id: "field.amount"; }
        @input name { id: "field.name"; }
        @button add { id: "button.add"; }
        @button sum { id: "button.sum"; }
        @button rename { id: "button.rename"; }
        @button greet { id: "button.greet"; }
        @button raise { id: "button.raise"; }
    }
    @interaction add {
        id: "action.add";
        trigger: button.add.activate;
        class: D1;
        state { read count: int; write count: int; }
        input { amount: int from "field.amount"; }
        set count = state.count + input.amount;
    }
    @interaction sum {
        id: "action.sum";
        trigger: button.sum.activate;
        class: D1;
        state { read count: int; read total: int; write total: int; write count: int; }
        input { amount: int from "field.amount"; }
        set total = state.total + state.count + input.amount + -1;
        set count = 0;
    }
    @interaction rename {
        id: "action.rename";
        trigger: button.rename.activate;
        class: D1;
        state { write label: text; }
        input { value: text from "field.name"; }
        set label = input.value;
    }
    @interaction greet {
        id: "action.greet";
        trigger: button.greet.activate;
        class: D1;
        state { write label: text; }
        input { }
        set label = "γειά 🌍 <tag> & 'end'";
    }
    @interaction raise {
        id: "action.raise";
        trigger: button.raise.activate;
        class: D1;
        state { write flag: bool; }
        input { }
        set flag = true;
    }
}"#;

#[derive(Clone, Debug)]
enum V {
    I(i64),
    T(String),
    B(bool),
}

impl V {
    fn d1(&self) -> D1Value {
        match self {
            V::I(n) => D1Value::Int(*n),
            V::T(s) => D1Value::Text(s.clone()),
            V::B(b) => D1Value::Bool(*b),
        }
    }
    fn js(&self) -> String {
        match self {
            V::I(n) => format!("intValue({n}n)"),
            V::T(s) => format!("textValue({})", json(s)),
            V::B(b) => format!("boolValue({b})"),
        }
    }
}

fn json(s: &str) -> String {
    let mut out = String::from("\"");
    for unit in s.encode_utf16() {
        match unit {
            0x22 => out.push_str("\\\""),
            0x5c => out.push_str("\\\\"),
            0x20..=0x7e => out.push(unit as u8 as char),
            _ => out.push_str(&format!("\\u{unit:04x}")),
        }
    }
    out.push('"');
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn render(value: &D1Value) -> String {
    match value {
        D1Value::Int(n) => format!("i{n}"),
        D1Value::Text(s) => format!("t{}", hex(s.as_bytes())),
        D1Value::Bool(b) => format!("b{b}"),
    }
}

struct Case {
    action: &'static str,
    state: BTreeMap<String, V>,
    revision: u64,
    inputs: Vec<(String, V)>,
}

/// Canonical outcome in the shared rendering. Input schema and type
/// failures are one canonical error, so both JS codes map to `INPUT`.
fn canonical(
    compiled: &BTreeMap<String, goya_lang::CompiledInteraction>,
    touched: &BTreeMap<String, BTreeSet<String>>,
    case: &Case,
) -> String {
    let interaction = &compiled[case.action];
    let snapshot_values = touched[case.action]
        .iter()
        .map(|k| (k.clone(), case.state[k].d1()))
        .collect();
    let inputs = case
        .inputs
        .iter()
        .map(|(k, v)| (k.clone(), v.d1()))
        .collect();
    let inputs = match InputSnapshot::new(inputs) {
        Ok(inputs) => inputs,
        Err(error) => return format!("err:{}", vm_code(&error.to_string())),
    };
    let snapshot = match D1Snapshot::new(case.revision, snapshot_values) {
        Ok(snapshot) => snapshot,
        Err(error) => return format!("err:{}", vm_code(&error.to_string())),
    };
    match interaction.execute(&snapshot, &inputs) {
        Ok(outcome) => {
            let mut state: BTreeMap<String, D1Value> =
                case.state.iter().map(|(k, v)| (k.clone(), v.d1())).collect();
            state.extend(outcome.next_state);
            let body: Vec<String> = state
                .iter()
                .map(|(k, v)| format!("{k}={}", render(v)))
                .collect();
            format!("ok:{}:{}", outcome.revision_after, body.join(","))
        }
        Err(goya_lang::TransitionError::InputTypeMismatch) => "err:INPUT".into(),
        Err(goya_lang::TransitionError::StateTypeMismatch) => "err:STATE_TYPE".into(),
        Err(goya_lang::TransitionError::VmRejected(message)) => format!("err:{}", vm_code(&message)),
        Err(other) => panic!("unexpected canonical error {other:?}"),
    }
}

fn vm_code(message: &str) -> &'static str {
    match message {
        "integer addition overflow" => "ARITHMETIC_OVERFLOW",
        "state revision counter exhausted" => "REVISION_OVERFLOW",
        "snapshot exceeds the byte limit" => "SNAPSHOT_LIMIT",
        "text state value exceeds the byte limit" => "TEXT_LIMIT",
        "D1 registers exceed the byte limit" => "REGISTER_LIMIT",
        "input snapshot keys do not match contract" => "INPUT",
        other => panic!("unmapped canonical VM error {other:?}"),
    }
}

fn base_state(count: i64, total: i64, label: &str, flag: bool) -> BTreeMap<String, V> {
    BTreeMap::from([
        ("count".into(), V::I(count)),
        ("total".into(), V::I(total)),
        ("label".into(), V::T(label.into())),
        ("flag".into(), V::B(flag)),
    ])
}

fn cases() -> Vec<Case> {
    let ints = [
        0,
        1,
        -1,
        42,
        i64::MAX,
        i64::MAX - 1,
        i64::MIN,
        i64::MIN + 1,
        i64::MAX / 2,
    ];
    let revisions = [0, 1, 41, u64::MAX - 1, u64::MAX];
    let mut out = Vec::new();
    for &count in &ints {
        for &amount in &ints {
            for &revision in &revisions[..3] {
                for action in ["action.add", "action.sum"] {
                    out.push(Case {
                        action,
                        state: base_state(count, 7, "x", false),
                        revision,
                        inputs: vec![("amount".into(), V::I(amount))],
                    });
                }
            }
        }
        out.push(Case {
            action: "action.sum",
            state: base_state(count, i64::MIN, "x", false),
            revision: 3,
            inputs: vec![("amount".into(), V::I(0))],
        });
    }
    for &revision in &revisions {
        for action in ["action.add", "action.greet", "action.raise"] {
            out.push(Case {
                action,
                state: base_state(i64::MAX, 0, "", true),
                revision,
                inputs: if action == "action.add" {
                    vec![("amount".into(), V::I(1))]
                } else {
                    vec![]
                },
            });
        }
    }
    let limit = "a".repeat(64 * 1024);
    let over = format!("{limit}a");
    let multibyte_limit = "€".repeat(64 * 1024 / 3) + "a";
    for text in [
        "",
        " Ünïcode\u{301}\n ",
        "🌍",
        "\u{0}",
        limit.as_str(),
        over.as_str(),
        multibyte_limit.as_str(),
    ] {
        out.push(Case {
            action: "action.rename",
            state: base_state(0, 0, "old", false),
            revision: 9,
            inputs: vec![("value".into(), V::T(text.into()))],
        });
    }
    // Schema and type failures.
    for inputs in [
        vec![],
        vec![("amount".into(), V::T("1".into()))],
        vec![("amount".into(), V::B(true))],
        vec![("amount".into(), V::I(1)), ("value".into(), V::I(1))],
        vec![("value".into(), V::I(1))],
    ] {
        out.push(Case {
            action: "action.add",
            state: base_state(5, 0, "", false),
            revision: 2,
            inputs,
        });
    }
    out.push(Case {
        action: "action.greet",
        state: base_state(5, 0, "", false),
        revision: 2,
        inputs: vec![("amount".into(), V::I(1))],
    });
    out
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn generated_runtime_matches_canonical_d1_case_by_case() {
    if !node_available() {
        assert!(
            std::env::var_os("MAJA_SKIP_NODE").is_some(),
            "node is required for the D1 differential test; set MAJA_SKIP_NODE=1 to skip"
        );
        eprintln!("SKIPPED: node unavailable and MAJA_SKIP_NODE set");
        return;
    }
    let handoff = handoff::admit_v2(SOURCE).unwrap();
    let parsed = handoff.parsed();
    let runtime = emit_d1_runtime(parsed).unwrap();
    assert!(runtime.javascript.is_ascii());
    assert_eq!(runtime, emit_d1_runtime(parsed).unwrap(), "deterministic");

    let compiled: BTreeMap<String, goya_lang::CompiledInteraction> = parsed
        .scene
        .interactions
        .iter()
        .zip(goya_lang::compile_d1_interactions(&parsed.scene).unwrap())
        .map(|(decl, c)| (decl.authored_id.clone().unwrap(), c))
        .collect();
    let touched: BTreeMap<String, BTreeSet<String>> = parsed
        .scene
        .interactions
        .iter()
        .map(|decl| {
            (
                decl.authored_id.clone().unwrap(),
                decl.state_reads
                    .iter()
                    .chain(&decl.state_writes)
                    .map(|k| k.name.clone())
                    .collect(),
            )
        })
        .collect();

    let cases = cases();
    let expected: Vec<String> = cases
        .iter()
        .map(|case| canonical(&compiled, &touched, case))
        .collect();
    assert!(expected.iter().any(|e| e.starts_with("ok:")));
    for code in [
        "ARITHMETIC_OVERFLOW",
        "REVISION_OVERFLOW",
        "TEXT_LIMIT",
        "INPUT",
    ] {
        assert!(
            expected.iter().any(|e| e == &format!("err:{code}")),
            "corpus never exercises {code}"
        );
    }

    let mut driver = String::from(
        "import { createRuntime, intValue, textValue, boolValue } from \"./runtime.mjs\";\n\
         const hex = (s) => Buffer.from(s, \"utf8\").toString(\"hex\");\n\
         const render = (v) => v.type === \"int\" ? `i${v.value}` : v.type === \"text\" ? `t${hex(v.value)}` : `b${v.value}`;\n\
         const out = [];\n\
         function run(action, state, revision, inputs) {\n\
           const rt = createRuntime(new Map(state), revision);\n\
           const before = rt.snapshot();\n\
           const r = rt.dispatch(action, new Map(inputs));\n\
           const after = rt.snapshot();\n\
           if (!r.ok) {\n\
             if (after.revision !== before.revision || [...after.state].some(([k, v]) => before.state.get(k) !== v)) throw new Error(\"failed dispatch changed state\");\n\
             out.push(`err:${r.code === \"INPUT_SCHEMA\" || r.code === \"INPUT_TYPE\" ? \"INPUT\" : r.code}`);\n\
             return;\n\
           }\n\
           const keys = [...after.state.keys()].sort();\n\
           out.push(`ok:${after.revision}:${keys.map((k) => `${k}=${render(after.state.get(k))}`).join(\",\")}`);\n\
         }\n",
    );
    for case in &cases {
        let state: Vec<String> = case
            .state
            .iter()
            .map(|(k, v)| format!("[{}, {}]", json(k), v.js()))
            .collect();
        let inputs: Vec<String> = case
            .inputs
            .iter()
            .map(|(k, v)| format!("[{}, {}]", json(k), v.js()))
            .collect();
        driver.push_str(&format!(
            "run({}, [{}], {}n, [{}]);\n",
            json(case.action),
            state.join(", "),
            case.revision,
            inputs.join(", ")
        ));
    }
    driver.push_str("process.stdout.write(out.join(\"\\n\") + \"\\n\");\n");

    let dir = std::env::temp_dir().join(format!("maja-d1-diff-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("runtime.mjs"), &runtime.javascript).unwrap();
    fs::write(dir.join("driver.mjs"), driver).unwrap();
    let output = Command::new("node")
        .arg(dir.join("driver.mjs"))
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(actual.len(), expected.len());
    for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(a, e, "case {i}: action {}", cases[i].action);
    }
}

#[test]
fn generated_runtime_rejects_what_the_canonical_compiler_rejects() {
    let parsed = |source: &str| goya_lang::parse_strict(source).unwrap();
    let d2 = SOURCE.replace("class: D1;", "class: D2;");
    assert!(goya_lang::parse_strict_gssh2(&d2)
        .map(|p| emit_d1_runtime(&p))
        .map_or(true, |r| r.is_err()));
    let none = parsed(r#"@ontology x { @cell r { id: "r"; } }"#);
    assert_eq!(
        emit_d1_runtime(&none).unwrap_err(),
        D1RuntimeError::Unsupported("scenes without interactions")
    );
    let handoff = handoff::admit_v2(SOURCE).unwrap();
    let runtime = emit_d1_runtime(handoff.parsed()).unwrap();
    assert_eq!(runtime.actions.len(), 5);
    let add = &runtime.actions[0];
    assert_eq!(add.action_id, "action.add");
    assert_eq!(add.trigger_button, "add");
    assert_eq!(add.inputs, vec![("amount".to_owned(), InteractionType::Int)]);
    assert!(SOURCE[add.range.start..add.range.end].contains("action.add"));
    assert_eq!(
        runtime
            .state_types
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["count", "flag", "label", "total"]
    );
}

#[test]
fn generated_runtime_guards_javascript_only_failure_modes() {
    if !node_available() {
        assert!(std::env::var_os("MAJA_SKIP_NODE").is_some(), "node is required");
        return;
    }
    let handoff = handoff::admit_v2(SOURCE).unwrap();
    let runtime = emit_d1_runtime(handoff.parsed()).unwrap();
    // Lone surrogates, non-BigInt numbers, out-of-range BigInts, unknown
    // actions and malformed initial state cannot be built in Rust.
    let driver = r#"import { createRuntime, intValue, textValue, boolValue } from "./runtime.mjs";
const init = () => new Map([["count", intValue(0n)], ["total", intValue(0n)], ["label", textValue("")], ["flag", boolValue(false)]]);
const out = [];
const rt = createRuntime(init());
out.push(rt.dispatch("action.rename", new Map([["value", textValue("a\ud800b")]])).code);
out.push(rt.dispatch("action.rename", new Map([["value", textValue("\udc00")]])).code);
out.push(rt.dispatch("action.add", new Map([["amount", { type: "int", value: 1 }]])).code);
out.push(rt.dispatch("action.add", new Map([["amount", intValue(9223372036854775808n)]])).code);
out.push(rt.dispatch("action.add", new Map([["amount", null]])).code);
out.push(rt.dispatch("action.missing", new Map()).code);
out.push(rt.dispatch(1, new Map()).code);
out.push(rt.dispatch("action.add", { amount: intValue(1n) }).code);
out.push(String(rt.revision));
const ok = rt.dispatch("action.add", new Map([["amount", intValue(5n)]]));
out.push(`${ok.ok}:${ok.revision}:${rt.get("count").value}`);
for (const bad of [
  () => createRuntime(new Map()),
  () => createRuntime(new Map([...init(), ["extra", intValue(0n)]])),
  () => createRuntime(new Map([...init(), ["count", textValue("0")]])),
  () => createRuntime(new Map([...init(), ["label", textValue("\ud800")]])),
  () => createRuntime(init(), -1n),
  () => createRuntime(init(), 18446744073709551616n),
  () => createRuntime({}),
]) { try { bad(); out.push("accepted"); } catch (e) { out.push(e.message); } }
process.stdout.write(out.join("\n") + "\n");
"#;
    let dir = std::env::temp_dir().join(format!("maja-d1-js-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("runtime.mjs"), &runtime.javascript).unwrap();
    fs::write(dir.join("driver.mjs"), driver).unwrap();
    let output = Command::new("node").arg(dir.join("driver.mjs")).output().unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let lines: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        lines,
        [
            "INVALID_TEXT",
            "INVALID_TEXT",
            "INPUT_TYPE",
            "INPUT_TYPE",
            "INPUT_TYPE",
            "UNKNOWN_ACTION",
            "UNKNOWN_ACTION",
            "INPUT_SCHEMA",
            "0",
            "true:1:5",
            "STATE_SCHEMA",
            "STATE_SCHEMA",
            "STATE_TYPE",
            "INVALID_TEXT",
            "REVISION_OVERFLOW",
            "REVISION_OVERFLOW",
            "STATE_SCHEMA",
        ]
    );
}
