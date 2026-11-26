//! GY-REPLAY/1 (draft) — behavioural replay groundwork for the Disparates
//! programme: the scenario authoring grammar, canonical scenario compilation,
//! the normalised state-trace schema, and a comparator skeleton.
//!
//! **This crate executes nothing and collects nothing.** It contains no
//! process spawning, no sandbox, no harness, and no I/O of any kind (test
//! fixtures are read by tests only). Trace *collection* requires executing
//! the original foreign application and the reconstructed Goya application,
//! which is authorised only when every authorisation condition of the draft
//! contract holds and is recorded per run: a dated authorisation decision,
//! OS-level sandboxing, enforced network denial, pinned builds, and preserved
//! licensing — see `docs/disparates/contracts/replay-1-draft.md`, "Scope
//! boundary and authorisation". No such record exists today, so the `runner`
//! and `collector` responsibilities named by the contract are deliberately
//! unimplemented; what exists here is the scaffolding that needs no
//! execution: authoring, validation, canonical serialisation, and the
//! comparison of traces that some future, authorised run produces.
//!
//! Honesty rules honoured by this crate:
//!
//! - Nothing here can mark a FAIR element `verified`; see
//!   [`verified_is_unreachable`]. The comparator reports a diff and nothing
//!   else — it derives no FAIR status.
//! - FAIR element IDs in bindings, expectations and traces are opaque,
//!   non-empty strings; they are never validated against a FAIR document
//!   here. [`unknown_fair_elements`] and [`unknown_fair_elements_in_trace`]
//!   are explicit cross-check helpers for callers that hold a document.
//! - Validation is strict: unknown triggers, unknown bindings, unknown input
//!   types and unknown properties are errors, never tolerated-and-ignored.
//!   Wall-clock sleep triggers are rejected as unrepresentable
//!   ([`SLEEP_TRIGGER_UNREPRESENTABLE`]); steps advance only at declared
//!   quiescence barriers, and the grammar has no timing syntax at all.
//!
//! Authoring grammar (v1; the `@`-block idiom of GY-LANG, one `@scenario`
//! per `.gscenario` file; `//` comments; `key: value;` properties; blocks
//! close at `}` and properties terminate at `;` or newline):
//!
//! ```text
//! @scenario NAME {
//!   format: "GY-REPLAY/1";     // optional, must match if present
//!   revision: 1;               // optional, must match if present
//!   target: <corpus-short-name>;
//!   setup {                      // exactly one, mandatory
//!     state: fresh;
//!     window { size: (WIDTH, HEIGHT); }
//!     seed { file "<sandbox-relative path>" = payload "<payload ref>"; }   // optional
//!   }
//!   bindings {                   // scenario identifier -> FAIR element ID
//!     button.<id>: <fair-element-id>;
//!     input.<id>:  <fair-element-id>;
//!     ipc.<channel>: <fair-element-id>;
//!     window.<event>: <fair-element-id>;
//!   }
//!   step NAME {                  // ordered; order is execution and comparison order
//!     trigger: button.<id>.activate | input.<id>.commit
//!            | ipc.send <channel> | window <event>;   // exactly one
//!     input { value: text "<value>"; }               // optional, at most one
//!     expect {                                       // optional
//!       ui    { visible "<text>"; input "<name>" = "<value>"; }
//!       store { file "<path>" contains key "<key>" = <scalar>; }
//!       effect{ ipc "<channel>"; }
//!     }
//!   }
//! }
//! ```
//!
//! A `ui` `visible`/`store`/`effect` observation aligns to the step's trigger
//! element; a `ui` `input "<name>"` observation aligns to binding
//! `input.<name>`. Every alignment key is the cited FAIR element ID, so
//! expectations are phrased in the trace vocabulary of record kinds
//! ([`Expected::record_kind`]).
//!
//! Contract: `GY-REPLAY/1` (draft, not ratified). See
//! `docs/disparates/contracts/replay-1-draft.md`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Replay contract identifier (draft).
pub const REPLAY_CONTRACT: &str = "GY-REPLAY/1";
/// Scenario format revision.
pub const FORMAT_REVISION: u32 = 1;
/// Trace format version (`trace_format: 1`).
pub const TRACE_FORMAT_VERSION: u64 = 1;

// ---------------------------------------------------------------------------
// Scenario documents
// ---------------------------------------------------------------------------

/// A structural failure of a scenario or trace, with a machine-readable
/// register code. Codes are a versioned register (v1): `SYNTAX`,
/// `TRAILING_INPUT`, `UNTERMINATED_STRING`, `UNKNOWN_PROPERTY`,
/// `CONTRACT_MISMATCH`, `BAD_REVISION`, `DUPLICATE_PROPERTY`, `MISSING_SETUP`,
/// `DUPLICATE_SETUP`, `MISSING_SETUP_STATE`, `INVALID_SETUP_STATE`,
/// `MISSING_WINDOW_SIZE`, `DUPLICATE_WINDOW_SIZE`, `BAD_WINDOW_SIZE`,
/// `EMPTY_SEED_PATH`, `EMPTY_SEED_PAYLOAD`, `UNKNOWN_BINDING`, `EMPTY_BINDING`,
/// `DUPLICATE_BINDING`, `BINDING_UNRESOLVED`, `UNKNOWN_TRIGGER`,
/// `SLEEP_TRIGGER_UNREPRESENTABLE`, `MISSING_TRIGGER`, `MULTIPLE_TRIGGERS`,
/// `UNKNOWN_INPUT_TYPE`, `EMPTY_INPUT`, `DUPLICATE_INPUT`, `NO_STEPS`,
/// `DUPLICATE_STEP_NAME`, `EMPTY_ELEMENT_REF`, `ELEMENT_NOT_BOUND`,
/// `EMPTY_ASSERTION_FIELD`. Trace-only codes: `BAD_TRACE_FORMAT`,
/// `BAD_SCENARIO_REVISION`, `EMPTY_IMPLEMENTATION`, `EMPTY_CORPUS_PIN`,
/// `EMPTY_HARNESS`, `BAD_BUILD_DIGEST`, `KIND_VALUE_MISMATCH`,
/// `EMPTY_PERSISTENCE_PATH`, `PERSISTENCE_VALUE_FORM`, `PERSISTENCE_KEY_FORM`,
/// `BAD_RECORD_DIGEST`, `IPC_CHANNEL_REQUIRED`, `CHILD_PROCESS_ARGV_REQUIRED`,
/// `OS_CALL_REQUIRED`, `SIDE_EFFECT_FIELD_CONFLICT`, `DUPLICATE_RECORD_KEY`.
/// The register grows by contract revision; codes are never renamed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioError {
    /// Machine-readable failure code from the register.
    pub code: String,
    /// Source line for parse failures; `None` for whole-document failures.
    pub line: Option<usize>,
    /// Step name or binding the failure concerns, where applicable.
    pub element: Option<String>,
    /// Human-readable explanation.
    pub message: String,
}

impl ScenarioError {
    fn new(code: &str, line: Option<usize>, element: Option<String>, message: impl Into<String>) -> Self {
        ScenarioError { code: code.into(), line, element, message: message.into() }
    }

    fn syntax(line: usize, message: impl Into<String>) -> Self {
        Self::new("SYNTAX", Some(line), None, message)
    }

    fn eof(expected: impl std::fmt::Display) -> Self {
        Self::new("SYNTAX", None, None, format!("unexpected end of input: expected {expected}"))
    }

    fn unknown_property(line: usize, key: &str, where_: &str) -> Self {
        Self::new(
            "UNKNOWN_PROPERTY",
            Some(line),
            None,
            format!("unknown property or block {key:?} in {where_} (strict grammar: unknown entries are errors, never ignored)"),
        )
    }

    fn sleep(line: usize) -> Self {
        Self::new(
            "SLEEP_TRIGGER_UNREPRESENTABLE",
            Some(line),
            None,
            "wall-clock sleeps are unrepresentable in the GY-REPLAY/1 grammar: steps advance only at declared quiescence barriers (replay-1-draft.md, timing discipline)",
        )
    }
}

/// A JSON scalar in an assertion, expectation or trace payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Scalar {
    /// String value.
    Str(String),
    /// Integer value.
    Int(i64),
    /// Boolean value.
    Bool(bool),
}

/// Declared deterministic initial state. v1 defines `fresh` only: a fresh
/// sandbox profile, every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupState {
    /// Fresh application profile; no prior state.
    Fresh,
}

/// Fixed window geometry; replay is resolution-pinned so both applications
/// observe the same viewport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowSize {
    /// Window width in pixels; must be positive.
    pub width: u64,
    /// Window height in pixels; must be positive.
    pub height: u64,
}

/// One seeded file: copied verbatim from a recorded payload into the fresh
/// sandbox profile before step 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedFile {
    /// Sandbox-relative destination path.
    pub path: String,
    /// Reference to the recorded payload seeded from.
    pub payload: String,
}

/// The deterministic start of a scenario. Absent setup is rejected: a
/// scenario without declared initial state cannot be deterministic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    /// Declared initial state; `fresh` in v1.
    pub state: SetupState,
    /// Fixed window geometry.
    pub window: WindowSize,
    /// Files seeded into the fresh profile, in authored order.
    pub seeded_files: Vec<SeedFile>,
}

/// The v1 trigger vocabulary, open and versioned like the FAIR element-kind
/// vocabulary. Unknown triggers are validation errors, never ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trigger {
    /// Activation of a bound button.
    Button {
        /// Bound identifier suffix; full binding key is `button.<id>`.
        id: String,
    },
    /// Commit of a bound input.
    Input {
        /// Bound identifier suffix; full binding key is `input.<id>`.
        id: String,
    },
    /// Outbound message on an IPC channel.
    IpcSend {
        /// Channel name; binding key is `ipc.<channel>`.
        channel: String,
    },
    /// Window lifecycle event.
    Window {
        /// Event name; binding key is `window.<event>`.
        event: String,
    },
}

impl Trigger {
    /// The scenario identifier this trigger resolves through the binding map:
    /// every scenario identifier is two-segment `<kind>.<name>`.
    pub fn binding_key(&self) -> String {
        match self {
            Trigger::Button { id } => format!("button.{id}"),
            Trigger::Input { id } => format!("input.{id}"),
            Trigger::IpcSend { channel } => format!("ipc.{channel}"),
            Trigger::Window { event } => format!("window.{event}"),
        }
    }

    /// Canonical authoring spelling of this trigger.
    pub fn spelling(&self) -> String {
        match self {
            Trigger::Button { id } => format!("button.{id}.activate"),
            Trigger::Input { id } => format!("input.{id}.commit"),
            Trigger::IpcSend { channel } => format!("ipc.send {channel}"),
            Trigger::Window { event } => format!("window {event}"),
        }
    }
}

/// Typed step input. The type vocabulary grows by contract revision; unknown
/// types are errors, never ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepInput {
    /// A text value committed to the bound input.
    Text {
        /// The committed text.
        value: String,
    },
}

/// One authored observable outcome, phrased in the trace vocabulary. The
/// `element` field is the FAIR element ID alignment key resolved through the
/// scenario bindings; it is never validated against a FAIR document here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expected {
    /// Visible text content on the step's element (`ui { visible "..." }`).
    UiText {
        /// FAIR element ID alignment key.
        element: String,
        /// Text expected to be visible.
        text: String,
    },
    /// An input's committed value (`ui { input "<field>" = "<value>" }`),
    /// aligned to binding `input.<field>`.
    UiInput {
        /// FAIR element ID alignment key.
        element: String,
        /// Bound input identifier suffix.
        field: String,
        /// Expected value.
        value: String,
    },
    /// A persistence assertion (`store { file "<path>" contains key "<key>" =
    /// <scalar> }`), aligned to the step's element.
    FileKey {
        /// FAIR element ID alignment key.
        element: String,
        /// Sandbox-relative file path.
        path: String,
        /// Key expected present in the file's normalised map.
        key: String,
        /// Expected scalar value of that key.
        value: Scalar,
    },
    /// A side-effect assertion (`effect { ipc "<channel>" }`): the outward
    /// action was issued, aligned to the step's element.
    EffectEmitted {
        /// FAIR element ID alignment key.
        element: String,
        /// Channel the effect was observed on.
        channel: String,
    },
}

