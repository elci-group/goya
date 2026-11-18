# Quitasol Q2 — Fire OS integration emission (Media Session track)

## Context
The Q0 audit established that the **only self-serve, documented Fire OS voice integration** is the Media Session transport path (play/pause, offset seek, skip next/previous, restart — via the `com.amazon.permission.media.session.voicecommandcontrol` manifest permission) plus automatic app launch by name. VSK/search is closed to new partners; in-app voice input does not exist. Q2 therefore emits a deterministic **Fire OS integration plan** from a resolved manifest: which Android manifest fragments an app needs and which intents can lower to MediaSession transport actions — with everything else honestly recorded as `Unresolved`, never silently claimed.

Working state check first: concurrent sessions moved branches (`main` is now checked out, clean); step 0 verifies the Q1 artifacts are present on `main` before anything builds on them.

## 1. `crates/goya-lang` — optional `transport` key on `@intent` (draft surface)
Quitasol needs an authoring hook to declare *which* MediaSession transport action an intent represents. Added inside the intent body, compat-parser only (strict GSSH profiles already reject all `@intent` with GY-E1010, so no admission change):
- **scene.rs**: `pub enum TransportClass { PlayPause, SkipNext, SkipPrevious, SeekForward, SeekBackward, Restart }` (exactly the six documented MediaSession behaviours; no absolute seek — Q0: unsupported), plus `pub transport: Option<TransportClass>` on `IntentDecl`.
- **parser.rs** (`parse_intent`): optional key `transport: play_pause|skip_next|skip_previous|seek_forward|seek_backward|restart;` after `action:`; unknown value or duplicate key → parse error (fail-closed, matching existing intent-body behaviour). Default remains `None`.
- Update `IntentDecl` construction sites (`parse_intent`, any tests constructing it literally).

## 2. `crates/quitasol-semantic` — new `fireos.rs` module
- `pub const FIRE_OS_PLAN_CONTRACT: &str = "QUITASOL-FIREOS-PLAN/1-draft";` — draft, proposed not ratified.
- `pub struct TransportAction { pub action: &'static str }` — Android MediaSession callback names (`ACTION_PLAY_PAUSE`, `ACTION_SKIP_TO_NEXT`, `ACTION_SKIP_TO_PREVIOUS`, `ACTION_SEEK_TO`+offset, `ACTION_RESTART`-equivalent), mapped 1:1 from `TransportClass`.
- `pub enum IntentLowering { MediaSessionTransport(Vec<&'static str>), LaunchOnly, Unresolved }` — an intent with a `transport` declaration lowers to its MediaSession actions; an intent without transport **and** whose action's trigger is an app-level button stays `Unresolved` (search/discovery would need VSK = closed per Q0). `LaunchOnly` is recorded only at plan level (automatic for every app).
- `pub struct FireOsPlan { contract, profile (quitasol.intent.v0), permission_entries: Vec<PermissionEntry>, launch: LaunchSupport, intents: Vec<(name, IntentLowering)> }`:
  - `permission_entries` contains `com.amazon.permission.media.session.voicecommandcontrol` iff ≥1 intent declares transport (Q0: not required on Fire OS 14+ — recorded as a note field, not a claim).
  - `to_json()` — hand-written, byte-deterministic, reusing `push_json_string`/`push_entry_json` patterns.
- `pub fn emit_fire_os_plan(scene: &Scene, resolved: &ResolvedManifest) -> Result<FireOsPlan, IntentDiagnostic>`: fail-closed — requires the resolved manifest's device/fallback resolution (Q1) to have succeeded; deterministic ordering by intent name.
- Lowering tier honesty: plan-level `lowering_tier` stays `"unresolved"` — a transport mapping is a *candidate* binding, not a runtime; no APK is produced and no conformance is claimed.

## 3. Tests
- goya-lang: `transport` parses into `IntentDecl` (each class), unknown value rejected, duplicate rejected, absence → `None`; intent corpus mutation sweep still total.
- quitasol-semantic: plan emission with mixed transport/unresolved intents; permission present iff transport declared; deterministic JSON (byte-identical across runs and declaration order); fail-closed ordering (compile errors precede); zero-intent scene → empty plan with launch entry only.

## 4. Docs
- **docs/quitasol/phase2.md**: phase record following phase1.md tone — what exists, Q0 anchoring (Media Session = only self-serve tier), BLOCKED table (no APK build, no MediaSession runtime, Fire OS 14 permission note unverified on device, Vega track untouched).
- **docs/contracts.md**: extend the GY-INTENT/1-draft entry with the `transport` key (draft surface).
- **docs/quitasol/quitasol-overview.md**: note Fire OS plan emission exists (Q2).
- **docs/quitasol/programme/quitasol-q2.json**: evidence record (gates: library tests PASS, plan determinism PASS; BLOCKED: device verification, runtime, APK emission).

## 5. Non-goals
No APK/AAB build, no Android project generation (Aragon/goya-sdk owns phone-track codegen; Nevada ledger N0-CODEGEN owns TV emission), no changes to `crates/nevada`, no runtime, no claim that transport intents work on-device.

## 6. Verification
- `cargo test -p goya-lang --offline` (incl. intent corpus)
- `cargo test -p quitasol-semantic --offline`
- `cargo clippy -p goya-lang -p quitasol-semantic --all-targets --offline -- -D warnings`
- Commit + push via kaptaind (daemon running; branch `main` — confirm with user implicit in established workflow).