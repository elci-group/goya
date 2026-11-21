//! JavaScript emitter for the bounded scalar D1 runtime.
//!
//! The emitter validates every interaction through Goya's canonical D1
//! compiler and then produces one standalone ES module. The module owns one
//! state map; each dispatch is a checked, atomic transition that mirrors
//! `goya_executable::D1Program::execute` step by step:
//!
//! 1. Input text is well-formed UTF-16 (`INVALID_TEXT`) and within the D1
//!    text limit (`TEXT_LIMIT`); the input map is within the snapshot limit
//!    (`SNAPSHOT_LIMIT`).
//! 2. Input keys and types match the action (`INPUT_SCHEMA`, `INPUT_TYPE`).
//! 3. The touched state plus inputs are within the snapshot limit.
//! 4. Instructions run in canonical order with canonical register names, so
//!    the register budget (`REGISTER_LIMIT`) and checked signed 64-bit
//!    addition (`ARITHMETIC_OVERFLOW`) fail at the same instruction.
//! 5. The next touched snapshot is re-validated (`TEXT_LIMIT`,
//!    `SNAPSHOT_LIMIT`).
//! 6. The revision is checked last (`REVISION_OVERFLOW`).
//! 7. An optional host `beforeCommit` hook sees the written values and may
//!    refuse the commit (`STORAGE_FAILED`, or `STORAGE_CONFLICT` when the
//!    hook reports one), so durable writes can precede the visible
//!    revision. This step has no canonical counterpart.
//!
//! Every failure leaves state and revision unchanged. The module has no
//! imports, timers, storage, network access or `eval`, and dispatch is
//! synchronous, so activations cannot interleave.

use goya_lang::{
    InteractionClass, InteractionExpr, InteractionType, InteractionValue, ParsedScene, SourceRange,
    TransitionError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Contract identifier embedded in the generated module.
pub const D1_RUNTIME_CONTRACT: &str = "MAJA-D1-JS/1-draft";

/// Generated module plus the metadata a shell needs to bind controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct D1Runtime {
    /// Standalone ES module text. ASCII only.
    pub javascript: String,
    pub source_digest: [u8; 32],
    /// One entry per interaction, in source order.
    pub actions: Vec<RuntimeAction>,
    /// Every state key any action touches, with its type, in key order.
    pub state_types: Vec<(String, InteractionType)>,
}

/// One dispatchable action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeAction {
    /// Authored action identity; the dispatch key.
    pub action_id: String,
    /// Structural interaction name.
    pub name: String,
    /// Structural name of the triggering `@button`.
    pub trigger_button: String,
    /// Declared inputs in key order.
    pub inputs: Vec<(String, InteractionType)>,
    /// Source range of the interaction.
    pub range: SourceRange,
}

/// Why a scene could not be emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum D1RuntimeError {
    /// The canonical D1 compiler rejected the scene.
    InvalidD1(TransitionError),
    /// The scene is valid but outside what this emitter supports.
    Unsupported(&'static str),
}

impl fmt::Display for D1RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidD1(error) => write!(f, "canonical D1 validation failed: {error}"),
            Self::Unsupported(what) => write!(f, "the D1 runtime does not support {what}"),
        }
    }
}
impl std::error::Error for D1RuntimeError {}