impl Expected {
    /// FAIR element ID alignment key of this assertion.
    pub fn element(&self) -> &str {
        match self {
            Expected::UiText { element, .. }
            | Expected::UiInput { element, .. }
            | Expected::FileKey { element, .. }
            | Expected::EffectEmitted { element, .. } => element,
        }
    }

    fn set_element(&mut self, id: String) {
        match self {
            Expected::UiText { element, .. }
            | Expected::UiInput { element, .. }
            | Expected::FileKey { element, .. }
            | Expected::EffectEmitted { element, .. } => *element = id,
        }
    }

    /// The trace record kind this assertion is phrased in.
    pub fn record_kind(&self) -> RecordKind {
        match self {
            Expected::UiText { .. } | Expected::UiInput { .. } => RecordKind::UiState,
            Expected::FileKey { .. } => RecordKind::Persistence,
            Expected::EffectEmitted { .. } => RecordKind::SideEffect,
        }
    }
}

/// One ordered scenario step. Order is execution order and comparison order;
/// each step advances at a quiescence barrier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Step name; unique within the scenario.
    pub name: String,
    /// Exactly one trigger.
    pub trigger: Trigger,
    /// Optional typed input.
    pub input: Option<StepInput>,
    /// Authored observable outcomes.
    pub expect: Vec<Expected>,
}

/// A compiled scenario document: the canonical JSON form of one `.gscenario`
/// file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Contract identifier; must equal [`REPLAY_CONTRACT`].
    pub format: String,
    /// Scenario format revision; must equal [`FORMAT_REVISION`].
    pub revision: u32,
    /// Scenario name; unique within a scenario set.
    pub name: String,
    /// Corpus short name of the target application, bound to the pin.
    pub target: String,
    /// Declared deterministic initial state.
    pub setup: Setup,
    /// Scenario identifier -> FAIR element ID. Identifiers are two-segment
    /// `<kind>.<name>`; values are opaque non-empty FAIR element ID
    /// references, never validated against a FAIR document here.
    pub bindings: BTreeMap<String, String>,
    /// Ordered steps.
    pub steps: Vec<Step>,
}

/// True when `s` is a well-formed v1 scenario identifier (`<kind>.<name>`
/// over the trigger vocabulary's kinds).
fn is_scenario_identifier(s: &str) -> bool {
    let segs: Vec<&str> = s.split('.').collect();
    segs.len() == 2
        && matches!(segs[0], "button" | "input" | "ipc" | "window")
        && !segs[1].is_empty()
}

fn is_lower_hex_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

