# NEVADA-2 — Pipeline integration: orchestration, target negotiation, diagnostics

Status: **Candidate**, 18 November 2026

## What it does

NEVADA-2 makes `goya-nevada` a pipeline-shaped library over Emperor's
canonical 13-stage model, without leaving the std-only, fail-closed,
deterministic regime established in NEVADA-1. Contract
`nevada.androidtv/0.0.2` (was `0.0.1` in NEVADA-1).

Implemented modules (all with exhaustive unit tests, clippy `-D warnings`
clean, rustfmt clean):

- `diagnostics.rs` — contract `nevada.diag/0.0.2`: stable diagnostic codes
  with explicit `#[repr(u16)]` discriminants, rendered as
  `GY-NV-<n>: <message>`. Codes: 501 focus-unreachable-essential,
  502 translation-refused, 503 capability-unsupported, 504 unknown-profile,
  505 missing-required-baseline, 506 feature-refused, 507 pipeline-blocked.
- `conformance.rs` — contract `nevada.conformance/0.0.2`: one fail-closed
  validator combining the translator's `conformance_report`, the focus
  engine's `unreachable_essential`, and DCR resolution. Emits a
  machine-readable report (`Pass` / `Fail` / `Refused`, diagnostics with
  stable codes, per-interaction refusal summaries, stage counts). Ready to
  become a quality gate in phase 3.
- `target.rs` — contract `nevada.target/0.0.2`: the `androidtv/0.0.2`
  target-negotiation profile. `DpadNavigation` and `SpatialFocus` are the
  always-granted baseline; `MotionPointer`, `VoiceSearch`,
  `GamepadNavigation` and `TouchScrolling` are granted only from census
  evidence resolved through `profiles::lookup` + `dcr::resolve`;
  `LeanbackManifest` and `CompanionScreen` are refused with
  `PhaseNotReached` until phase 3 codegen exists. Unknown profile ids fail
  the whole negotiation.
- `orchestrate.rs` — contract `nevada.pipeline/0.0.2`: `execute(PlanInput)
  -> Run` over all 13 canonical stage names (pinned by test against
  Emperor's list). Honest statuses: `effects-capabilities`,
  `target-negotiation` and `static-verification` really run and can Pass
  or Fail; `lexical-syntactic-admission`, `type-semantic-validation`,
  `canonical-goya-ir` and `optimisation` are `NOT_APPLICABLE` (Nevada
  admits no scenes); `native-linux-lowering`, `runtime-platform-linking`,
  `machine-code-generation`, `elf-assembly-linking`, `runtime-conformance`
  and `packaging-signing-release` are `BLOCKED` and never reported as
  passing. `Run::ok` is true only when no stage Failed.

Design decisions frozen by tests in this increment:

- **Baseline IME**: the Android TV on-screen keyboard is stock baseline
  software (spec §2), so `tv_keyboard` maps from `spatial_navigation`.
  `GY-NV-502` stays defensive for future targets without a system IME.
- **Output path is usability evidence**: a remote exposing native pointer
  events grants spatial pointer even without a declared
  accelerometer/gyroscope; sensor presence without an output path enables
  nothing (presence ≠ usability, in both directions).
- **Empty plan is never a pass**: an empty layout *and* empty interaction
  list refuse static verification (`GY-NV-507`).

## What it does NOT do

- No scene admission: Nevada still consumes no `goya-lang` output; the
  two admission stages are honestly `NOT_APPLICABLE`, not stubbed passes.
- No codegen, no APK/AAB, no runtime, no device contact.
- No quality gate: nothing in `quality-gates.toml` changes. The
  conformance report is a *future gate input*, not a gate.
- No runtime capability re-resolution: the compile-time census remains
  caller-supplied (gap ledger N0-7).
- No gate identifiers reserved beyond the diagnostics namespace above;
  gate numbering remains an open item in the gap ledger.

## Grammar reconciliation (open items carried from NEVADA-1)

1. Touch-interaction vocabulary is still a local enum in `translate.rs`,
   not `goya-lang` gesture declarations. Binding both is phase 3 work.
2. `PlanInput` takes hand-built `FocusElement` layouts; there is no
   scene-to-layout lowering.
3. `long_press_supported` remains hard-false from census derivation; if
   the grammar ever gains a long-press intent, the derivation must gain a
   census field with it.
4. `Orchestrate` statuses for stages Nevada cannot perform must be
   re-negotiated when scene admission lands — `NOT_APPLICABLE` may become
   `Fail` (unadmitted) rather than a free pass.
