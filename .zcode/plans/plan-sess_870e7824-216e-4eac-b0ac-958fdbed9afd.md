# Quitasol Q1 — Voice capability resolution over Nevada infrastructure

## Context
Quitasol Q1 ("shared Nevada television UX infrastructure") needs spatial focus, device capability resolution and remote fallback — but Nevada phase 0 already ships exactly those as deterministic modules in `crates/nevada` (focus graph `nevada.focus/0.0.1`, OEM profiles `nevada.profiles/0.0.1`, capability resolution `dcr.rs`). Nevada's gap ledger (docs/nevada/gap-ledger.md) reserves the shared scene→`FocusElement` lowering as its own phase-1 item (N0-FOCUSGRAPH). This plan therefore keeps `crates/nevada` and `crates/goya-lang` untouched: Quitasol gets a minimal, self-contained resolution layer inside `quitasol-semantic` that consumes Nevada as-is, referenced-not-claimed.

## 1. `crates/quitasol-semantic` — new `resolve.rs` module
- Add path dep on Nevada (package name per `crates/nevada/Cargo.toml` — confirm `goya-nevada` vs `nevada` at implementation time).
- **`scene_focus_elements(scene: &Scene, width_pt, height_pt) -> Vec<FocusElement>`**: lay out the scene with `goya_lang::layout` (millipoints already match Nevada's i64 geometry), convert every button/input node to a focusable `FocusElement` (id = layout index, rect from `DrawNode.rect`), `essential` false by default. Documented as a Quitasol-minimal variant; N0-FOCUSGRAPH remains the future shared lowering — no scene semantics (viewport policy, scroll containers) claimed beyond what `layout` already resolves.
- **`device_support(profile_id: &str) -> Result<DeviceSupport, Diagnostic>`**: look up the OEM profile via `nevada::profiles` (fail-closed `lookup`), run `nevada::dcr::resolve` on its census, project `InteractionSupport` into `DeviceSupport { voice_input, dpad, keyboard, spatial_pointer, gamepad }`. Unknown profile → `QS-E2000` diagnostic (new Quitasol code space; Nevada's ledger N0-DIAGNOSTICS notes NV-… codes are theirs, so Quitasol gets QS-…).
- **`resolve_manifest(scene, profile_id, width_pt, height_pt) -> Result<ResolvedManifest, Diagnostic>`**: run existing `compile` validation first (fail-closed), then per intent:
  - `fallback_remote`/`fallback_keyboard` targets must name an existing scene element (`Scene::find_node`); unknown target → `QS-E2001`.
  - Remote-fallback reachability: mark all declared remote-fallback targets as `essential: true` in the focus element list and run Nevada's `unreachable_essential` conformance — an unreachable remote fallback (the deterministic tier, the only tier guaranteed by the Q0 audit) is a `QS-E2002` diagnostic. Keyboard targets only require existence (keyboard navigation model is not claimed).
  - Record per-intent `fallback_resolution { remote: Option<Resolved>, keyboard: Option<Resolved> }` with `name`, `reachable: bool`; record the manifest-level `device { profile, interaction_support }`.
- Manifest JSON (`to_json`) gains `"device"` and per-intent `"fallback_resolution"` sections; serialisation stays hand-written and byte-deterministic. Contract/profile identifiers unchanged (`QUITASOL-INTENT-IR/1-draft`, `quitasol.intent.v0` — draft shape may change).
- Lowering tier stays `"unresolved"`: device support says what the *hardware* can express, never that Alexa integration exists (Q0 audit boundary).

## 2. Tests (in `quitasol-semantic`)
- Reachable fallback: two buttons laid out side by side, fallback target is one of them → resolved, reachable.
- Unreachable fallback: fallback target names a non-focusable element (a plain cell) → `QS-E2002`.
- Unknown fallback target → `QS-E2001`; unknown profile → `QS-E2000`.
- `oem.acme.voice-remote` vs `oem.generic.tv` profiles flip `voice_input` in the device section (Nevada's built-in profiles).
- Determinism: identical scene+profile → byte-identical JSON including new sections.
- Zero-intent scene + known profile still compiles with device section present.
- Nevada consumption smoke test: profile lookup failure is fail-closed.

## 3. Docs
- New **docs/quitasol/phase1.md**: phase record — what exists (resolution layer, QS-E200x codes), what is consumed from Nevada (focus/profiles/dcr, with contract identifiers), what remains BLOCKED (Vega track, Alexa contract, runtime re-resolution N0-RERESOLVE, shared N0-FOCUSGRAPH lowering). Follow phase0.md tone.
- Update **docs/quitasol/quitasol-overview.md** §3: OEM Capability Resolver + Remote Fallback Engine move from "contingent" to "draft implemented over nevada::dcr / nevada::focus"; §1 note Nevada phase 0 exists at `crates/nevada`.
- Update **docs/quitasol/phase0.md** verification block to include Q1 commands.
- Evidence record **docs/quitasol/programme/quitasol-q1.json** (sash-ent-002 convention): gates for library tests, Nevada-consumption determinism, BLOCKED entries (device census reality, Alexa contract, Vega).

## 4. Explicit non-goals
No changes to `crates/nevada` (Nevada programme owns N0-FOCUSGRAPH/N0-TRANSLATECTX), no `goya-lang` changes, no scene admission consuming `@intent`, no Alexa/VSK lowering, no runtime.

## 5. Verification
- `cargo test -p quitasol-semantic --offline` (all new + existing tests)
- `cargo test -p nevada --offline` (unchanged, proves no collision)
- `cargo clippy -p quitasol-semantic --all-targets --offline -- -D warnings`
- `cargo test -p goya-lang --offline` (regression, untouched but confirm)