impl Scenario {
    /// Structural validation per the draft contract's invariants: at least
    /// one step; every step exactly one trigger (guaranteed by the schema,
    /// checked for resolution here); every trigger identifier resolves
    /// through the bindings; bindings and expectation alignment keys are
    /// non-empty FAIR element ID references; expected observables
    /// well-formed; no duplicate step names.
    pub fn validate(&self) -> Result<(), Vec<ScenarioError>> {
        let mut errs: Vec<ScenarioError> = Vec::new();
        if self.format != REPLAY_CONTRACT {
            errs.push(ScenarioError::new(
                "CONTRACT_MISMATCH",
                None,
                None,
                format!("format field must be {REPLAY_CONTRACT}, found {:?}", self.format),
            ));
        }
        if self.revision != FORMAT_REVISION {
            errs.push(ScenarioError::new(
                "BAD_REVISION",
                None,
                None,
                format!("revision field must be {FORMAT_REVISION}, found {}", self.revision),
            ));
        }
        if self.name.trim().is_empty() {
            errs.push(ScenarioError::new("EMPTY_SCENARIO_NAME", None, None, "scenario name must be non-empty"));
        }
        if self.target.trim().is_empty() {
            errs.push(ScenarioError::new("EMPTY_TARGET", None, None, "scenario target must be non-empty"));
        }
        if self.setup.window.width == 0 || self.setup.window.height == 0 {
            errs.push(ScenarioError::new(
                "BAD_WINDOW_SIZE",
                None,
                None,
                "window geometry must be positive",
            ));
        }
        for seed in &self.setup.seeded_files {
            if seed.path.trim().is_empty() {
                errs.push(ScenarioError::new("EMPTY_SEED_PATH", None, None, "seeded file path must be non-empty"));
            }
            if seed.payload.trim().is_empty() {
                errs.push(ScenarioError::new("EMPTY_SEED_PAYLOAD", None, None, "seeded payload reference must be non-empty"));
            }
        }
        for (key, value) in &self.bindings {
            if !is_scenario_identifier(key) {
                errs.push(ScenarioError::new(
                    "UNKNOWN_BINDING",
                    None,
                    Some(key.clone()),
                    format!("binding key {key:?} is outside the v1 identifier vocabulary (<button|input|ipc|window>.<name>)"),
                ));
            }
            if value.trim().is_empty() {
                errs.push(ScenarioError::new(
                    "EMPTY_BINDING",
                    None,
                    Some(key.clone()),
                    "bindings must cite non-empty FAIR element IDs",
                ));
            }
        }
        if self.steps.is_empty() {
            errs.push(ScenarioError::new(
                "NO_STEPS",
                None,
                None,
                "a scenario requires at least one step",
            ));
        }
        let mut seen = BTreeSet::new();
        for step in &self.steps {
            if !seen.insert(step.name.clone()) {
                errs.push(ScenarioError::new(
                    "DUPLICATE_STEP_NAME",
                    None,
                    Some(step.name.clone()),
                    format!("step name {:?} appears more than once", step.name),
                ));
            }
            let key = step.trigger.binding_key();
            if !self.bindings.contains_key(&key) {
                errs.push(ScenarioError::new(
                    "BINDING_UNRESOLVED",
                    None,
                    Some(step.name.clone()),
                    format!("trigger binding {key:?} is not declared in the scenario bindings"),
                ));
            }
            for expect in &step.expect {
                if expect.element().trim().is_empty() {
                    errs.push(ScenarioError::new(
                        "EMPTY_ELEMENT_REF",
                        None,
                        Some(step.name.clone()),
                        "expected observables must cite a non-empty FAIR element ID",
                    ));
                } else if !self.bindings.values().any(|v| v == expect.element()) {
                    errs.push(ScenarioError::new(
                        "ELEMENT_NOT_BOUND",
                        None,
                        Some(step.name.clone()),
                        format!(
                            "expectation cites FAIR element {:?}, which no binding cites (alignment keys must be bound)",
                            expect.element()
                        ),
                    ));
                }
                if let Expected::UiInput { element, field, .. } = expect {
                    match self.bindings.get(&format!("input.{field}")) {
                        Some(v) if v == element => {}
                        _ => errs.push(ScenarioError::new(
                            "ELEMENT_NOT_BOUND",
                            None,
                            Some(step.name.clone()),
                            format!(
                                "ui input observable cites element {:?} but binding \"input.{field}\" is missing or points elsewhere",
                                element
                            ),
                        )),
                    }
                }
                match expect {
                    Expected::UiText { text, .. } if text.trim().is_empty() => errs.push(ScenarioError::new(
                        "EMPTY_ASSERTION_FIELD",
                        None,
                        Some(step.name.clone()),
                        "ui visible assertion requires non-empty text",
                    )),
                    Expected::UiInput { field, .. } if field.trim().is_empty() => errs.push(ScenarioError::new(
                        "EMPTY_ASSERTION_FIELD",
                        None,
                        Some(step.name.clone()),
                        "ui input assertion requires a non-empty field name",
                    )),
                    Expected::FileKey { path, key, .. } if path.trim().is_empty() || key.trim().is_empty() => {
                        errs.push(ScenarioError::new(
                            "EMPTY_ASSERTION_FIELD",
                            None,
                            Some(step.name.clone()),
                            "store assertion requires non-empty path and key",
                        ))
                    }
                    Expected::EffectEmitted { channel, .. } if channel.trim().is_empty() => errs.push(ScenarioError::new(
                        "EMPTY_ASSERTION_FIELD",
                        None,
                        Some(step.name.clone()),
                        "effect assertion requires a non-empty channel",
                    )),
                    _ => {}
                }
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Deterministic canonical serialisation: fixed field order, sorted
    /// binding keys (via `BTreeMap`), trailing newline.
    pub fn canonical_json(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).expect("scenario serialises");
        out.push('\n');
        out
    }
}

/// Parse and strictly validate one `.gscenario` authoring file. Unknown
/// triggers, bindings, input types and properties are errors; wall-clock
/// sleep triggers are rejected as unrepresentable; `setup` is mandatory;
/// every trigger identifier must resolve through the scenario bindings.
pub fn parse_scenario(text: &str) -> Result<Scenario, ScenarioError> {
    let toks = lex(text)?;
    let mut p = Parser { toks, pos: 0 };
    let draft = p.scenario()?;
    p.skip_newlines();
    if let Some((l, t)) = p.peek() {
        return Err(ScenarioError::new(
            "TRAILING_INPUT",
            Some(l),
            None,
            format!("a scenario file contains exactly one `@scenario`; found trailing {t:?}"),
        ));
    }
    finalize(draft)
}

/// Compile `.gscenario` authoring text to its canonical JSON form (trailing
/// newline, deterministic): identical semantics, however commented or laid
/// out, compile to byte-identical output.
pub fn compile_scenario(text: &str) -> Result<String, ScenarioError> {
    Ok(parse_scenario(text)?.canonical_json())
}

// ---------------------------------------------------------------------------
// Authoring-grammar lexer and parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Newline,
    LBrace,
    RBrace,
    Colon,
    Semi,
    Eq,
    LParen,
    RParen,
    Comma,
    Ident(String),
    Str(String),
    Int(i64),
}

fn lex(src: &str) -> Result<Vec<(usize, Tok)>, ScenarioError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1usize;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\n' => {
                out.push((line, Tok::Newline));
                line += 1;
                i += 1;
            }
            ' ' | '\t' | '\r' => i += 1,
            '/' if i + 1 < chars.len() && chars[i + 1] == '/' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '{' => {
                out.push((line, Tok::LBrace));
                i += 1;
            }
            '}' => {
                out.push((line, Tok::RBrace));
                i += 1;
            }
            ':' => {
                out.push((line, Tok::Colon));
                i += 1;
            }
            ';' => {
                out.push((line, Tok::Semi));
                i += 1;
            }
            '=' => {
                out.push((line, Tok::Eq));
                i += 1;
            }
            '(' => {
                out.push((line, Tok::LParen));
                i += 1;
            }
            ')' => {
                out.push((line, Tok::RParen));
                i += 1;
            }
            ',' => {
                out.push((line, Tok::Comma));
                i += 1;
            }
            '"' => {
                let start_line = line;
                i += 1;
                let mut s = String::new();
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(ScenarioError::new(
                                "UNTERMINATED_STRING",
                                Some(start_line),
                                None,
                                "string literal is never closed",
                            ))
                        }
                        Some('\n') => {
                            return Err(ScenarioError::new(
                                "UNTERMINATED_STRING",
                                Some(start_line),
                                None,
                                "string literal may not contain a raw newline",
                            ))
                        }
                        Some('"') => {
                            i += 1;
                            break;
                        }
                        Some('\\') => match chars.get(i + 1) {
                            Some('"') => {
                                s.push('"');
                                i += 2;
                            }
                            Some('\\') => {
                                s.push('\\');
                                i += 2;
                            }
                            _ => {
                                return Err(ScenarioError::new(
                                    "SYNTAX",
                                    Some(line),
                                    None,
                                    "only \\\" and \\\\ escapes are defined",
                                ))
                            }
                        },
                        Some(&ch) => {
                            s.push(ch);
                            i += 1;
                        }
                    }
                }
                out.push((start_line, Tok::Str(s)));
            }
            '0'..='9' => {
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                let n: i64 = text.parse().map_err(|_| {
                    ScenarioError::new("SYNTAX", Some(line), None, format!("integer literal {text:?} out of range"))
                })?;
                out.push((line, Tok::Int(n)));
            }
            c if c.is_ascii_alphabetic() || c == '_' || c == '@' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-' || chars[i] == '.')
                {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                out.push((line, Tok::Ident(text)));
            }
            other => {
                return Err(ScenarioError::syntax(
                    line,
                    format!("unexpected character {other:?}"),
                ))
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(usize, Tok)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<(usize, Tok)> {
        self.toks.get(self.pos).cloned()
    }

    fn bump(&mut self) -> Option<(usize, Tok)> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Some((_, Tok::Newline))) {
            self.pos += 1;
        }
    }

    fn eof(&self, expected: impl std::fmt::Display) -> ScenarioError {
        let line = self.toks.last().map(|(l, _)| *l).unwrap_or(1);
        ScenarioError::new("SYNTAX", Some(line), None, format!("unexpected end of input: expected {expected}"))
    }

    fn expect_tok(&mut self, want: &Tok, what: &str) -> Result<usize, ScenarioError> {
        match self.bump() {
            None => Err(self.eof(what)),
            Some((l, t)) if &t == want => Ok(l),
            Some((l, t)) => Err(ScenarioError::syntax(l, format!("expected {what}, found {t:?}"))),
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<(usize, String), ScenarioError> {
        match self.bump() {
            None => Err(self.eof(what)),
            Some((l, Tok::Ident(s))) => Ok((l, s)),
            Some((l, t)) => Err(ScenarioError::syntax(l, format!("expected {what}, found {t:?}"))),
        }
    }

    fn expect_string(&mut self, what: &str) -> Result<(usize, String), ScenarioError> {
        match self.bump() {
            None => Err(self.eof(what)),
            Some((l, Tok::Str(s))) => Ok((l, s)),
            Some((l, t)) => Err(ScenarioError::syntax(l, format!("expected {what}, found {t:?}"))),
        }
    }

    fn expect_int(&mut self, what: &str) -> Result<i64, ScenarioError> {
        match self.bump() {
            None => Err(self.eof(what)),
            Some((l, Tok::Int(n))) => Ok(n),
            Some((l, t)) => Err(ScenarioError::syntax(l, format!("expected {what}, found {t:?}"))),
        }
    }

    fn expect_terminator(&mut self, after: &str) -> Result<(), ScenarioError> {
        match self.peek() {
            None => Err(self.eof(format!("`;` or newline after {after}"))),
            Some((_, Tok::Semi)) | Some((_, Tok::Newline)) => {
                self.bump();
                Ok(())
            }
            Some((l, t)) => Err(ScenarioError::syntax(
                l,
                format!("expected `;` or newline after {after}, found {t:?}"),
            )),
        }
    }

    fn collect_value(&mut self, what: &str) -> Result<Vec<(usize, Tok)>, ScenarioError> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.eof(format!("value for {what}"))),
                Some((_, Tok::Semi)) | Some((_, Tok::Newline)) => break,
                Some((l, Tok::RBrace)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("expected `;` or newline after {what}"),
                    ))
                }
                Some((l, t)) => {
                    out.push((l, t));
                    self.bump();
                }
            }
        }
        if out.is_empty() {
            let l = self.peek().map(|(l, _)| l).unwrap_or(1);
            return Err(ScenarioError::syntax(l, format!("empty value for {what}")));
        }
        Ok(out)
    }

    fn scenario(&mut self) -> Result<DraftScenario, ScenarioError> {
        self.skip_newlines();
        let (l, kw) = self.expect_ident("`@scenario`")?;
        if kw != "@scenario" {
            return Err(ScenarioError::syntax(l, format!("expected `@scenario`, found {kw:?}")));
        }
        let (_, name) = self.expect_ident("scenario name")?;
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after scenario name")?;
        let mut d = DraftScenario {
            name,
            target: None,
            format: None,
            revision: None,
            setup: None,
            bindings: BTreeMap::new(),
            steps: Vec::new(),
        };
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `@scenario` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(d);
                }
                Some((l, Tok::Ident(key))) => {
                    self.bump();
                    self.scenario_item(&mut d, l, &key)?;
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in `@scenario` body"),
                    ))
                }
            }
        }
    }

    fn scenario_item(&mut self, d: &mut DraftScenario, line: usize, key: &str) -> Result<(), ScenarioError> {
        match key {
            "target" => {
                if d.target.is_some() {
                    return Err(ScenarioError::new("DUPLICATE_PROPERTY", Some(line), None, "`target` declared twice"));
                }
                let v = self.collect_value("target")?;
                d.target = Some(single_name(&v, "target")?);
                self.expect_terminator("target")?;
            }
            "format" => {
                if d.format.is_some() {
                    return Err(ScenarioError::new("DUPLICATE_PROPERTY", Some(line), None, "`format` declared twice"));
                }
                let v = self.collect_value("format")?;
                let s = single_name(&v, "format")?;
                if s != REPLAY_CONTRACT {
                    return Err(ScenarioError::new(
                        "CONTRACT_MISMATCH",
                        Some(line),
                        None,
                        format!("scenario `format` must be {REPLAY_CONTRACT:?}, found {s:?}"),
                    ));
                }
                d.format = Some(s);
                self.expect_terminator("format")?;
            }
            "revision" => {
                if d.revision.is_some() {
                    return Err(ScenarioError::new("DUPLICATE_PROPERTY", Some(line), None, "`revision` declared twice"));
                }
                let v = self.collect_value("revision")?;
                let n = single_int(&v, "revision")?;
                if n != FORMAT_REVISION as i64 {
                    return Err(ScenarioError::new(
                        "BAD_REVISION",
                        Some(line),
                        None,
                        format!("scenario `revision` must be {FORMAT_REVISION}, found {n}"),
                    ));
                }
                d.revision = Some(n as u32);
                self.expect_terminator("revision")?;
            }
            "setup" => {
                if d.setup.is_some() {
                    return Err(ScenarioError::new("DUPLICATE_SETUP", Some(line), None, "`setup` declared twice"));
                }
                d.setup = Some(self.setup()?);
            }
            "bindings" => {
                self.bindings(&mut d.bindings)?;
            }
            "step" => {
                let step = self.step(&d.bindings)?;
                d.steps.push(step);
            }
            "sleep" | "wait" => return Err(ScenarioError::sleep(line)),
            _ => return Err(ScenarioError::unknown_property(line, key, "the `@scenario` body")),
        }
        Ok(())
    }

    fn setup(&mut self) -> Result<DraftSetup, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `setup`")?;
        let mut s = DraftSetup { state: None, window: None, seeded_files: Vec::new() };
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `setup` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(s);
                }
                Some((l, Tok::Ident(key))) => {
                    self.bump();
                    match key.as_str() {
                        "state" => {
                            if s.state.is_some() {
                                return Err(ScenarioError::new("DUPLICATE_PROPERTY", Some(l), None, "`state` declared twice"));
                            }
                            let v = self.collect_value("state")?;
                            let val = single_name(&v, "state")?;
                            if val != "fresh" {
                                return Err(ScenarioError::new(
                                    "INVALID_SETUP_STATE",
                                    Some(l),
                                    None,
                                    format!("only `fresh` initial state is defined in v1, found {val:?}"),
                                ));
                            }
                            s.state = Some(SetupState::Fresh);
                            self.expect_terminator("state")?;
                        }
                        "window" => {
                            if s.window.is_some() {
                                return Err(ScenarioError::new("DUPLICATE_WINDOW_SIZE", Some(l), None, "`window` declared twice"));
                            }
                            s.window = Some(self.window()?);
                        }
                        "seed" => {
                            let mut files = self.seed()?;
                            s.seeded_files.append(&mut files);
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &key, "the `setup` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in `setup` body"),
                    ))
                }
            }
        }
    }

    fn window(&mut self) -> Result<WindowSize, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `window`")?;
        let mut size: Option<WindowSize> = None;
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `window` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return size.ok_or_else(|| {
                        ScenarioError::new(
                            "MISSING_WINDOW_SIZE",
                            None,
                            None,
                            "`window` block requires `size: (width, height);`",
                        )
                    });
                }
                Some((l, Tok::Ident(key))) => {
                    self.bump();
                    match key.as_str() {
                        "size" => {
                            if size.is_some() {
                                return Err(ScenarioError::new("DUPLICATE_WINDOW_SIZE", Some(l), None, "`size` declared twice"));
                            }
                            self.expect_tok(&Tok::Colon, "`:` after `size`")?;
                            self.expect_tok(&Tok::LParen, "`(` opening the window size")?;
                            let w = self.expect_int("window width")?;
                            self.expect_tok(&Tok::Comma, "`,` between width and height")?;
                            let h = self.expect_int("window height")?;
                            self.expect_tok(&Tok::RParen, "`)` closing the window size")?;
                            self.expect_terminator("size")?;
                            let width = u64::try_from(w).map_err(|_| {
                                ScenarioError::new("BAD_WINDOW_SIZE", Some(l), None, "window width must be non-negative")
                            })?;
                            let height = u64::try_from(h).map_err(|_| {
                                ScenarioError::new("BAD_WINDOW_SIZE", Some(l), None, "window height must be non-negative")
                            })?;
                            size = Some(WindowSize { width, height });
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &key, "the `window` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in `window` body"),
                    ))
                }
            }
        }
    }

    fn seed(&mut self) -> Result<Vec<SeedFile>, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `seed`")?;
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `seed` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(out);
                }
                Some((l, Tok::Ident(key))) => {
                    self.bump();
                    match key.as_str() {
                        "file" => {
                            let (_, path) = self.expect_string("seed file path")?;
                            self.expect_tok(&Tok::Eq, "`=` between file and payload")?;
                            let (pl, kw) = self.expect_ident("`payload`")?;
                            if kw != "payload" {
                                return Err(ScenarioError::syntax(pl, format!("expected `payload`, found {kw:?}")));
                            }
                            let (_, payload) = self.expect_string("payload reference")?;
                            self.expect_terminator("seed entry")?;
                            out.push(SeedFile { path, payload });
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &key, "the `seed` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in `seed` body"),
                    ))
                }
            }
        }
    }

    fn bindings(&mut self, bindings: &mut BTreeMap<String, String>) -> Result<(), ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `bindings`")?;
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `bindings` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(());
                }
                Some((l, t)) => {
                    let key = match t {
                        Tok::Ident(ref s) | Tok::Str(ref s) => {
                            let s = s.clone();
                            self.bump();
                            s
                        }
                        _ => return Err(ScenarioError::syntax(l, format!("expected a binding key, found {t:?}"))),
                    };
                    self.expect_tok(&Tok::Colon, "`:` after the binding key")?;
                    let (vl, vt) = self.peek().ok_or_else(|| self.eof("a FAIR element ID after `:`"))?;
                    let value = match vt {
                        Tok::Ident(ref s) | Tok::Str(ref s) => {
                            let s = s.clone();
                            self.bump();
                            s
                        }
                        _ => return Err(ScenarioError::syntax(vl, format!("expected a FAIR element ID, found {vt:?}"))),
                    };
                    self.expect_terminator("binding")?;
                    if bindings.insert(key.clone(), value).is_some() {
                        return Err(ScenarioError::new(
                            "DUPLICATE_BINDING",
                            Some(l),
                            Some(key),
                            "binding declared twice",
                        ));
                    }
                }
            }
        }
    }

    fn step(&mut self, bindings: &BTreeMap<String, String>) -> Result<DraftStep, ScenarioError> {
        let (_, name) = self.expect_ident("step name")?;
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after step name")?;
        let mut st = DraftStep { name, trigger: None, input: None, expect: Vec::new() };
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the step block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(st);
                }
                Some((l, Tok::Ident(key))) => {
                    self.bump();
                    match key.as_str() {
                        "trigger" => {
                            if st.trigger.is_some() {
                                return Err(ScenarioError::new(
                                    "MULTIPLE_TRIGGERS",
                                    Some(l),
                                    Some(st.name.clone()),
                                    "a step has exactly one trigger",
                                ));
                            }
                            let v = self.collect_value("trigger")?;
                            st.trigger = Some(interpret_trigger(&v)?);
                            self.expect_terminator("trigger")?;
                        }
                        "input" => {
                            if st.input.is_some() {
                                return Err(ScenarioError::new(
                                    "DUPLICATE_INPUT",
                                    Some(l),
                                    Some(st.name.clone()),
                                    "a step has at most one `input` block",
                                ));
                            }
                            st.input = Some(self.step_input()?);
                        }
                        "expect" => {
                            let mut expect = self.expect_block(bindings)?;
                            st.expect.append(&mut expect);
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &key, "a step body")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in a step body"),
                    ))
                }
            }
        }
    }

    fn step_input(&mut self) -> Result<StepInput, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `input`")?;
        let mut input: Option<StepInput> = None;
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `input` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return input.ok_or_else(|| {
                        ScenarioError::new("EMPTY_INPUT", None, None, "`input` block requires a `value` property")
                    });
                }
                Some((l, Tok::Ident(key))) => {
                    self.bump();
                    match key.as_str() {
                        "value" => {
                            if input.is_some() {
                                return Err(ScenarioError::new(
                                    "DUPLICATE_INPUT",
                                    Some(l),
                                    None,
                                    "`value` declared twice in the `input` block",
                                ));
                            }
                            self.expect_tok(&Tok::Colon, "`:` after `value`")?;
                            let v = self.collect_value("input value")?;
                            input = Some(interpret_input(&v)?);
                            self.expect_terminator("input value")?;
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &key, "the `input` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in the `input` block"),
                    ))
                }
            }
        }
    }

    fn expect_block(&mut self, bindings: &BTreeMap<String, String>) -> Result<Vec<Expected>, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `expect`")?;
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `expect` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(out);
                }
                Some((l, Tok::Ident(kind))) => {
                    self.bump();
                    match kind.as_str() {
                        "ui" => {
                            let mut obs = self.ui_observations(bindings)?;
                            out.append(&mut obs);
                        }
                        "store" => {
                            let mut obs = self.store_observations()?;
                            out.append(&mut obs);
                        }
                        "effect" => {
                            let mut obs = self.effect_observations()?;
                            out.append(&mut obs);
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &kind, "the `expect` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in the `expect` block"),
                    ))
                }
            }
        }
    }

    fn ui_observations(&mut self, bindings: &BTreeMap<String, String>) -> Result<Vec<Expected>, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `ui`")?;
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `ui` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(out);
                }
                Some((l, Tok::Ident(word))) => {
                    self.bump();
                    match word.as_str() {
                        "visible" => {
                            let (_, text) = self.expect_string("visible text")?;
                            self.expect_terminator("ui visible assertion")?;
                            out.push(Expected::UiText { element: String::new(), text });
                        }
                        "input" => {
                            let (_, field) = self.expect_string("input field name")?;
                            self.expect_tok(&Tok::Eq, "`=` between field and value")?;
                            let (_, value) = self.expect_string("input value")?;
                            self.expect_terminator("ui input assertion")?;
                            let key = format!("input.{field}");
                            let element = bindings.get(&key).ok_or_else(|| {
                                ScenarioError::new(
                                    "BINDING_UNRESOLVED",
                                    Some(l),
                                    Some(key.clone()),
                                    format!("ui input observable cites {key:?}, which is not declared in the scenario bindings"),
                                )
                            })?;
                            out.push(Expected::UiInput { element: element.clone(), field, value });
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &word, "the `ui` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in the `ui` block"),
                    ))
                }
            }
        }
    }

    fn store_observations(&mut self) -> Result<Vec<Expected>, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `store`")?;
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `store` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(out);
                }
                Some((l, Tok::Ident(word))) => {
                    self.bump();
                    match word.as_str() {
                        "file" => {
                            let (_, path) = self.expect_string("store file path")?;
                            let (cl, kw) = self.expect_ident("`contains`")?;
                            if kw != "contains" {
                                return Err(ScenarioError::syntax(cl, format!("expected `contains`, found {kw:?}")));
                            }
                            let (kl, kw) = self.expect_ident("`key`")?;
                            if kw != "key" {
                                return Err(ScenarioError::syntax(kl, format!("expected `key`, found {kw:?}")));
                            }
                            let (_, key) = self.expect_string("store key")?;
                            self.expect_tok(&Tok::Eq, "`=` between key and value")?;
                            let value = self.expect_scalar()?;
                            self.expect_terminator("store assertion")?;
                            out.push(Expected::FileKey { element: String::new(), path, key, value });
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &word, "the `store` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in the `store` block"),
                    ))
                }
            }
        }
    }

    fn effect_observations(&mut self) -> Result<Vec<Expected>, ScenarioError> {
        self.skip_newlines();
        self.expect_tok(&Tok::LBrace, "`{` after `effect`")?;
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                None => return Err(self.eof("`}` to close the `effect` block")),
                Some((_, Tok::RBrace)) => {
                    self.bump();
                    return Ok(out);
                }
                Some((l, Tok::Ident(word))) => {
                    self.bump();
                    match word.as_str() {
                        "ipc" => {
                            let (_, channel) = self.expect_string("ipc channel")?;
                            self.expect_terminator("effect assertion")?;
                            out.push(Expected::EffectEmitted { element: String::new(), channel });
                        }
                        "sleep" | "wait" => return Err(ScenarioError::sleep(l)),
                        _ => return Err(ScenarioError::unknown_property(l, &word, "the `effect` block")),
                    }
                }
                Some((l, t)) => {
                    return Err(ScenarioError::syntax(
                        l,
                        format!("unexpected {t:?} in the `effect` block"),
                    ))
                }
            }
        }
    }

    fn expect_scalar(&mut self) -> Result<Scalar, ScenarioError> {
        match self.bump() {
            None => Err(self.eof("a scalar value")),
            Some((l, Tok::Str(s))) => Ok(Scalar::Str(s)),
            Some((l, Tok::Int(n))) => Ok(Scalar::Int(n)),
            Some((l, Tok::Ident(s))) if s == "true" => Ok(Scalar::Bool(true)),
            Some((l, Tok::Ident(s))) if s == "false" => Ok(Scalar::Bool(false)),
            Some((l, t)) => Err(ScenarioError::syntax(l, format!("expected a string, integer or boolean scalar, found {t:?}"))),
        }
    }
}