/// Emit the runtime for a scene that already passed strict admission. The
/// D1 surface is re-checked through the canonical compiler.
pub fn emit_d1_runtime(parsed: &ParsedScene) -> Result<D1Runtime, D1RuntimeError> {
    let scene = &parsed.scene;
    if scene.interactions.is_empty() {
        return Err(D1RuntimeError::Unsupported("scenes without interactions"));
    }
    if scene.interactions.iter().any(|i| {
        i.class != InteractionClass::D1 || !i.declared_effects.is_empty() || !i.requests.is_empty()
    }) {
        return Err(D1RuntimeError::Unsupported("D2 or effectful interactions"));
    }
    let compiled = goya_lang::compile_d1_interactions(scene).map_err(D1RuntimeError::InvalidD1)?;
    if compiled.len() != scene.interactions.len() {
        return Err(D1RuntimeError::Unsupported("an incomplete D1 compilation"));
    }

    let mut state_types = BTreeMap::<String, InteractionType>::new();
    let mut actions = Vec::new();
    for interaction in &scene.interactions {
        let id = interaction
            .authored_id
            .as_ref()
            .filter(|id| !id.is_empty())
            .ok_or(D1RuntimeError::Unsupported("actions without authored IDs"))?;
        for key in interaction
            .state_reads
            .iter()
            .chain(&interaction.state_writes)
        {
            if state_types
                .insert(key.name.clone(), key.value_type)
                .is_some_and(|old| old != key.value_type)
            {
                return Err(D1RuntimeError::Unsupported("conflicting state types"));
            }
        }
        let range = parsed
            .entries
            .iter()
            .find(|e| e.kind == "interaction" && e.semantic_id.as_deref() == Some(id.as_str()))
            .map(|e| e.range)
            .ok_or(D1RuntimeError::Unsupported(
                "actions without source origins",
            ))?;
        let inputs = interaction
            .inputs
            .iter()
            .map(|k| (k.name.clone(), k.value_type))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect();
        actions.push(RuntimeAction {
            action_id: id.clone(),
            name: interaction.name.clone(),
            trigger_button: interaction.trigger_button.clone(),
            inputs,
            range,
        });
    }
    if state_types.len() > 256 {
        return Err(D1RuntimeError::Unsupported(
            "more than 256 application state keys",
        ));
    }

    let mut out = String::new();
    out.push_str(&format!(
        "// Generated by Maja from Goya's canonical D1 compiler ({D1_RUNTIME_CONTRACT}). Do not edit.\n"
    ));
    out.push_str(PRELUDE);
    out.push_str("const STATE_TYPES = new Map([");
    for (n, (key, ty)) in state_types.iter().enumerate() {
        if n > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!(
            "[{}, {}]",
            js_string(key),
            js_string(type_name(*ty))
        ));
    }
    out.push_str("]);\n");
    out.push_str("const ACTIONS = new Map([\n");
    for (interaction, action) in scene.interactions.iter().zip(&actions) {
        let touched = interaction
            .state_reads
            .iter()
            .chain(&interaction.state_writes)
            .map(|k| k.name.as_str())
            .collect::<BTreeSet<_>>();
        out.push_str(&format!(
            "  [{}, {{\n    inputs: [",
            js_string(&action.action_id)
        ));
        for (n, (key, ty)) in action.inputs.iter().enumerate() {
            if n > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!(
                "[{}, {}]",
                js_string(key),
                js_string(type_name(*ty))
            ));
        }
        out.push_str("],\n    touched: [");
        for (n, key) in touched.iter().enumerate() {
            if n > 0 {
                out.push_str(", ");
            }
            out.push_str(&js_string(key));
        }
        out.push_str("],\n    run(state, inputs) {\n      let rb = 0;\n");
        let mut register = 0usize;
        let mut writes = Vec::new();
        for assignment in &interaction.assignments {
            let reg = emit_expr(&assignment.value, &mut register, &mut out);
            writes.push((assignment.key.as_str(), reg));
        }
        out.push_str("      return [");
        for (n, (key, reg)) in writes.iter().enumerate() {
            if n > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("[{}, {reg}]", js_string(key)));
        }
        out.push_str("];\n    },\n  }],\n");
    }
    out.push_str("]);\n");
    out.push_str(RUNTIME);
    Ok(D1Runtime {
        javascript: out,
        source_digest: parsed.source_digest,
        actions,
        state_types: state_types.into_iter().collect(),
    })
}

