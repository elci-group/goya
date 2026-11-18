# Execute Quitasol intent primitive (Phase 0)

## Context
The revised Quitasol directive ([docs/quitasol/intents.md](docs/quitasol/intents.md), [quitasol-overview.md](docs/quitasol/quitasol-overview.md)) adopts a first-class `intent` primitive (rejected: extending `gesture`). Exploration confirmed: GY-IR/1 freezes `Module`/`Node` (so goya-ir is untouched), repo convention is authoring syntax in `goya-lang` + a semantic-boundary sibling crate on the `maja-semantic` pattern (contract const, fail-closed profile, versioned diagnostics), and strict-profile gating via new GY-E error codes.

## 1. Authoring surface — `crates/goya-lang`
- **scene.rs**: add `IntentDecl { name, action: String, utterances: Vec<String>, confirmation: bool, fallback_remote: Option<String>, fallback_keyboard: Option<String> }` plus `SceneIntents` collection surfaced on `ParsedScene`/`Scene`, mirroring how `GesturePolicy` sits on `SceneCell`.
- **parser.rs**: add an `@intent Name { ... }` arm in `parse_cell_body`'s `AtWord` match (~line 956, following the `@interaction` pattern), with a dedicated `parse_intent` method: body properties `action:`, `voice { utterance: ..., confirmation: ... }`, `fallback { remote:, keyboard: }`. Push `SourceEntry` for the source map. New error codes GY-E1010 (bad intent shape), GY-E1011 (intent validation) in the existing GY-E1000..1009 block; **strict profiles (gssh1/2/3) reject `@intent`** exactly as they reject GSSH/2 constructs (GY-E1006 pattern).
- **intents.rs** (new module, quarantined docstring like `gestures.rs`: "Experimental, proposed not ratified"): pinned versioned utterance grammar v0 — `{slot}` placeholders, slot names must be idents, unknown/duplicate slots rejected, duplicate utterances rejected; required fallback enforcement (at least one of remote/keyboard); utterance must reference at least the action's declared slots when the action takes parameters (validation limited to what the AST carries).

## 2. Semantic Intent Compiler — new crate `crates/quitasol-semantic`
- New workspace member (Cargo.toml `[workspace.members]`, shared `version = "0.1.8"`, edition 2021, MIT OR Apache-2.0).
- **lib.rs**: consts `INTENT_IR_CONTRACT = "QUITASOL-INTENT-IR/1-draft"`, `INTENT_PROFILE = "quitasol.intent.v0"`; fail-closed: every diagnostic carries code + SourceRange; unknown inputs rejected, never defaulted.
- Consumes `SceneIntentDecl`s (via goya-lang types): validates and emits a **versioned intent manifest** — `emit_manifest(&[IntentDecl]) -> Result<IntentManifest, Diagnostic>` with a deterministic JSON serialisation (contract id, profile, sorted intent list, utterances, confirmation, fallback, per-intent lowering tier `Unresolved` — tier resolution is explicitly out of scope, recorded as such in the manifest).
- Validation passes: required fallback, utterance grammar, duplicate intent names, actions referenced must exist in the scene's action set, conflicting lowerings (an intent is limited to one voice tier + fallback — nothing here can conflict yet, so the check validates the declared tier set).

## 3. Contracts & docs
- **docs/contracts.md**: add `GY-INTENT/1-draft` entry describing provides/invariants/stability (Candidate, draft; shape may change; nothing ratified).
- **docs/quitasol/intents.md**: update §6 — IR representation decision taken for phase 0 (side-collection in goya-lang scene + separate draft manifest; GY-IR/1 untouched); mark implemented parts as phase-0 experimental.
- New **docs/quitasol/phase0.md**: short phase-0 record (what exists, what is BLOCKED: lowerings, runtime, owners/ratification) following `docs/sash/phase0/` convention.

## 4. Tests & fixtures
- **goya-lang corpus**: `tests/corpus/accept/` fixtures for valid `@intent`; `reject/` fixtures with header comments (`// profile:`, `// expect: GY-E101x`, `// at:`) for: missing fallback, duplicate utterance, bad slot grammar, `@intent` under strict profile, duplicate intent name. Follow existing `strict_corpus.rs` conventions.
- **quitasol-semantic tests**: unit tests for manifest determinism (identical inputs → byte-identical JSON), each validation failure path, and an end-to-end parse→emit test.
- Inline parser tests in goya-lang mirroring existing `@interaction` tests.

## 5. Verification
- `cargo test -p goya-lang --offline --tests`
- `cargo test -p quitasol-semantic --offline --lib --tests`
- `cargo clippy -p goya-lang -p quitasol-semantic --all-targets --offline -- -D warnings && cargo fmt -p goya-lang -p quitasol-semantic -- --check`
- Confirm existing strict corpus still passes (no regression to GSSH profiles): `cargo test -p goya-lang --offline` full.

## Not in scope
goya-ir changes, lowering to Alexa/VSK, runtime, quality gate 14, OEM capability resolver, app/store conformance.