// Parser-local draft structures: resolution against the binding map happens
// once the whole scenario body has been read, so authoring order (bindings
// before or after steps) is free.

struct DraftScenario {
    name: String,
    target: Option<String>,
    format: Option<String>,
    revision: Option<u32>,
    setup: Option<DraftSetup>,
    bindings: BTreeMap<String, String>,
    steps: Vec<DraftStep>,
}

struct DraftSetup {
    state: Option<SetupState>,
    window: Option<WindowSize>,
    seeded_files: Vec<SeedFile>,
}

struct DraftStep {
    name: String,
    trigger: Option<Trigger>,
    input: Option<StepInput>,
    // `UiText`/`FileKey`/`EffectEmitted` carry an empty element until the
    // step's trigger element is resolved in `finalize`.
    expect: Vec<Expected>,
}

fn single_name(toks: &[(usize, Tok)], what: &str) -> Result<String, ScenarioError> {
    if toks.len() != 1 {
        let l = toks.first().map(|(l, _)| *l).unwrap_or(1);
        return Err(ScenarioError::syntax(l, format!("{what} takes exactly one value")));
    }
    match &toks[0].1 {
        Tok::Ident(s) | Tok::Str(s) => Ok(s.clone()),
        _ => Err(ScenarioError::syntax(toks[0].0, format!("{what} must be a name"))),
    }
}

fn single_int(toks: &[(usize, Tok)], what: &str) -> Result<i64, ScenarioError> {
    if toks.len() != 1 {
        let l = toks.first().map(|(l, _)| *l).unwrap_or(1);
        return Err(ScenarioError::syntax(l, format!("{what} takes exactly one value")));
    }
    match &toks[0].1 {
        Tok::Int(n) => Ok(*n),
        _ => Err(ScenarioError::syntax(toks[0].0, format!("{what} must be an integer"))),
    }
}

fn interpret_trigger(toks: &[(usize, Tok)]) -> Result<Trigger, ScenarioError> {
    let (line, head) = match toks.first() {
        Some((l, Tok::Ident(s))) => (*l, s.clone()),
        Some((l, t)) => {
            return Err(ScenarioError::new(
                "UNKNOWN_TRIGGER",
                Some(*l),
                None,
                format!("trigger must be an identifier from the v1 vocabulary, found {t:?}"),
            ))
        }
        None => return Err(ScenarioError::new("SYNTAX", None, None, "empty trigger")),
    };
    let first_seg = head.split('.').next().unwrap_or("");
    if first_seg == "sleep" || first_seg == "wait" {
        return Err(ScenarioError::sleep(line));
    }
    match head.as_str() {
        "ipc.send" => {
            if toks.len() < 2 {
                return Err(ScenarioError::syntax(line, "`ipc.send` requires a channel identifier"));
            }
            if toks.len() > 2 {
                return Err(ScenarioError::syntax(toks[2].0, "unexpected tokens after the ipc.send channel"));
            }
            match &toks[1].1 {
                Tok::Ident(channel) => Ok(Trigger::IpcSend { channel: channel.clone() }),
                _ => Err(ScenarioError::syntax(toks[1].0, "`ipc.send` channel must be an identifier")),
            }
        }
        "window" => {
            if toks.len() < 2 {
                return Err(ScenarioError::syntax(line, "`window` trigger requires an event identifier"));
            }
            if toks.len() > 2 {
                return Err(ScenarioError::syntax(toks[2].0, "unexpected tokens after the window event"));
            }
            match &toks[1].1 {
                Tok::Ident(event) => Ok(Trigger::Window { event: event.clone() }),
                _ => Err(ScenarioError::syntax(toks[1].0, "`window` event must be an identifier")),
            }
        }
        _ => {
            if toks.len() != 1 {
                return Err(ScenarioError::syntax(toks[1].0, "unexpected tokens after the trigger"));
            }
            let segs: Vec<&str> = head.split('.').collect();
            match segs.as_slice() {
                ["button", id, "activate"] => Ok(Trigger::Button { id: (*id).to_string() }),
                ["input", id, "commit"] => Ok(Trigger::Input { id: (*id).to_string() }),
                _ => Err(ScenarioError::new(
                    "UNKNOWN_TRIGGER",
                    Some(line),
                    None,
                    format!(
                        "trigger {head:?} is outside the v1 vocabulary (button.<id>.activate, input.<id>.commit, ipc.send <channel>, window <event>)"
                    ),
                )),
            }
        }
    }
}

fn interpret_input(toks: &[(usize, Tok)]) -> Result<StepInput, ScenarioError> {
    let (line, head) = match toks.first() {
        Some((l, Tok::Ident(s))) => (*l, s.as_str()),
        Some((l, t)) => {
            return Err(ScenarioError::new(
                "UNKNOWN_INPUT_TYPE",
                Some(*l),
                None,
                format!("input value must be typed (`value: text \"...\"` in v1), found {t:?}"),
            ))
        }
        None => return Err(ScenarioError::new("SYNTAX", None, None, "empty input value")),
    };
    match head {
        "text" => {
            if toks.len() != 2 {
                let l = toks.get(2).map(|(l, _)| *l).unwrap_or(line);
                return Err(ScenarioError::syntax(l, "`value: text` takes exactly one string"));
            }
            match &toks[1].1 {
                Tok::Str(value) => Ok(StepInput::Text { value: value.clone() }),
                _ => Err(ScenarioError::syntax(toks[1].0, "`value: text` requires a string")),
            }
        }
        other => Err(ScenarioError::new(
            "UNKNOWN_INPUT_TYPE",
            Some(line),
            None,
            format!("input type {other:?} is outside the v1 vocabulary (text only)"),
        )),
    }
}