/// Emit one canonical instruction per node, naming registers `r0, r1, ...`
/// exactly as `goya_lang::transition::compile_expr` does, because the
/// register budget charges the name's bytes.
fn emit_expr(expr: &InteractionExpr, next: &mut usize, out: &mut String) -> String {
    let value = match expr {
        InteractionExpr::Literal(InteractionValue::Int(n)) => format!("intValue({n}n)"),
        InteractionExpr::Literal(InteractionValue::Text(s)) => {
            format!("textValue({})", js_string(s))
        }
        InteractionExpr::Literal(InteractionValue::Bool(b)) => format!("boolValue({b})"),
        InteractionExpr::State(key) => format!("state.get({})", js_string(key)),
        InteractionExpr::Input(key) => format!("inputs.get({})", js_string(key)),
        InteractionExpr::Add(a, b) => {
            let left = emit_expr(a, next, out);
            let right = emit_expr(b, next, out);
            format!("checkedAdd({left}, {right})")
        }
        InteractionExpr::Effect(_) => unreachable!("canonical D1 admission excludes effects"),
    };
    let reg = format!("r{next}");
    *next += 1;
    out.push_str(&format!("      const {reg} = {value};\n"));
    if matches!(expr, InteractionExpr::Add(..)) {
        out.push_str(&format!(
            "      if ({reg} === null) return \"ARITHMETIC_OVERFLOW\";\n"
        ));
    }
    out.push_str(&format!(
        "      rb += {} + valueSize({reg}); if (rb > MAX_REGISTER_BYTES) return \"REGISTER_LIMIT\";\n",
        reg.len()
    ));
    reg
}

fn type_name(ty: InteractionType) -> &'static str {
    match ty {
        InteractionType::Int => "int",
        InteractionType::Text => "text",
        InteractionType::Bool => "bool",
    }
}

/// A double-quoted ASCII JavaScript string literal.
pub(crate) fn js_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                for unit in c.encode_utf16(&mut [0; 2]).iter() {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
    out
}

const PRELUDE: &str = r#"const MAX_SNAPSHOT_BYTES = 1048576;
const MAX_TEXT_BYTES = 65536;
const MAX_REGISTER_BYTES = 1048576;
const I64_MIN = -9223372036854775808n;
const I64_MAX = 9223372036854775807n;
const U64_MAX = 18446744073709551615n;
export const ERROR_CODES = Object.freeze(["UNKNOWN_ACTION", "INPUT_SCHEMA", "INPUT_TYPE", "STATE_SCHEMA", "STATE_TYPE", "INVALID_TEXT", "TEXT_LIMIT", "SNAPSHOT_LIMIT", "REGISTER_LIMIT", "ARITHMETIC_OVERFLOW", "REVISION_OVERFLOW", "STORAGE_FAILED", "STORAGE_CONFLICT", "REENTRANT_DISPATCH"]);
export function intValue(value) { return Object.freeze({ type: "int", value }); }
export function textValue(value) { return Object.freeze({ type: "text", value }); }
export function boolValue(value) { return Object.freeze({ type: "bool", value }); }
// UTF-8 length of a string, or -1 when it holds an unpaired surrogate.
function utf8Length(s) {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff) {
      const d = s.charCodeAt(i + 1);
      if (!(d >= 0xdc00 && d <= 0xdfff)) return -1;
      n += 4; i++;
    } else if (c >= 0xdc00 && c <= 0xdfff) return -1;
    else n += 3;
  }
  return n;
}
// The value's type name, or null when it is not a well-formed D1 value.
function kind(v) {
  if (v === null || typeof v !== "object") return null;
  switch (v.type) {
    case "int": return typeof v.value === "bigint" && v.value >= I64_MIN && v.value <= I64_MAX ? "int" : null;
    case "text": return typeof v.value === "string" ? "text" : null;
    case "bool": return typeof v.value === "boolean" ? "bool" : null;
    default: return null;
  }
}
function valueSize(v) { return v.type === "int" ? 8 : v.type === "bool" ? 1 : utf8Length(v.value); }
// Text check for well-shaped values; shape errors are reported later as type errors.
function textError(v) {
  if (kind(v) !== "text") return null;
  const n = utf8Length(v.value);
  return n < 0 ? "INVALID_TEXT" : n > MAX_TEXT_BYTES ? "TEXT_LIMIT" : null;
}
function entriesSize(entries) {
  let n = 8;
  for (const [k, v] of entries) n += 8 + utf8Length(k) + (kind(v) === null ? 0 : valueSize(v));
  return n;
}
function checkedAdd(a, b) {
  const sum = a.value + b.value;
  return sum < I64_MIN || sum > I64_MAX ? null : intValue(sum);
}
"#;

