# Quitasol Q1 — Voice capability resolution over Nevada

Status: **proposed, not ratified**. Implementation is experimental; it does
not admit, ratify or freeze any profile. Phase 0 record:
[phase0.md](phase0.md); design: [intents.md](intents.md); Q0 evidence:
[q0-capability-audit.md](q0-capability-audit.md).

## What Q1 delivers

Quitasol's first three subsystems now have draft implementations inside
`crates/quitasol-semantic` (`src/resolve.rs`), consuming the Nevada
television infrastructure as-is:

- **OEM Capability Resolver** — `device_support(profile_id)` resolves an
  OEM profile through `nevada::profiles` (fail-closed exact-id lookup,
  contract `nevada.profiles/0.0.1`) and `nevada::dcr::resolve` (contract
  `nevada.dcr/0.0.1`), projecting `InteractionSupport` into
  `DeviceSupport { dpad, voice_input, keyboard, spatial_pointer, gamepad }`.
  Device support is a hardware expression only; it never claims Alexa or
  any voice-platform integration.
- **Remote Fallback Engine** — every intent's `remote` fallback must name an
  existing scene element (`QS-E2001`) and be reachable by deterministic
  D-pad focus (`nevada::focus`, contract `nevada.focus/0.0.1`,
  `unreachable_essential` with remote targets marked essential, `QS-E2002`).
  The remote tier is the only tier the Q0 audit guarantees to every app, so
  it must be reachable. Keyboard fallbacks are existence-checked only; no
  keyboard navigation model is claimed.
- **Semantic Intent Compiler** (phase 0, extended) — `resolve_manifest`
  compiles the manifest first (fail-closed `GY-E1011`), then resolves
  device support and per-intent fallback reachability into
  `ResolvedManifest`, serialised deterministically with `"device"` and
  per-entry `"fallback_resolution"` sections. Contract and profile
  identifiers are unchanged (`QUITASOL-INTENT-IR/1-draft`,
  `quitasol.intent.v0`); the lowering tier stays `"unresolved"`.

A Quitasol-minimal scene→focus-element lowering (`scene_focus_elements`)
converts `goya_lang::layout` output — buttons and inputs focusable, plain
cells not — into Nevada `FocusElement`s. This is deliberately **not** the
shared N0-FOCUSGRAPH lowering: Nevada's gap ledger reserves that work
(viewport policy, scroll containers, essential-element derivation) for the
Nevada programme, and this module's variant is superseded when the shared
lowering exists.

## Diagnostics

| Code | Meaning |
|---|---|
| `QS-E2000` | unknown or malformed OEM profile id |
| `QS-E2001` | fallback target names no scene element |
| `QS-E2002` | remote fallback unreachable by D-pad focus |
| `QS-E2003` | profile census fails the Nevada D-pad baseline |

Nevada's ledger reserves `NV-…` codes for the Nevada programme
(N0-DIAGNOSTICS); Quitasol uses its own `QS-…` space.

## BLOCKED (unchanged by Q1)

- **Nevada built-in profiles are synthetic** (`oem.generic.tv`,
  `oem.acme.*`, `oem.generic.gamepad`). No real Fire TV census exists; real
  profile data requires the N0-CENSUS supply chain and registered devices.
- **Runtime capability re-resolution** (N0-RERESOLVE) — compile-time
  support is not a device guarantee; no runtime exists to re-check.
- **Voice platform integration** — the lowering tier stays `"unresolved"`;
  see the Q0 audit for the Alexa contract boundaries per OS.
- **Vega track** — nothing in Q1 touches Vega; the Nevada modules consumed
  here are Android-TV-oriented and their reuse for Vega is unclaimed.
- **Owners/ratification** — unresolved, per phase 0.

## Verification

```
cargo test -p quitasol-semantic --offline
cargo test -p nevada --offline
cargo test -p goya-lang --offline
```

Evidence record: [programme/quitasol-q1.json](programme/quitasol-q1.json).