fn finalize(draft: DraftScenario) -> Result<Scenario, ScenarioError> {
    let setup = draft.setup.ok_or_else(|| {
        ScenarioError::new(
            "MISSING_SETUP",
            None,
            None,
            "a scenario requires a `setup` block declaring its deterministic initial state",
        )
    })?;
    let state = setup.state.ok_or_else(|| {
        ScenarioError::new(
            "MISSING_SETUP_STATE",
            None,
            None,
            "`setup` requires `state: fresh;` — a scenario without declared initial state cannot be deterministic",
        )
    })?;
    let window = setup.window.ok_or_else(|| {
        ScenarioError::new(
            "MISSING_WINDOW_SIZE",
            None,
            None,
            "`setup` requires a `window` block with fixed geometry",
        )
    })?;
    let target = draft.target.ok_or_else(|| {
        ScenarioError::new("MISSING_TARGET", None, None, "a scenario requires a `target` property")
    })?;
    let _ = draft.format; // presence checked in scenario_item; absence is fine
    let _ = draft.revision;

    let mut steps = Vec::new();
    for st in draft.steps {
        let trigger = st.trigger.clone().ok_or_else(|| {
            ScenarioError::new(
                "MISSING_TRIGGER",
                None,
                Some(st.name.clone()),
                format!("step {:?} has no trigger", st.name),
            )
        })?;
        let key = trigger.binding_key();
        let step_element = draft.bindings.get(&key).ok_or_else(|| {
            ScenarioError::new(
                "BINDING_UNRESOLVED",
                None,
                Some(st.name.clone()),
                format!("trigger binding {key:?} is not declared in the scenario bindings"),
            )
        })?;
        let mut expect = st.expect;
        for e in &mut expect {
            if e.element().is_empty() {
                e.set_element(step_element.clone());
            }
            if let Expected::UiInput { element, field, .. } = e {
                match draft.bindings.get(&format!("input.{field}")) {
                    Some(v) if v == element => {}
                    _ => {
                        return Err(ScenarioError::new(
                            "BINDING_UNRESOLVED",
                            None,
                            Some(st.name.clone()),
                            format!(
                                "ui input observable cites element {element:?} but binding \"input.{field}\" is missing or points elsewhere"
                            ),
                        ))
                    }
                }
            }
        }
        steps.push(Step { name: st.name, trigger, input: st.input, expect });
    }

    let scenario = Scenario {
        format: REPLAY_CONTRACT.into(),
        revision: FORMAT_REVISION,
        name: draft.name,
        target,
        setup: Setup { state, window, seeded_files: setup.seeded_files },
        bindings: draft.bindings,
        steps,
    };
    scenario.validate().map_err(|mut errs| errs.remove(0))?;
    Ok(scenario)
}

// ---------------------------------------------------------------------------
// Trace documents
// ---------------------------------------------------------------------------

/// The record-kind vocabulary (v1), deliberately drawn from the FAIR
/// element-kind vocabulary so trace records and extracted elements speak the
/// same language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    /// Observable UI values at a barrier: text content, input values,
    /// control enabled/selected state — addressed by binding.
    UiState,
    /// Files/keys written or mutated: sandbox-relative path, normalised
    /// parsed value or canonical-byte SHA-256.
    Persistence,
    /// Outward actions: IPC message sent, child process spawned (with argv),
    /// OS integration call issued — kind and payload.
    SideEffect,
}

impl RecordKind {
    /// The canonical tag spelling.
    pub fn tag(&self) -> &'static str {
        match self {
            RecordKind::UiState => "ui_state",
            RecordKind::Persistence => "persistence",
            RecordKind::SideEffect => "side_effect",
        }
    }
}

/// Observable UI surface recorded at a barrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiObservable {
    /// Text content.
    Text,
    /// Committed input value.
    InputValue,
    /// Control enabled state.
    Enabled,
    /// Control selected state.
    Selected,
}

/// Outward side-effect kind (v1), aligned with the FAIR element-kind
/// vocabulary (`ipc channel`, `child_process`, `os_api_call`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectKind {
    /// Outbound IPC message.
    IpcMessage,
    /// Child process spawn.
    ChildProcess,
    /// OS integration call.
    OsIntegration,
}

/// Payload of a `ui_state` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiStateValue {
    /// Which observable surface this value came from.
    pub observable: UiObservable,
    /// The observed value.
    pub value: Scalar,
}

/// Payload of a `persistence` record. Exactly one of `value` (normalised
/// parsed value) or `sha256` (canonical-byte digest) is present; a keyed
/// entry carries `key` with `value`, a file-level digest carries neither.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistenceValue {
    /// Sandbox-relative path, normalised to scenario-relative form.
    pub path: String,
    /// Map key, for keyed entries.
    pub key: Option<String>,
    /// Normalised parsed value.
    pub value: Option<Scalar>,
    /// Lowercase hex SHA-256 over canonical bytes, for digest form.
    pub sha256: Option<String>,
}

/// Payload of a `side_effect` record: kind and payload. The kind determines
/// which payload field is present — `ipc_message` requires `channel`,
/// `child_process` requires `argv`, `os_integration` requires `call`; the
/// other fields must be absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SideEffectValue {
    /// The outward-action kind.
    pub effect: SideEffectKind,
    /// Channel, for `ipc_message`.
    pub channel: Option<String>,
    /// Normalised argv, for `child_process`.
    pub argv: Option<String>,
    /// Call name, for `os_integration`.
    pub call: Option<String>,
}

/// The value payload of a trace record. The variant always agrees with the
/// record's [`TraceRecord::kind`]; [`RecordValue::Ui`] carries `ui_state`,
/// [`RecordValue::Persistence`] carries `persistence`, and
/// [`RecordValue::SideEffect`] carries `side_effect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RecordValue {
    /// UI-state payload.
    Ui(UiStateValue),
    /// Persistence payload.
    Persistence(PersistenceValue),
    /// Side-effect payload.
    SideEffect(SideEffectValue),
}

/// One normalised state record, collected at a quiescence barrier. This type
/// is the *schema* only: nothing in this crate collects records (collection
/// requires executing applications under the draft contract's authorisation
/// conditions, none of which exist).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceRecord {
    /// Record kind.
    pub kind: RecordKind,
    /// FAIR element ID alignment key (opaque, non-empty; binding identity —
    /// never validated against a FAIR document here).
    pub element: String,
    /// Step index within the scenario: traces align per step, by step index,
    /// never by timestamp.
    pub step: u64,
    /// Occurrence index of this record within its barrier (step).
    pub occurrence: u64,
    /// Value payload; variant agrees with `kind`.
    pub value: RecordValue,
}

/// A normalised state trace: the ordered sequence of observable state
/// records one application emitted for one scenario run. Schema only —
/// collection is unimplemented (see the crate header).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    /// Trace format; must equal [`TRACE_FORMAT_VERSION`].
    pub trace_format: u64,
    /// Scenario name the trace was produced under.
    pub scenario: String,
    /// Scenario format revision the trace was produced under.
    pub scenario_revision: u64,
    /// Which application emitted this trace (for example `foreign` or
    /// `goya`).
    pub implementation: String,
    /// Corpus pin identity; traces are comparable only under the same pin.
    pub corpus_pin: String,
    /// Lowercase hex SHA-256 of the pinned build the trace was produced from.
    pub build_sha256: String,
    /// Declared observation-harness identity and version.
    pub harness: String,
    /// Ordered records; canonical output is sorted by
    /// (step, occurrence, kind, element).
    pub records: Vec<TraceRecord>,
}

/// A structural validation failure of a trace document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceError {
    /// Machine-readable failure code from the register (see [`ScenarioError`]).
    pub code: String,
    /// FAIR element ID the failure concerns, where applicable.
    pub element: Option<String>,
    /// Human-readable explanation.
    pub message: String,
}

impl TraceError {
    fn new(code: &str, element: Option<String>, message: impl Into<String>) -> Self {
        TraceError { code: code.into(), element, message: message.into() }
    }
}

impl Trace {
    /// Structural validation of a trace document: format and revision,
    /// non-empty provenance fields, well-formed build digest, and every
    /// record well-formed with a unique (step, element, occurrence) key.
    pub fn validate_trace(&self) -> Result<(), Vec<TraceError>> {
        let mut errs: Vec<TraceError> = Vec::new();
        if self.trace_format != TRACE_FORMAT_VERSION {
            errs.push(TraceError::new(
                "BAD_TRACE_FORMAT",
                None,
                format!("trace_format must be {TRACE_FORMAT_VERSION}, found {}", self.trace_format),
            ));
        }
        if self.scenario.trim().is_empty() {
            errs.push(TraceError::new("EMPTY_SCENARIO_NAME", None, "trace scenario must be non-empty"));
        }
        if self.scenario_revision != FORMAT_REVISION as u64 {
            errs.push(TraceError::new(
                "BAD_SCENARIO_REVISION",
                None,
                format!("scenario_revision must be {FORMAT_REVISION}, found {}", self.scenario_revision),
            ));
        }
        if self.implementation.trim().is_empty() {
            errs.push(TraceError::new("EMPTY_IMPLEMENTATION", None, "trace implementation must be non-empty"));
        }
        if self.corpus_pin.trim().is_empty() {
            errs.push(TraceError::new("EMPTY_CORPUS_PIN", None, "trace corpus_pin must be non-empty"));
        }
        if !is_lower_hex_sha256(&self.build_sha256) {
            errs.push(TraceError::new(
                "BAD_BUILD_DIGEST",
                None,
                "build_sha256 is not lowercase hex SHA-256",
            ));
        }
        if self.harness.trim().is_empty() {
            errs.push(TraceError::new("EMPTY_HARNESS", None, "trace harness must be non-empty"));
        }
        let mut seen = BTreeSet::new();
        for r in &self.records {
            if r.element.trim().is_empty() {
                errs.push(TraceError::new(
                    "EMPTY_ELEMENT_REF",
                    None,
                    "record element alignment keys must be non-empty FAIR element IDs",
                ));
                continue;
            }
            if !seen.insert((r.step, r.element.clone(), r.occurrence)) {
                errs.push(TraceError::new(
                    "DUPLICATE_RECORD_KEY",
                    Some(r.element.clone()),
                    format!(
                        "record (step {}, element {:?}, occurrence {}) appears more than once",
                        r.step, r.element, r.occurrence
                    ),
                ));
                continue;
            }
            let value_kind = match &r.value {
                RecordValue::Ui(_) => RecordKind::UiState,
                RecordValue::Persistence(_) => RecordKind::Persistence,
                RecordValue::SideEffect(_) => RecordKind::SideEffect,
            };
            if value_kind != r.kind {
                errs.push(TraceError::new(
                    "KIND_VALUE_MISMATCH",
                    Some(r.element.clone()),
                    format!(
                        "record kind {:?} does not agree with its {} payload",
                        r.kind.tag(),
                        value_kind.tag()
                    ),
                ));
            }
            match &r.value {
                RecordValue::Ui(_) => {}
                RecordValue::Persistence(p) => {
                    if p.path.trim().is_empty() {
                        errs.push(TraceError::new(
                            "EMPTY_PERSISTENCE_PATH",
                            Some(r.element.clone()),
                            "persistence path must be non-empty",
                        ));
                    }
                    match (&p.value, &p.sha256) {
                        (Some(_), None) | (None, Some(_)) => {}
                        _ => errs.push(TraceError::new(
                            "PERSISTENCE_VALUE_FORM",
                            Some(r.element.clone()),
                            "persistence carries exactly one of `value` (normalised parsed value) or `sha256` (canonical-byte digest)",
                        )),
                    }
                    match (&p.key, &p.value) {
                        (Some(k), Some(_)) if !k.trim().is_empty() => {}
                        (None, None) => {}
                        _ => errs.push(TraceError::new(
                            "PERSISTENCE_KEY_FORM",
                            Some(r.element.clone()),
                            "a keyed value entry carries a non-empty `key`; a file-level digest carries neither",
                        )),
                    }
                    if let Some(d) = &p.sha256 {
                        if !is_lower_hex_sha256(d) {
                            errs.push(TraceError::new(
                                "BAD_RECORD_DIGEST",
                                Some(r.element.clone()),
                                "record sha256 is not lowercase hex SHA-256",
                            ));
                        }
                    }
                }
                RecordValue::SideEffect(s) => {
                    let (present, code, field) = match s.effect {
                        SideEffectKind::IpcMessage => (&s.channel, "IPC_CHANNEL_REQUIRED", "channel"),
                        SideEffectKind::ChildProcess => (&s.argv, "CHILD_PROCESS_ARGV_REQUIRED", "argv"),
                        SideEffectKind::OsIntegration => (&s.call, "OS_CALL_REQUIRED", "call"),
                    };
                    if present.as_ref().map_or(true, |v| v.trim().is_empty()) {
                        errs.push(TraceError::new(
                            code,
                            Some(r.element.clone()),
                            format!("{:?} side effect requires a non-empty `{field}`", s.effect_tag()),
                        ));
                    }
                    let extra = match s.effect {
                        SideEffectKind::IpcMessage => s.argv.as_ref().map(|_| "argv").or_else(|| s.call.as_ref().map(|_| "call")),
                        SideEffectKind::ChildProcess => s.channel.as_ref().map(|_| "channel").or_else(|| s.call.as_ref().map(|_| "call")),
                        SideEffectKind::OsIntegration => s.channel.as_ref().map(|_| "channel").or_else(|| s.argv.as_ref().map(|_| "argv")),
                    };
                    if let Some(extra) = extra {
                        errs.push(TraceError::new(
                            "SIDE_EFFECT_FIELD_CONFLICT",
                            Some(r.element.clone()),
                            format!("{:?} side effect must not carry `{extra}`", s.effect_tag()),
                        ));
                    }
                }
            }
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Deterministic canonical serialisation: records sorted by
    /// (step, occurrence, kind, element), fixed field order, trailing
    /// newline. Storage order is preserved on the struct; only serialised
    /// output is canonicalised, as in `goya-fair`.
    pub fn canonical_json(&self) -> String {
        let mut sorted = self.clone();
        sorted.records.sort_by(|a, b| {
            (a.step, a.occurrence, a.kind.tag(), a.element.as_str()).cmp(&(
                b.step,
                b.occurrence,
                b.kind.tag(),
                b.element.as_str(),
            ))
        });
        let mut out = serde_json::to_string_pretty(&sorted).expect("trace serialises");
        out.push('\n');
        out
    }
}

impl SideEffectValue {
    fn effect_tag(&self) -> &'static str {
        match self.effect {
            SideEffectKind::IpcMessage => "ipc_message",
            SideEffectKind::ChildProcess => "child_process",
            SideEffectKind::OsIntegration => "os_integration",
        }
    }
}

// ---------------------------------------------------------------------------
// Comparator skeleton
// ---------------------------------------------------------------------------

/// One record present in both traces under the same
/// (step, element, occurrence) alignment key whose payloads differ; both
/// values are shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MismatchDetail {
    /// Step index of the barrier.
    pub step: u64,
    /// FAIR element ID alignment key.
    pub element: String,
    /// Occurrence index within the barrier.
    pub occurrence: u64,
    /// The expected trace's payload.
    pub expected: RecordValue,
    /// The actual trace's payload.
    pub actual: RecordValue,
}