const RUNTIME: &str = r#"export const stateTypes = () => new Map(STATE_TYPES);
export const actionInputs = (actionId) => ACTIONS.has(actionId) ? new Map(ACTIONS.get(actionId).inputs) : null;
// Create the single state owner. `initialState` is a Map from every state key
// to a typed value; `initialRevision` is a BigInt in [0, 2^64 - 1].
export function createRuntime(initialState, initialRevision = 0n) {
  if (!(initialState instanceof Map)) throw new TypeError("STATE_SCHEMA");
  if (typeof initialRevision !== "bigint" || initialRevision < 0n || initialRevision > U64_MAX) throw new TypeError("REVISION_OVERFLOW");
  if (initialState.size !== STATE_TYPES.size || [...STATE_TYPES.keys()].some((k) => !initialState.has(k))) throw new TypeError("STATE_SCHEMA");
  for (const [k, v] of initialState) {
    const t = textError(v);
    if (t) throw new TypeError(t);
    if (kind(v) !== STATE_TYPES.get(k)) throw new TypeError("STATE_TYPE");
  }
  let values = new Map([...initialState].map(([k, v]) => [k, Object.freeze({ type: v.type, value: v.value })]));
  let revision = initialRevision;
  const fail = (code) => Object.freeze({ ok: false, code });
  let committing = false;
  // `beforeCommit`, when given, receives the frozen map of written values
  // after every canonical check has passed and before state changes. It
  // returns true to commit, or "STORAGE_CONFLICT" to report that durable
  // storage changed elsewhere; anything else (or a throw) fails with
  // STORAGE_FAILED. On refusal nothing changes. It must not dispatch.
  function dispatch(actionId, inputs, beforeCommit) {
    if (committing) return fail("REENTRANT_DISPATCH");
    const action = typeof actionId === "string" ? ACTIONS.get(actionId) : undefined;
    if (action === undefined) return fail("UNKNOWN_ACTION");
    if (!(inputs instanceof Map) || [...inputs.keys()].some((k) => typeof k !== "string")) return fail("INPUT_SCHEMA");
    for (const v of inputs.values()) { const t = textError(v); if (t) return fail(t); }
    for (const k of inputs.keys()) if (utf8Length(k) < 0) return fail("INPUT_SCHEMA");
    const inputSize = entriesSize(inputs);
    if (inputSize > MAX_SNAPSHOT_BYTES) return fail("SNAPSHOT_LIMIT");
    if (inputs.size !== action.inputs.length || action.inputs.some(([k]) => !inputs.has(k))) return fail("INPUT_SCHEMA");
    if (action.inputs.some(([k, t]) => kind(inputs.get(k)) !== t)) return fail("INPUT_TYPE");
    const touched = new Map(action.touched.map((k) => [k, values.get(k)]));
    if (entriesSize(touched) + inputSize > MAX_SNAPSHOT_BYTES) return fail("SNAPSHOT_LIMIT");
    const frozenInputs = new Map([...inputs].map(([k, v]) => [k, Object.freeze({ type: v.type, value: v.value })]));
    const writes = action.run(touched, frozenInputs);
    if (typeof writes === "string") return fail(writes);
    const next = new Map(touched);
    for (const [k, v] of writes) next.set(k, v);
    for (const v of next.values()) { const t = textError(v); if (t) return fail(t); }
    if (entriesSize(next) > MAX_SNAPSHOT_BYTES) return fail("SNAPSHOT_LIMIT");
    if (revision === U64_MAX) return fail("REVISION_OVERFLOW");
    if (beforeCommit !== undefined) {
      let answer = false;
      committing = true;
      try { answer = beforeCommit(Object.freeze(new Map(writes))); } catch { answer = false; } finally { committing = false; }
      if (answer !== true) return fail(answer === "STORAGE_CONFLICT" ? "STORAGE_CONFLICT" : "STORAGE_FAILED");
    }
    const committed = new Map(values);
    for (const [k, v] of writes) committed.set(k, v);
    values = committed;
    revision += 1n;
    return Object.freeze({ ok: true, revision });
  }
  return Object.freeze({
    dispatch,
    get revision() { return revision; },
    get(key) { return values.get(key); },
    snapshot() { return Object.freeze({ revision, state: new Map(values) }); },
  });
}
"#;
