# Nevada roadmap — Android TV compiler target

Status: proposed, 18 November 2026 (phase 0 complete; phase 2 complete
as NEVADA-2, absorbing the conformance-validator item from phase 1;
phase 3 not started). This is a plan, not evidence. Nothing on this page claims an
Android TV build, a leanback APK/AAB, an admitted scene, a D1 leanback
runtime, or a conformance gate that does not yet exist.

Nevada is Goya's proposed Android TV compiler target — "hardware-aware UX
compilation". It translates application intent expressed in the
touch/pointer vocabulary into remote-first interactions: the translator
maps *what the user means* onto what the device can actually express,
never *how the original platform implemented it*. Its governing
principles are: stock hardware first; remote-first, not remote-only;
deterministic focus; functional parity with the touch-authored
application; graceful degradation; and explicit incompatibility
(fail-closed compile reports — a refused translation is an error, never a
silent drop).

The crate exists: `crates/nevada` (`goya-nevada`, contract
`nevada.androidtv/0.0.1`) compiles with four modules and passing tests.
Everything beyond it is unbuilt.

## Reconciliation with the repository as it exists

The proposal was reconciled against the repository on 18 November 2026:

| Proposal assumption | Repository reality |
| --- | --- |
| Nevada translates Goya scenes to Android TV | `crates/nevada` is a pure, std-only library. It admits no scenes, consumes no `goya-lang` output, and produces no artefacts. It is four deterministic subsystems with unit tests. |
| Goya-lang already models remote intent | True in part: `IntentDecl` (crates/goya-lang/src/scene.rs) already carries `fallback_remote: Option<String>` and `fallback_keyboard`, and `validate_intent_decl` (crates/goya-lang/src/intents.rs, `GY-E1011`) requires a non-voice fallback — voice is additive. But this module is marked "proposed, not ratified"; no strict admission consumes it. |
| Device capabilities are discoverable at compile time | No Android TV device census exists in the repo. `dcr.rs` consumes a caller-supplied `DeviceProfile` census and explicitly does not infer hardware facts from OS labels. Where real censuses come from is an open item. |
| Android TV codegen reuses Aragon's infrastructure | The Aragon Android infrastructure exists (`crates/goya-sdk`: android_scene.rs, kotlin.rs, android_d1_runtime.rs, android_button_preview.rs, application.rs, bin/aragon-androidc.rs, bin/aragon-android-project.rs), but it targets phones/tablets; nothing in it emits leanback/TV artefacts, and Nevada does not call it yet. |
| OEM hardware routes through Caprichios | `goya-caprichios` exists and has a proven pattern (typed declaration → canonical IR → fail-closed lowering → factory, per the Blue Moon programme), but no Nevada caprichio module exists. |
| The pipeline is Emperor's 13 stages | `emperor::stage::NAMES` defines the 13 stages (lexical-syntactic-admission … packaging-signing-release), but Nevada has no orchestrate module and no target-negotiation profile `androidtv`; gates for `runtime-conformance` on TV are undefined. |
| Unimplemented gestures can be declared | `goya-lang` strict admission currently rejects the gesture/intent surface Nevada would translate; scene admission is a later phase and no grammar change was made. |

The implementing strategy therefore follows the repository's own proven
pattern first: bounded, deterministic, fail-closed, std-only construction
modules with exhaustive tests — the pattern already shipped by Sash
admission, the Blue Moon caprichios and goya-caprichios. Android TV
codegen, real device censuses and OEM caprichios are later phases, and no
phase claims them before evidence exists.

## Definition of done for the programme

A Nevada phase is done when it has executable tests, deterministic
outputs and documented evidence that name exactly what is and is not
proven: no translated interaction reported as parity without a
conformance gate, no capability claimed from presence alone, no artefact
generated before a profile exists.

## Phases

### Phase 0 — Core subsystems (complete 18 November 2026)

Implemented as NEVADA-1 (`docs/nevada/NEVADA-1.md`), `crates/nevada`
(`goya-nevada`), contract `nevada.androidtv/0.0.1`:

- [x] `dcr.rs` — Device Capability Resolver, contract `nevada.dcr/0.0.1`:
      `HardwareCapability` census → `InteractionSupport` resolution. D-pad
      is the only `Required` baseline; a missing D-pad fails closed with
      `ResolveError::MissingRequiredBaseline`. Presence ≠ usability: a
      motion sensor without a usable `MotionOutput` path enables nothing.
- [x] `profiles.rs` — OEM Profile Registry, contract
      `nevada.profiles/0.0.1`: four built-in immutable profiles
      (`oem.generic.tv`, `oem.acme.pointer-remote`, `oem.acme.voice-remote`,
      `oem.generic.gamepad`, all `/1.0.0`); exact-id lookup only, no
      partial matches, no implicit defaults.
- [x] `translate.rs` — Interaction Translator, contract
      `nevada.translate/0.0.1`: the full ten-row touch→remote translation
      table, `Translated{primary, fallbacks}` / `Refused{reason}`
      outcomes, and a deterministic ordered `conformance_report`.