/// Which trace a record was found in but not its counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnalignedSide {
    /// Present only in the expected trace.
    ExpectedOnly,
    /// Present only in the actual trace.
    ActualOnly,
}

/// One `unaligned` record: present in exactly one trace. Unaligned records
/// are never dropped from the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnalignedDetail {
    /// Step index of the barrier.
    pub step: u64,
    /// FAIR element ID alignment key.
    pub element: String,
    /// Occurrence index within the barrier.
    pub occurrence: u64,
    /// Record kind.
    pub kind: RecordKind,
    /// Which trace the record came from.
    pub side: UnalignedSide,
    /// The record's payload.
    pub value: RecordValue,
}

/// The complete diff of two traces. **This report derives nothing about FAIR
/// statuses**: `matched`/`mismatched`/`expected_only`/`actual_only` are
/// counts and details of the diff and nothing else. Turning them into an
/// element verdict requires a ratified GY-REPLAY/1 run under a recorded
/// authorisation *and* the GY-FAIR/1 amendment naming this contract as
/// evidence authority — see [`verified_is_unreachable`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Scenario the compared traces were produced under (from `expected`).
    pub scenario: String,
    /// Records aligned and equal.
    pub matched: usize,
    /// Records aligned but differing (details in [`ComparisonReport::mismatches`]).
    pub mismatched: usize,
    /// Records only the expected trace has (unaligned; details in
    /// [`ComparisonReport::unaligned`]).
    pub expected_only: usize,
    /// Records only the actual trace has (unaligned; details in
    /// [`ComparisonReport::unaligned`]).
    pub actual_only: usize,
    /// Per-record mismatch detail, in alignment-key order.
    pub mismatches: Vec<MismatchDetail>,
    /// Per-record unaligned detail, in alignment-key order.
    pub unaligned: Vec<UnalignedDetail>,
}

impl ComparisonReport {
    /// True when every record aligned with an equal counterpart. This is a
    /// statement about the diff only — not a scenario verdict (authored
    /// expectations are a separate, unimplemented report layer) and not a
    /// FAIR status.
    pub fn is_aligned(&self) -> bool {
        self.mismatched == 0 && self.expected_only == 0 && self.actual_only == 0
    }

    /// Total records classified. Per the contract, every record appears in
    /// the report under exactly one classification, so this equals
    /// `expected.records.len() + actual.records.len()` for valid traces.
    pub fn records_classified(&self) -> usize {
        self.matched + self.mismatched + self.expected_only + self.actual_only
    }
}

/// Align two traces and produce the complete diff. Alignment is per step, by
/// step index (never by timestamp), and within a step by FAIR element ID and
/// occurrence index — the v1 declared ordering. Records present in one trace
/// only are unaligned and count as mismatches in the honest-verdict sense;
/// a comparator that cannot align a record never drops it from the report.
///
/// Preconditions: both traces were produced under the same scenario revision,
/// the same corpus pin, and declared-compatible harness versions; both
/// validate ([`Trace::validate_trace`]). These comparability conditions are
/// the run record's responsibility and are not re-checked here. For invalid
/// traces (duplicate alignment keys) alignment is best-effort: the actual
/// trace's last record under a duplicated key wins.
///
/// The declared-substitution `not_compared` classification of the draft
/// contract is deferred with the (unimplemented) normalisation register:
/// nothing is excluded here, every record is classified exactly once.
pub fn compare(expected: &Trace, actual: &Trace) -> ComparisonReport {
    let mut index: BTreeMap<(u64, &str, u64), &TraceRecord> = BTreeMap::new();
    for r in &actual.records {
        index.insert((r.step, r.element.as_str(), r.occurrence), r);
    }
    let mut report = ComparisonReport {
        scenario: expected.scenario.clone(),
        matched: 0,
        mismatched: 0,
        expected_only: 0,
        actual_only: 0,
        mismatches: Vec::new(),
        unaligned: Vec::new(),
    };
    let mut expected_sorted: Vec<&TraceRecord> = expected.records.iter().collect();
    expected_sorted.sort_by(|a, b| {
        (a.step, a.occurrence, a.kind.tag(), a.element.as_str()).cmp(&(
            b.step,
            b.occurrence,
            b.kind.tag(),
            b.element.as_str(),
        ))
    });
    for rec in expected_sorted {
        match index.remove(&(rec.step, rec.element.as_str(), rec.occurrence)) {
            Some(a) if a.kind == rec.kind && record_values_equal(&rec.value, &a.value) => report.matched += 1,
            Some(a) => {
                report.mismatches.push(MismatchDetail {
                    step: rec.step,
                    element: rec.element.clone(),
                    occurrence: rec.occurrence,
                    expected: rec.value.clone(),
                    actual: a.value.clone(),
                });
                report.mismatched += 1;
            }
            None => {
                report.unaligned.push(UnalignedDetail {
                    step: rec.step,
                    element: rec.element.clone(),
                    occurrence: rec.occurrence,
                    kind: rec.kind,
                    side: UnalignedSide::ExpectedOnly,
                    value: rec.value.clone(),
                });
                report.expected_only += 1;
            }
        }
    }
    for ((step, element, occurrence), a) in &index {
        report.unaligned.push(UnalignedDetail {
            step: *step,
            element: (*element).to_string(),
            occurrence: *occurrence,
            kind: a.kind,
            side: UnalignedSide::ActualOnly,
            value: a.value.clone(),
        });
        report.actual_only += 1;
    }
    report
}

fn record_values_equal(a: &RecordValue, b: &RecordValue) -> bool {
    let sa = serde_json::to_string(a).expect("record value serialises");
    let sb = serde_json::to_string(b).expect("record value serialises");
    sa == sb
}

/// Always `true`, by construction, under the current programme state.
///
/// FAIR's `verified` state (behavioural equivalence demonstrated) is
/// reachable only after all three of: GY-REPLAY/1 ratified (it is a draft —
/// no scenario captured, no trace collected, no run authorised), execution
/// authorisation recorded for real runs (no such record exists), and the
/// GY-FAIR/1 revision naming GY-REPLAY/1 as the authorised replay-evidence
/// authority (not made). The draft is explicit: "Until both revisions are
/// ratified, tools must refuse to emit `verified`, and the refusal is a
/// validation rule, not a convention." This crate has no code path that
/// could mark an element `verified` — the comparator emits counts and
/// diffs only — and this function is that refusal, stated where a caller
/// might look for the upgrade. Callers must not branch on it expecting
/// `false` in this revision.
pub fn verified_is_unreachable() -> bool {
    true
}

// ---------------------------------------------------------------------------
// Explicit FAIR cross-check (not part of validate)
// ---------------------------------------------------------------------------

/// FAIR element IDs a scenario's bindings cite that are absent from the
/// given GY-FAIR/1 document, sorted, deduplicated. An explicit,
/// caller-initiated cross-check — `Scenario::validate` deliberately does
/// *not* consult a FAIR document (binding references are opaque here; the
/// per-application binding maps are run-record territory).
pub fn unknown_fair_elements(scenario: &Scenario, fair: &goya_fair::FairDocument) -> Vec<String> {
    let known: BTreeSet<&str> = fair.elements.iter().map(|e| e.id.as_str()).collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for v in scenario.bindings.values() {
        if !known.contains(v.as_str()) && seen.insert(v.clone()) {
            out.push(v.clone());
        }
    }
    out
}

