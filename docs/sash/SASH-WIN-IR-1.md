# SASH-WIN-IR/1 — Sash Windows emission IR

Contract identifier: `SASH-WIN-IR/1`. Status: **Candidate**, 18 November 2026.
Architecture approval is required before a target may cite it as a frozen
contract (`docs/contracts.md`). Implementation: `crates/sash-ir` (types),
`crates/sash/src/ir_lower.rs` (lowering), `crates/sash/src/ir_codegen.rs`
(code generation).

SASH-WIN-IR/1 is how the Sash compiler separates admission from emission. It
is an in-process Rust value, not a file format and not a second source of
window meaning. The source of meaning stays GY-LANG/1 through strict GSSH
admission (`GSSH/1`); the IR carries exactly what the emitter consumes, plus
the authored origins of every fact it carries, so the evidence source map and
the disposition story survive the split.

Admission remains the gate for reachable meaning. A scene the static-window or
interactive profile rejects never reaches the IR. The IR adds no meaning, drops
no meaning, and defaults nothing: lowering is a field-for-field rename of
admitted meaning.

## Pipeline

```text
admit (GSSH/1) ──▶ AdmittedWindow | AdmittedInteractive
                        │
                        ▼
              ir_lower::lower_static_window / lower_interactive_window
                        │
                        ▼
                  sash_ir::IrDocument          ◀── SASH-WIN-IR/1
                        │
                        ▼
              ir_codegen::codegen_from_ir
                        │
                        ▼
        crate::emit::Generated  (main.rs, Cargo.toml, build.rs,
                    app.manifest, app.rc, evidence source map)
```

`emit::generate` and `interactive::generate` are thin delegations onto this
pipeline. There is no second emission path.

## Document shape

`IrDocument` carries, in field order: `version`, `profile`, `window`,
`controls`, `transition`, `gesture_policy`, `identity`, `resources`,
`provenance`.

| Field | Carries | Notes |
| --- | --- | --- |
| `version` | `"1"` | Schema version. Bump only for a shape change, never for a profile change. |
| `profile` | presentation profile string | `sash.win32.static-window/0.0.3` or `sash.win32.interactive/0.0.1|0.0.2`. Codegen dispatches on this value. |
| `window` | scene name, root semantic id, client extent in DIP, layout extent in millipoints | The title and client size the emitted constants carry. |
| `controls` | one `Control` per placed control, in document order | Role, native id, structural name, semantic id, text, DIP rectangle, and the three authored origins (text, id, extent). |
| `transition` | the interactive D1 transition: action id/name, state and input keys, initial values, control indices, button id, and the `IntExpr` tree | `None` for the static profile. The expression tree is checked-addition over literals, state, and input only. |
| `gesture_policy` | copy/paste allowed flags and the cut mode | `None` for the static profile. Cut mode discriminants match the emitted `CUT_*` constants (default 0, allow 1, deny 2). |
| `identity` | title and root id as UTF-8 bytes with lengths | What the emitted `TITLE_UTF8` / `ROOT_ID_UTF8` arrays carry. |
| `resources` | the four non-main build files as text | `cargo.toml` and `build.rs` are fixed compiler text for every profile; codegen takes them from the `emit` constants and asserts the IR copy matches. `manifest.xml` and the resource script vary by profile version. |
| `provenance` | source digest, the window-level authored origins, and one `ControlOrigin` per control | The evidence source map is built from this and only this. |

## Emission guarantees

1. **Byte-exact against the adopted emitters.** At adoption, the IR path was
   compared field-for-field against the previous direct emitters for both
   fixtures and both profiles and produced identical bytes, including the
   evidence source map. The `generated_digest` of each fixture is pinned in
   `crates/sash/src/ir_codegen.rs` tests; any emission drift fails the suite.
   Pinned digests: `b0bf2d20…` (static window), `b1ca4cbb…` (nested static),
   `83a96632…` (interactive counter).
   **End-to-end confirmation, 2026-11-18, this host:** fresh cross-links of
   both fixtures through the IR-refactored compiler reproduced the recorded
   pre-refactor `*_x64_repro_pair` and `*_architecture_stamp` hashes in
   `docs/sash/programme/sash-ent-002.json` byte-for-byte, for every admitted
   triple: `b60cd84a…` / `ab0078ca…` / `743d2443…` (static exe, x64 / i686 /
   ARM64) and `97dc90d8…` / `68e927a1…` / `92960685…` (interactive exe,
   x64 / i686 / ARM64), with matching PDBs. `sash verify` exited 3 on all six
   images with the correct machine and presentation profile. The refactor is
   therefore invisible from the linked artifact through the evidence map on
   every admitted target. The reproduction is a permanent gate:
   `cargo test -p sash --offline --test link_reproduction -- --ignored`
   links every recorded pair — the six above plus the four win8-floor pairs
   first linked on 2026-11-18 — and asserts the hashes, skipping visibly on a
   host without the pinned cross toolchain. The floor refactor that added
   `--api-floor win8` changed zero win10 emitted bytes: the six win10 pins
   still pass unchanged.
2. **Determinism.** Two lowerings of one admitted scene are equal `IrDocument`
   values; two code generations of one document produce equal output and equal
   digests. Lowering and codegen are pure functions of their input.
3. **Origins decode.** Every `text` origin in the evidence source map covers
   exactly the byte span whose decimal bytes are the control's authored text.
   The `interaction` origin covers the emitted transition and names the
   authored interaction span, not a control extent.
4. **Layout parity.** `sash_ir::SourceRange` is a hand-maintained structural
   duplicate of `goya_lang::SourceRange` so `sash-ir` stays independent of
   goya-lang. Field-for-field layout parity is pinned by
   `source_range_layout_parity` in `crates/sash/src/ir_lower.rs`, the only
   crate that depends on both.

## What the IR does not claim

- It is not an executable format, a serialization commitment for build
  artifacts, or an interchange between compilers. The serde derives exist for
  tooling (linters, diffs) and tests; nothing in a build report or evidence
  directory carries an IR document yet.
- It does not ratify a new profile. The adoption refactor changed zero emitted
  bytes, so no profile bumped and no build-report claim changed.
- It does not change admission. Rejection reasons, diagnostics, dispositions,
  and source digests are all admission output and are untouched.
- It carries no Windows runtime claim. Runtime qualification stays BLOCKED
  until a Windows worker launches the images.
- The API floor (`--api-floor win10|win8`) is not an IR field: it selects the
  Win32 adapter at codegen and the emitted resources at lowering. Window
  meaning is floor-independent, so no IR shape change was needed and the
  version stays 1.

## Versioning rules

- A field addition or removal bumps the major version (`SASH-WIN-IR/2`) and
  requires a migration note here.
- A profile change (new admitted meaning) never bumps the IR version.
- Codegen dispatch keys on the profile string prefix; an unlowered profile
  reaching codegen is a compiler defect and panics by design rather than
  emitting a guess.
