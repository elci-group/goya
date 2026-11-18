# NEVADA-1 — Nevada phase-0 core subsystems: DCR, OEM profiles, interaction translation, spatial focus

Status: **Candidate**, 18 November 2026. This is the first increment of
the Nevada Android TV programme: four deterministic, std-only library
modules in a new crate. It is a proposed specification with a phase-0
library, not a compiler target.
Implementation: `crates/nevada` (`goya-nevada`, contract
`nevada.androidtv/0.0.1`), modules `src/dcr.rs`, `src/profiles.rs`,
`src/translate.rs`, `src/focus.rs`. No dependency on other Goya crates.

## What it does

- `dcr::resolve(census)` — Device Capability Resolver (contract
  `nevada.dcr/0.0.1`). Turns a caller-supplied `DeviceProfile` census
  (`HardwareCapability` presence list plus a `MotionOutput` path) into an
  `InteractionSupport` summary of eight interactions. D-pad is the only
  `Required` baseline; a census without one fails closed with
  `ResolveError::MissingRequiredBaseline`. Presence ≠ usability is the
  governing rule: a motion sensor (accelerometer or gyroscope) present
  without a usable output path enables nothing — `motion_gestures` and
  `spatial_pointer` require `MotionOutput::NativePointerEvents` or
  `MotionDataStream`; `MotionOutput::None` (the default) never enables
  them. Deterministic: the same census always yields the same result
  (test-pinned).
- `profiles::lookup(id)` — OEM Profile Registry (contract
  `nevada.profiles/0.0.1`). Four built-in immutable profiles, all
  version `1.0.0`: `oem.generic.tv` (D-pad only — the stock-hardware
  baseline), `oem.acme.pointer-remote` (D-pad + touchpad with native
  pointer events), `oem.acme.voice-remote` (D-pad + microphone),
  `oem.generic.gamepad` (D-pad + game controller + Bluetooth). Lookup is
  exact-id only: unknown ids are `LookupError::UnknownProfile`, malformed
  ids (no `family/version` shape) are
  `LookupError::MalformedProfileId`; there are no partial matches, no
  prefix or fuzzy matching, and no implicit defaults. Profiles are
  immutable static data, never mutated at runtime.
- `translate::translate(interaction, ctx)` — Interaction Translator
  (contract `nevada.translate/0.0.1`). The full ten-row touch→remote
  translation table (below). Outcomes are total and deterministic:
  `Translated { primary, fallbacks }` (all acceptable; primary is the
  best match for the device) or `Refused { reason }` — no silent loss of
  intent. `conformance_report` / `full_conformance_report` produce a
  deterministic, ordered split of translated and refused entries in
  canonical table order, ready to feed the future Conformance Validator.
- `focus::next_focus / next_focus_in_viewport / focus_graph /
  unreachable_essential` — Spatial Focus Engine (contract
  `nevada.focus/0.0.1`). Deterministic directional navigation over
  millipoint-integer (`i64`) rectangles with `i128` edge arithmetic (no
  overflow or panic possible). Navigation rule: candidates must be
  focusable, strictly forward on the primary axis, and overlapping on the
  perpendicular axis (half-open); ranked by primary distance, then
  perpendicular centre distance, then lower id. Initial focus is
  topmost-then-leftmost. `focus_graph` is computed by the same rule as
  per-query navigation, so graph and queries always agree.
  `unreachable_essential` performs a deterministic BFS and reports every
  essential element unreachable by directional navigation (fail-closed:
  an unknown or non-focusable start reports all essentials) — the
  future Conformance Validator's reachability check.
- 40 unit tests passing; `cargo clippy -D warnings` clean.

## Interaction translation table

The translator's left column is the touch/pointer vocabulary of a
direct-manipulation device; the right column is what a remote-control
device expresses. On a D-pad-only device (the stock-hardware baseline)
every row translates with zero refusals — remote-first, not remote-only:
capability-dependent rows pick better primaries when the hardware
exists, never worse ones than the D-pad fallback.