/// FAIR element IDs a trace's records cite that are absent from the given
/// GY-FAIR/1 document, sorted, deduplicated. Same caveat as
/// [`unknown_fair_elements`].
pub fn unknown_fair_elements_in_trace(trace: &Trace, fair: &goya_fair::FairDocument) -> Vec<String> {
    let known: BTreeSet<&str> = fair.elements.iter().map(|e| e.id.as_str()).collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for r in &trace.records {
        if !known.contains(r.element.as_str()) && seen.insert(r.element.clone()) {
            out.push(r.element.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use goya_fair::{FairDocument, FairElement};

    /// The draft contract's illustrative scenario, made strict.
    const SOURCE: &str = r#"
// comment lines are ignored
@scenario timeline_filter_show {
  target: reactotron-app;
  setup {
    state: fresh;
    window { size: (1280, 800); }
    seed { file "arc/config.json" = payload "seed/config.json"; }
  }
  bindings {
    button.timeline: ui-0001;
    input.filter: input-0002;
  }
  step open_timeline {
    trigger: button.timeline.activate;
    expect { ui { visible "Timeline"; } }
  }
  step filter_styles {
    trigger: input.filter.commit;
    input { value: text "styles"; }
    expect {
      ui    { input "filter" = "styles"; }
      store { file "arc/filters.json" contains key "styles" = true; }
    }
  }
}
"#;

    fn err_code(res: Result<Scenario, ScenarioError>) -> String {
        res.expect_err("expected a scenario error").code
    }

    fn first_validation_code(s: &Scenario) -> String {
        match s.validate() {
            Err(errs) => errs[0].code.clone(),
            Ok(()) => panic!("expected validation errors"),
        }
    }

    fn validation_codes(s: &Scenario) -> Vec<String> {
        match s.validate() {
            Err(errs) => errs.iter().map(|e| e.code.clone()).collect(),
            Ok(()) => Vec::new(),
        }
    }

    #[test]
    fn parses_and_validates_the_illustration() {
        let s = parse_scenario(SOURCE).expect("illustration parses");
        assert_eq!(s.validate(), Ok(()));
        assert_eq!(s.name, "timeline_filter_show");
        assert_eq!(s.target, "reactotron-app");
        assert_eq!(s.setup.state, SetupState::Fresh);
        assert_eq!(s.setup.window, WindowSize { width: 1280, height: 800 });
        assert_eq!(s.setup.seeded_files.len(), 1);
        assert_eq!(s.bindings["button.timeline"], "ui-0001");
        assert_eq!(s.bindings["input.filter"], "input-0002");
        assert_eq!(s.steps.len(), 2);
        assert_eq!(s.steps[0].trigger.spelling(), "button.timeline.activate");
        assert_eq!(s.steps[0].expect[0].record_kind(), RecordKind::UiState);
        assert_eq!(s.steps[1].trigger, Trigger::Input { id: "filter".into() });
        assert_eq!(s.steps[1].input, Some(StepInput::Text { value: "styles".into() }));
        assert_eq!(s.steps[1].expect[0], Expected::UiInput {
            element: "input-0002".into(),
            field: "filter".into(),
            value: "styles".into(),
        });
        assert_eq!(s.steps[1].expect[1], Expected::FileKey {
            element: "input-0002".into(),
            path: "arc/filters.json".into(),
            key: "styles".into(),
            value: Scalar::Bool(true),
        });
    }

    #[test]
    fn ipc_and_window_triggers_parse() {
        let src = r#"
@scenario watcher {
  target: watcher-app;
  setup { state: fresh; window { size: (800, 600); } }
  bindings {
    ipc.filter-set: ipc-0003;
    window.close: win-0004;
  }
  step send_filter {
    trigger: ipc.send filter-set;
  }
  step close_window {
    trigger: window close;
  }
}
"#;
        let s = parse_scenario(src).expect("ipc/window scenario parses");
        assert_eq!(s.validate(), Ok(()));
        assert_eq!(s.steps[0].trigger, Trigger::IpcSend { channel: "filter-set".into() });
        assert_eq!(s.steps[0].trigger.binding_key(), "ipc.filter-set");
        assert_eq!(s.steps[1].trigger, Trigger::Window { event: "close".into() });
        assert_eq!(s.steps[1].trigger.spelling(), "window close");
    }

    #[test]
    fn unknown_trigger_rejected() {
        let src = SOURCE.replace("trigger: button.timeline.activate;", "trigger: menu.item.open;");
        assert_eq!(err_code(parse_scenario(&src)), "UNKNOWN_TRIGGER");
        let partial = SOURCE.replace("trigger: button.timeline.activate;", "trigger: button.timeline.press;");
        assert_eq!(err_code(parse_scenario(&partial)), "UNKNOWN_TRIGGER");
        let bare = SOURCE.replace("trigger: button.timeline.activate;", "trigger: tap;");
        assert_eq!(err_code(parse_scenario(&bare)), "UNKNOWN_TRIGGER");
    }

    #[test]
    fn sleep_trigger_rejected_as_unrepresentable() {
        let mut src = SOURCE.replace("trigger: button.timeline.activate;", "trigger: sleep 500ms;");
        assert_eq!(err_code(parse_scenario(&src)), "SLEEP_TRIGGER_UNREPRESENTABLE");
        src = SOURCE.replace("trigger: button.timeline.activate;", "trigger: wait.barrier.elapse;");
        assert_eq!(err_code(parse_scenario(&src)), "SLEEP_TRIGGER_UNREPRESENTABLE");
        src = SOURCE.replace(
            "trigger: button.timeline.activate;",
            "trigger: button.timeline.activate;\n    wait: 250;",
        );
        assert_eq!(err_code(parse_scenario(&src)), "SLEEP_TRIGGER_UNREPRESENTABLE");
    }

    #[test]
    fn missing_setup_and_incomplete_setup_rejected() {
        let scenario = |body: &str| {
            format!("@scenario s {{\n  target: t;\n{body}  bindings {{ button.a: ui-0001; }}\n  step one {{ trigger: button.a.activate; }}\n}}\n")
        };
        assert_eq!(err_code(parse_scenario(&scenario(""))), "MISSING_SETUP");        assert_eq!(err_code(parse_scenario(&scenario("  setup { window { size: (100, 100); } }\n"))), "MISSING_SETUP_STATE");
        assert_eq!(err_code(parse_scenario(&scenario("  setup { state: fresh; }\n"))), "MISSING_WINDOW_SIZE");
        assert_eq!(
            err_code(parse_scenario(&scenario("  setup { state: frozen; window { size: (100, 100); } }\n"))),
            "INVALID_SETUP_STATE"
        );
        assert_eq!(
            err_code(parse_scenario(&scenario("  setup { state: fresh; }\n  setup { state: fresh; window { size: (100, 100); } }\n"))),
            "DUPLICATE_SETUP"
        );
    }

    #[test]
    fn missing_and_multiple_triggers_rejected() {
        let text = "@scenario s {\n  target: t;\n  setup { state: fresh; window { size: (10, 10); } }\n  bindings { button.a: ui-0001; }\n  step one { input { value: text \"x\"; } }\n}\n";
        assert_eq!(err_code(parse_scenario(text)), "MISSING_TRIGGER");
        let text2 = "@scenario s {\n  target: t;\n  setup { state: fresh; window { size: (10, 10); } }\n  bindings { button.a: ui-0001; input.b: input-0002; }\n  step one {\n    trigger: button.a.activate;\n    trigger: input.b.commit;\n  }\n}\n";
        assert_eq!(err_code(parse_scenario(&text2)), "MULTIPLE_TRIGGERS");
    }

    #[test]
    fn duplicate_step_names_rejected() {
        let text = "@scenario s {\n  target: t;\n  setup { state: fresh; window { size: (10, 10); } }\n  bindings { button.a: ui-0001; }\n  step one { trigger: button.a.activate; }\n  step one { trigger: button.a.activate; }\n}\n";
        let s = parse_scenario(text).expect_err("duplicate step names are errors");
        assert_eq!(s.code, "DUPLICATE_STEP_NAME");
    }

    #[test]
    fn unknown_property_rejected() {
        let src = SOURCE.replace("target: reactotron-app;", "target: reactotron-app;\n  flavour: vanilla;");
        assert_eq!(err_code(parse_scenario(&src)), "UNKNOWN_PROPERTY");
        let src = SOURCE.replace("state: fresh;", "state: fresh;\n    patience: high;");
        assert_eq!(err_code(parse_scenario(&src)), "UNKNOWN_PROPERTY");
    }

    #[test]
    fn binding_failures_rejected() {
        let src = SOURCE.replace("trigger: button.timeline.activate;", "trigger: button.missing.activate;");
        assert_eq!(err_code(parse_scenario(&src)), "BINDING_UNRESOLVED");
        let src = SOURCE.replace("input.filter: input-0002;", "\"input.filter\": \"\";");
        assert_eq!(err_code(parse_scenario(&src)), "EMPTY_BINDING");
        let src = SOURCE.replace("bindings {", "bindings {\n    spindle.x: ui-0009;");
        assert_eq!(err_code(parse_scenario(&src)), "UNKNOWN_BINDING");
    }

    #[test]
    fn unknown_input_type_rejected() {
        let src = SOURCE.replace("value: text \"styles\";", "value: pointer \"styles\";");
        assert_eq!(err_code(parse_scenario(&src)), "UNKNOWN_INPUT_TYPE");
    }

    #[test]
    fn trailing_input_rejected() {
        let src = format!("{SOURCE}\n@scenario second {{ }}\n");
        assert_eq!(err_code(parse_scenario(&src)), "TRAILING_INPUT");
    }

    #[test]
    fn canonical_json_is_deterministic() {
        let reformatted = r#"
// same semantics, different layout: reordered bindings, extra comments,
// semicolon-free property at block end
@scenario timeline_filter_show {

  // corpus short name
  target: reactotron-app
  setup {
    state: fresh   // fresh sandbox profile, every run
    window { size: (1280, 800); }
  }
  bindings {
    input.filter: input-0002;
    button.timeline: ui-0001;
  }
  step open_timeline { trigger: button.timeline.activate; expect { ui { visible "Timeline"; } } }
  step filter_styles {
    trigger: input.filter.commit; input { value: text "styles"; }
    expect {
      store { file "arc/filters.json" contains key "styles" = true; }
      ui { input "filter" = "styles"; }
    }
  }
}
"#;
        let a = compile_scenario(SOURCE).expect("compiles");
        let b = compile_scenario(reformatted).expect("compiles");
        assert_eq!(a, b, "identical semantics must compile to byte-identical JSON");
        assert!(a.ends_with('\n'));
        assert_eq!(compile_scenario(&reformatted).unwrap(), b, "compilation is stable across runs");
    }

    #[test]
    fn canonical_json_round_trips() {
        let s = parse_scenario(SOURCE).expect("parses");
        let canonical = s.canonical_json();
        let reparsed: Scenario = serde_json::from_str(&canonical).expect("canonical JSON parses");
        assert_eq!(s, reparsed);
        assert_eq!(canonical, reparsed.canonical_json());
    }

    #[test]
    fn validate_rejects_empty_steps_and_unbound_expect_element() {
        let mut s = parse_scenario(SOURCE).expect("parses");
        s.steps.clear();
        assert_eq!(first_validation_code(&s), "NO_STEPS");

        let s = parse_scenario(SOURCE).expect("parses");
        let mut s2 = s.clone();
        s2.steps[0].expect.push(Expected::UiText {
            element: "ui-9999".into(),
            text: "Ghost".into(),
        });
        assert_eq!(validation_codes(&s2), vec!["ELEMENT_NOT_BOUND".to_string()]);
        let _ = &s;

        let mut s3 = parse_scenario(SOURCE).expect("parses");
        s3.steps[0].trigger = Trigger::Button { id: "ghost".into() };
        assert_eq!(validation_codes(&s3), vec!["BINDING_UNRESOLVED".to_string()]);

        let mut s4 = parse_scenario(SOURCE).expect("parses");
        s4.setup.window = WindowSize { width: 0, height: 800 };
        assert_eq!(validation_codes(&s4), vec!["BAD_WINDOW_SIZE".to_string()]);
    }

    #[test]
    fn fixture_compiles_to_canonical() {
        let raw = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/timeline_filter_show.gscenario"
        ))
        .expect("fixture present");
        let canonical = compile_scenario(&raw).expect("fixture compiles");
        assert!(canonical.ends_with('\n'));
        assert!(canonical.contains("\"format\": \"GY-REPLAY/1\""));
        let from_json: Scenario = serde_json::from_str(&canonical).expect("canonical parses");
        let direct = parse_scenario(&raw).expect("fixture parses");
        assert_eq!(from_json, direct);
        assert_eq!(canonical, direct.canonical_json());
    }

    // ---- trace schema ----

    fn base_trace(implementation: &str, records: Vec<TraceRecord>) -> Trace {
        Trace {
            trace_format: TRACE_FORMAT_VERSION,
            scenario: "timeline_filter_show".into(),
            scenario_revision: 1,
            implementation: implementation.into(),
            corpus_pin: "reactotron@corpus-1".into(),
            build_sha256: "a".repeat(64),
            harness: "declared harness v1".into(),
            records,
        }
    }

    fn ui_rec(step: u64, occ: u64, element: &str, observable: UiObservable, value: Scalar) -> TraceRecord {
        TraceRecord {
            kind: RecordKind::UiState,
            element: element.into(),
            step,
            occurrence: occ,
            value: RecordValue::Ui(UiStateValue { observable, value }),
        }
    }

    fn pers_rec(step: u64, occ: u64, element: &str, path: &str, key: &str, value: Scalar) -> TraceRecord {
        TraceRecord {
            kind: RecordKind::Persistence,
            element: element.into(),
            step,
            occurrence: occ,
            value: RecordValue::Persistence(PersistenceValue {
                path: path.into(),
                key: Some(key.into()),
                value: Some(value),
                sha256: None,
            }),
        }
    }

    fn side_rec(step: u64, occ: u64, element: &str, channel: &str) -> TraceRecord {
        TraceRecord {
            kind: RecordKind::SideEffect,
            element: element.into(),
            step,
            occurrence: occ,
            value: RecordValue::SideEffect(SideEffectValue {
                effect: SideEffectKind::IpcMessage,
                channel: Some(channel.into()),
                argv: None,
                call: None,
            }),
        }
    }

    #[test]
    fn trace_validates_and_canonicalises_sorted() {
        let t = base_trace(
            "foreign",
            vec![
                pers_rec(1, 0, "input-0002", "arc/filters.json", "styles", Scalar::Bool(true)),
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("Timeline".into())),
                side_rec(1, 1, "ipc-0003", "filter-set"),
                ui_rec(1, 0, "input-0002", UiObservable::InputValue, Scalar::Str("styles".into())),
            ],
        );
        assert_eq!(t.validate_trace(), Ok(()));
        let canonical = t.canonical_json();
        assert!(canonical.ends_with('\n'));
        // Canonical order: (step, occurrence, kind, element).
        let order: Vec<(u64, u64)> = {
            let parsed: Trace = serde_json::from_str(&canonical).unwrap();
            parsed.records.iter().map(|r| (r.step, r.occurrence)).collect()
        };
        assert_eq!(order, vec![(0, 0), (1, 0), (1, 0), (1, 1)]);
        // Input order must not matter.
        let shuffled = base_trace(
            "foreign",
            vec![
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("Timeline".into())),
                ui_rec(1, 0, "input-0002", UiObservable::InputValue, Scalar::Str("styles".into())),
                side_rec(1, 1, "ipc-0003", "filter-set"),
                pers_rec(1, 0, "input-0002", "arc/filters.json", "styles", Scalar::Bool(true)),
            ],
        );
        assert_eq!(canonical, shuffled.canonical_json());
        // JSON round-trip.
        let reparsed: Trace = serde_json::from_str(&canonical).unwrap();
        assert_eq!(reparsed, t);
    }

    #[test]
    fn validate_trace_rejects_malformed_records() {
        let mut t = base_trace("foreign", vec![ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("x".into()))]);
        t.build_sha256 = "ZZ".into();
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "BAD_BUILD_DIGEST");

        let t = base_trace(
            "foreign",
            vec![
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("x".into())),
                ui_rec(0, 0, "ui-0001", UiObservable::Enabled, Scalar::Bool(true)),
            ],
        );
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "DUPLICATE_RECORD_KEY");

        let bad_kind = TraceRecord {
            kind: RecordKind::Persistence,
            element: "ui-0001".into(),
            step: 0,
            occurrence: 0,
            value: RecordValue::Ui(UiStateValue { observable: UiObservable::Text, value: Scalar::Str("x".into()) }),
        };
        let t = base_trace("foreign", vec![bad_kind]);
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "KIND_VALUE_MISMATCH");

        let conflict = TraceRecord {
            kind: RecordKind::Persistence,
            element: "fs-0005".into(),
            step: 0,
            occurrence: 0,
            value: RecordValue::Persistence(PersistenceValue {
                path: "arc/filters.json".into(),
                key: Some("styles".into()),
                value: Some(Scalar::Bool(true)),
                sha256: Some("a".repeat(64)),
            }),
        };
        let t = base_trace("foreign", vec![conflict]);
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "PERSISTENCE_VALUE_FORM");

        let no_channel = TraceRecord {
            kind: RecordKind::SideEffect,
            element: "ipc-0003".into(),
            step: 0,
            occurrence: 0,
            value: RecordValue::SideEffect(SideEffectValue {
                effect: SideEffectKind::IpcMessage,
                channel: None,
                argv: None,
                call: None,
            }),
        };
        let t = base_trace("foreign", vec![no_channel]);
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "IPC_CHANNEL_REQUIRED");

        let empty_element = TraceRecord {
            kind: RecordKind::UiState,
            element: String::new(),
            step: 0,
            occurrence: 0,
            value: RecordValue::Ui(UiStateValue { observable: UiObservable::Text, value: Scalar::Str("x".into()) }),
        };
        let t = base_trace("foreign", vec![empty_element]);
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "EMPTY_ELEMENT_REF");

        let mut t = base_trace("foreign", vec![]);
        t.trace_format = 7;
        assert_eq!(t.validate_trace().unwrap_err()[0].code, "BAD_TRACE_FORMAT");
    }

    #[test]
    fn persistence_digest_form_is_validated() {
        let digest = TraceRecord {
            kind: RecordKind::Persistence,
            element: "fs-0005".into(),
            step: 0,
            occurrence: 0,
            value: RecordValue::Persistence(PersistenceValue {
                path: "arc/config.json".into(),
                key: None,
                value: None,
                sha256: Some("b".repeat(64)),
            }),
        };
        let t = base_trace("foreign", vec![digest]);
        assert_eq!(t.validate_trace(), Ok(()));
    }

    // ---- comparator ----

    #[test]
    fn compare_all_matched() {
        let expected = base_trace(
            "foreign",
            vec![
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("Timeline".into())),
                ui_rec(1, 0, "input-0002", UiObservable::InputValue, Scalar::Str("styles".into())),
                pers_rec(1, 1, "input-0002", "arc/filters.json", "styles", Scalar::Bool(true)),
            ],
        );
        let actual = base_trace(
            "goya",
            vec![
                pers_rec(1, 1, "input-0002", "arc/filters.json", "styles", Scalar::Bool(true)),
                ui_rec(1, 0, "input-0002", UiObservable::InputValue, Scalar::Str("styles".into())),
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("Timeline".into())),
            ],
        );
        let report = compare(&expected, &actual);
        assert_eq!(report.matched, 3);
        assert_eq!(report.mismatched, 0);
        assert_eq!(report.expected_only, 0);
        assert_eq!(report.actual_only, 0);
        assert!(report.is_aligned());
        assert_eq!(report.records_classified(), 6);
        assert_eq!(report.scenario, "timeline_filter_show");
    }

    #[test]
    fn compare_mismatch_detail_shows_both_values() {
        let expected = base_trace(
            "foreign",
            vec![ui_rec(1, 0, "input-0002", UiObservable::InputValue, Scalar::Str("styles".into()))],
        );
        let actual = base_trace(
            "goya",
            vec![ui_rec(1, 0, "input-0002", UiObservable::InputValue, Scalar::Str("stiles".into()))],
        );
        let report = compare(&expected, &actual);
        assert_eq!(report.matched, 0);
        assert_eq!(report.mismatched, 1);
        assert!(!report.is_aligned());
        let m = &report.mismatches[0];
        assert_eq!(m.step, 1);
        assert_eq!(m.element, "input-0002");
        assert_eq!(m.occurrence, 0);
        assert_eq!(
            m.expected,
            RecordValue::Ui(UiStateValue { observable: UiObservable::InputValue, value: Scalar::Str("styles".into()) })
        );
        assert_eq!(
            m.actual,
            RecordValue::Ui(UiStateValue { observable: UiObservable::InputValue, value: Scalar::Str("stiles".into()) })
        );
    }

    #[test]
    fn compare_expected_only_and_actual_only() {
        let expected = base_trace(
            "foreign",
            vec![
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("Timeline".into())),
                side_rec(1, 2, "ipc-0003", "filter-set"),
            ],
        );
        let actual = base_trace(
            "goya",
            vec![
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("Timeline".into())),
                pers_rec(1, 5, "fs-0005", "arc/extra.json", "k", Scalar::Int(1)),
            ],
        );
        let report = compare(&expected, &actual);
        assert_eq!(report.matched, 1);
        assert_eq!(report.mismatched, 0);
        assert_eq!(report.expected_only, 1);
        assert_eq!(report.actual_only, 1);
        assert_eq!(report.unaligned.len(), 2);
        let expected_only = report
            .unaligned
            .iter()
            .find(|u| u.side == UnalignedSide::ExpectedOnly)
            .expect("expected-only record reported");
        assert_eq!(expected_only.element, "ipc-0003");
        assert_eq!(expected_only.kind, RecordKind::SideEffect);
        let actual_only = report
            .unaligned
            .iter()
            .find(|u| u.side == UnalignedSide::ActualOnly)
            .expect("actual-only record reported");
        assert_eq!(actual_only.element, "fs-0005");
        assert_eq!(actual_only.kind, RecordKind::Persistence);
        // Every record classified exactly once.
        assert_eq!(report.records_classified(), 4);
    }

    #[test]
    fn compare_is_deterministic_in_report_order() {
        let expected = base_trace(
            "foreign",
            vec![
                ui_rec(2, 0, "ui-0001", UiObservable::Text, Scalar::Str("a".into())),
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("b".into())),
            ],
        );
        let actual = base_trace(
            "goya",
            vec![
                ui_rec(1, 3, "ui-0002", UiObservable::Enabled, Scalar::Bool(true)),
                ui_rec(1, 1, "ui-0003", UiObservable::Selected, Scalar::Bool(false)),
            ],
        );
        let r1 = compare(&expected, &actual);
        let r2 = compare(&actual, &expected);
        let keys = |r: &ComparisonReport| -> Vec<(u64, String, u64)> {
            r.unaligned.iter().map(|u| (u.step, u.element.clone(), u.occurrence)).collect()
        };
        assert_eq!(keys(&r1), vec![(0, "ui-0001".to_string(), 0), (2, "ui-0001".to_string(), 0)]);
        assert_eq!(keys(&r2), vec![(1, "ui-0002".to_string(), 1), (1, "ui-0003".to_string(), 3)]);
        assert_eq!(r1.records_classified(), 4);
        assert_eq!(r2.records_classified(), 4);
    }

    // ---- honesty and cross-checks ----

    #[test]
    fn verified_is_unreachable_is_true() {
        // The refusal is a validation rule, not a convention.
        assert!(verified_is_unreachable());
    }

    fn fair_doc(elements: &[&str]) -> FairDocument {
        FairDocument {
            contract: goya_fair::FAIR_CONTRACT.into(),
            project: goya_fair::ProjectProvenance {
                name: "reactotron-app".into(),
                frameworks: vec![],
                entry_points: vec![],
            },
            source: goya_fair::SourceManifest {
                root: "fixtures/sample-app".into(),
                files: vec![goya_fair::SourceFile { path: "src/main.ts".into(), sha256: "a".repeat(64) }],
            },
            extractor: goya_fair::ExtractorIdentity {
                name: "test".into(),
                version: "0.0.0".into(),
                id_rule: "test rule".into(),
            },
            elements: elements
                .iter()
                .map(|id| FairElement {
                    id: (*id).to_string(),
                    kind: goya_fair::ElementKind::Component,
                    provenance: goya_fair::ElementProvenance {
                        file: "src/main.ts".into(),
                        line: 1,
                        span_start: None,
                        span_end: None,
                        fallback_origin: false,
                    },
                    representability: goya_fair::Representability {
                        status: goya_fair::RepresentabilityStatus::Extracted,
                        reason: None,
                        detail: None,
                    },
                    semantics: BTreeMap::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn unknown_fair_elements_cross_check() {
        let s = parse_scenario(SOURCE).expect("parses");
        let fair = fair_doc(&["ui-0001"]);
        assert_eq!(unknown_fair_elements(&s, &fair), vec!["input-0002".to_string()]);
        let fair = fair_doc(&["ui-0001", "input-0002"]);
        assert!(unknown_fair_elements(&s, &fair).is_empty());

        let t = base_trace(
            "foreign",
            vec![
                ui_rec(0, 0, "ui-0001", UiObservable::Text, Scalar::Str("x".into())),
                side_rec(1, 0, "ipc-0003", "ch"),
            ],
        );
        assert_eq!(unknown_fair_elements_in_trace(&t, &fair), vec!["ipc-0003".to_string()]);
    }
}