- [x] `focus.rs` — Spatial Focus Engine, contract `nevada.focus/0.0.1`:
      deterministic `next_focus` with documented tie-breaks (primary
      distance, perpendicular centre distance, lower id), millipoint
      `i64` geometry with `i128` edge arithmetic, `focus_graph`
      precomputation that always agrees with per-query navigation, and
      `unreachable_essential` BFS for future fail-closed conformance.
- [x] 40 unit tests passing; clippy clean with `-D warnings`.
- [x] This roadmap, the gap ledger (`docs/nevada/gap-ledger.md`), the
      threat model (`docs/nevada/threat-model.md`) and NEVADA-1.

**What phase 0 does NOT do**: no scene admission, no goya-lang grammar
change, no pipeline integration, no codegen, no artefacts, no runtime, no
gates. The subsystems are pure Rust with tests; nothing consumes them yet.

### Phase 1 — Conformance validation and language hooks

- [x] Conformance validator module (`conformance.rs`): combine
      `conformance_report` + `unreachable_essential` + DCR resolution into
      one fail-closed validator with machine-readable
      PASS/FAIL/REFUSED results, ready to become a gate. *Delivered in
      phase 2 (NEVADA-2) because `orchestrate.rs`'s static-verification
      stage consumes it.*
- [ ] Wire the translation and focus checks against real parsed scene
      data — requires a scene ingestion path (see gap ledger row on
      goya-lang strict admission); the gesture/intent grammar
      reconciliation items in NEVADA-1 must be resolved first.
- [ ] Runtime capability re-resolution contract: define how a compile-time
      census is re-checked on device and what happens when they disagree
      (currently TBD, gap ledger).
- [x] Deterministic `DeviceCapabilities` derivation from
      `InteractionSupport` — delivered in `conformance.rs::derive_device_capabilities`
      (NEVADA-2).

**What it does NOT do**: no pipeline stage, no codegen, no gates wired
into a compiler binary.

### Phase 2 — Pipeline integration

- [x] `conformance.rs` (absorbed from phase 1): the fail-closed validator
      with machine-readable PASS/FAIL/REFUSED results, stable
      `GY-NV-50x` diagnostic codes, and deterministic
      `DeviceCapabilities` derivation from `InteractionSupport`
      (NEVADA-2).
- [x] `orchestrate.rs`: Nevada as a pipeline over Emperor's 13 stages
      (`crates/emperor/src/stage.rs::NAMES`), implementing the subset
      Nevada needs: lexical-syntactic-admission, type-semantic-validation,
      effects-capabilities (DCR + profiles), target-negotiation,
      static-verification (conformance validator), and explicit
      NOT_APPLICABLE / BLOCKED statuses for the stages Nevada cannot yet
      perform (native-linux-lowering, machine-code-generation,
      elf-assembly-linking, packaging-signing-release).
- [x] Target-negotiation profile `androidtv/0.0.2` with fail-closed
      refusal of unsupported feature requests (NEVADA-2).
- [x] Explicit incompatibility compile reports: refusals surfaced as
      diagnostics with stable codes (GY-NV-501…507), never silent loss
      (NEVADA-2).
- [ ] Gate numbering: define Nevada's gate identifiers and their position
      relative to the existing gates; no gate exists today (gap ledger).

**What it does NOT do**: no Android TV emission, no APK/AAB, no runtime.

### Phase 3 — Codegen via Aragon infrastructure

- [ ] Leanback APK/AAB emission reusing `crates/goya-sdk` (android_scene,
      kotlin, android_d1_runtime, aragon-androidc, aragon-android-project)
      extended for Android TV: leanback manifest, banner, D-pad-first
      generated Kotlin interaction layer.
- [ ] A D1 leanback runtime (today `android_d1_runtime.rs` targets phone
      form factors; TV needs its own focus/intent runtime contract).
- [ ] A new quality gate for TV conformance (reachability of essential
      elements, translation parity evidence, capability honesty), wired as
      a blocking gate — not informational.
- [ ] OEM-specific hardware routing through goya-caprichios-style
      Caprichios: typed declarations, canonical IR, fail-closed lowering,
      generated factories — the Blue Moon pattern.
- [ ] Parity hooks: per-interaction evidence that the remote-authored
      behaviour preserves the touch-authored intent (`IntentDecl` /
      `fallback_remote` as the existing language anchor).

**What it does NOT do**: nothing in this phase is scheduled against real
Android TV hardware; on-device qualification (leanback launcher
behaviour, real OEM remotes, voice services) is blocked until hardware is
provisioned, and remains an unscoped follow-on phase. No phase claims
runtime conformance before a device runs the artefact.

## Current supported surface

Exactly four pure library modules in `crates/nevada` under contract
`nevada.androidtv/0.0.1`, with sub-contracts `nevada.dcr/0.0.1`,
`nevada.profiles/0.0.1`, `nevada.translate/0.0.1` and
`nevada.focus/0.0.1`. No scene is admitted, no diagnostic code space is
assigned, no artefact is generated, no gate exists, and none may be
reported as such. Nevada is a proposed specification with a phase-0
library, not a compiler target yet.