| Touch interaction | Remote-native translation | Notes |
| --- | --- | --- |
| Touch tap | Focus and select | Move focus to the tapped element, activate with D-pad OK/Enter. |
| Swipe | Directional navigation | D-pad up/down/left/right. |
| Scroll | Pointer scrolling when a spatial pointer exists; otherwise directional scrolling | `PointerScroll` primary + `DirectionalScroll` fallback with a pointer; `DirectionalScroll` alone without. |
| Hover | Focus state | Hover feedback becomes focus gained/lost. |
| Right click | Context action | Context menu via the remote menu button. |
| Drag and drop | Select, reposition, confirm | The remote equivalent of drag-and-drop; a three-step interaction, not a simulation. |
| Pinch zoom | Incremental zoom | Stepped zoom in/out; continuous two-finger scaling is a degradation, recorded as such. |
| Text input | TV keyboard / voice / connected keyboard | Preference order: connected keyboard, then voice, then on-screen TV keyboard; remaining methods kept as ordered fallbacks. Refused with `NoTextInputMethodAvailable` only when the device offers none. |
| Navigation drawer | Remote-accessible panel | A panel reachable by remote focus traversal. |
| Long press | Remote long press when supported; otherwise context action | `RemoteLongPress` primary + `ContextAction` fallback when the platform supports remote long-press; `ContextAction` alone otherwise. |

## What it does not do

No scene admission of any kind: the crate consumes typed Rust arguments,
not `.gscene` input, and no `goya-lang` admission consumes Nevada's
output. No grammar change: the Goya-language gesture/intent surface is
untouched (see below). No artefacts: no APK, AAB, Kotlin emission,
manifest, or any build output. No pipeline integration: no orchestrate
module, no Emperor stage wiring, no `androidtv` target-negotiation
profile, no gates. No runtime: no D1 leanback runtime, no device I/O, no
on-device capability detection — the census is caller-supplied by
design, and the resolver "does not infer hardware facts from OS labels".
No diagnostic code space: refusals are typed enums, not yet stable
`NV-…` codes. No OEM caprichios: the registry is hand-authored static
data, not a Caprichio family. The four modules are mutually independent
by design (the translator mirrors capability concepts locally rather
than importing `dcr`); closing that seam is phase 1 work, recorded in
the gap ledger.

## Grammar reconciliation (open item)

The Goya-language hook already exists in part: `IntentDecl`
(`crates/goya-lang/src/scene.rs`) carries `fallback_remote:
Option<String>` and `fallback_keyboard: Option<String>`, and
`validate_intent_decl` (`crates/goya-lang/src/intents.rs`, code
`GY-E1011`) requires a non-voice fallback for every intent — voice is
additive, which is exactly Nevada's remote-first principle expressed in
the language today. But that module is marked "proposed, not ratified":
no strict admission consumes it, and the shape may change.

Open items to resolve before phase 1 wires scenes to Nevada:

1. **Gesture declarations vs intent declarations.** The translation
   table operates on touch *interactions* (`TouchInteraction`); the
   language currently offers *intents* (`IntentDecl` → `@interaction`)
   and no gesture grammar. Whether gestures become scene syntax, part of
   `@interaction` blocks, or a separate declaration kind is undecided.
   Unimplemented gesture admission is currently rejected by goya-lang
   strict admission, and it must stay rejected until this is resolved.
2. **`fallback_remote` semantics.** `IntentDecl::fallback_remote` names
   a remote-navigable target; Nevada's translator produces
   `RemoteInteraction` values. The link between an authored fallback
   target and a translated interaction (and whether translation may
   override or contradict an authored fallback) is undefined.
3. **Essential elements.** `FocusElement::essential` is a caller flag
   with no scene source. Whether essentiality is authored, derived from
   intent bindings, or derived from interaction declarations must be
   decided before reachability conformance means anything.
4. **Contract status.** Adding ratified gesture/intent grammar to
   `goya-lang` is a language revision that must go through
   `docs/contracts.md` approval, not an incremental patch. No grammar
   change was made in this increment; the audit trail is recorded here
   and in the roadmap.

These are open items, not decisions. Nothing in this increment consumes
or constrains them beyond recording that they exist.
