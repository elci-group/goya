# GY-EMP-NIR/1

Contract identifier: `GY-EMP-NIR/1`. Programme GY-EMP-001, revision 1.0.0. Date 16 November 2026.

This is the target intermediate representation for Emperor. It is not a second scene language. Linux-only syntax is not added to `.gscene`. The canonical bytes are the input to `emperor_runtime::oracle` and the body of `nir.json`.

Repository status: the types below are specified for this cycle in `crates/emperor-runtime`, and the lowering is specified in `crates/emperor`. That specification is not a certification that EMP-M1 has passed.

## Profile

`emperor.linux.interactive/0.0.1` admits only a scene Sash admits as `SceneTarget::Interactive` with presentation profile `sash.win32.interactive/0.0.2`. The shape is one heading, one integer edit, one button, and one result cell, with one checked D1 addition. The fixture is `crates/sash/fixtures/interactive-counter.gscene`.

A static scene, including `crates/sash/fixtures/static-window.gscene`, is rejected. Anything Sash rejects is rejected. The Sash diagnostic code is preserved. Exit code is 1.

Lowering SHALL call `sash::admit::open` and `goya_lang::handoff::admit`. If one succeeds and the other fails, admission fails. Emperor does not parse a private dialect.

The NIR profile string and the Sash presentation profile string are different names for the same admitted counter. They are not interchangeable identifiers.

## Types

All three structs use `serde::Serialize`, `serde::Deserialize`, and `deny_unknown_fields`. An unknown field fails deserialization. A missing field fails deserialization. Optional extensions are not smuggled in as extra keys. This profile has no extension object.

Field order is the order written here. `serde_json::to_string` of the struct is the canonical form. Pretty-printing is not canonical. Key reordering is not canonical. The hash of the IR, when one is required, is the SHA-256 of those canonical bytes.

```text
NativeIr
  schema: string                  exactly "GY-EMP-NIR/1"
  profile: string                 exactly "emperor.linux.interactive/0.0.1"
  source_digest_sha256: string    lowercase hex SHA-256 of the admitted scene bytes
  scene: string                   original scene text
  action_id: string               AdmittedInteractive.action_id
  state_id: string                AdmittedInteractive.state_key
  input_id: string                AdmittedInteractive.input_key
  initial_state: i64
  initial_input: i64
  width_dip: i32
  height_dip: i32
  elements: array of Element      admission order
  capabilities: Capabilities

Element
  id: string                      semantic id
  role: string                    client, edit, button, result, or static
  text: string                    authored text at admission
  x_dip: i32
  y_dip: i32
  width_dip: i32
  height_dip: i32
  span_start: usize               half-open UTF-8 byte offset
  span_end: usize                 half-open UTF-8 byte offset

Capabilities
  copy: bool
  paste: bool
  cut: string                     "allow", "default", or "deny"
  filesystem: string              "none" for this profile
  network: bool                   false for this profile
  subprocess: bool                false for this profile
  web_content: bool               false for this profile
```

`source_digest_sha256` SHALL equal the lowercase hex of `AdmittedInteractive.source_digest`. That digest is the SHA-256 of the source bytes Sash admitted. A lowering that hashes a different buffer is a defect.

`span_start` and `span_end` are the control's Sash `id_range`. They are byte offsets into `scene`, not UTF-16 code units and not line and column. The end is exclusive. A span that does not fall inside `scene` is a lowering defect.

## Element roles

Assign one role per admitted control, in control order:

1. Sash `ControlRole::Edit` becomes `edit`.
2. Sash `ControlRole::Button` becomes `button`.
3. The root heading becomes `client`. It is the first control, and its semantic id is `window.root` on the fixture. A static control whose semantic id ends with `.root` is `client`.
4. The other static control becomes `result` when its semantic id is `state.` concatenated with `state_id`. On the fixture that id is `state.count` and `state_id` is `count`.
5. Any remaining static control becomes `static`.

This profile admits exactly two static controls, and both are consumed by rules 3 and 4. Emitting `static` for an admitted counter is a lowering defect. The role exists so an unknown static cannot be silently relabelled as `result` or `client`. A role string outside the five names fails at the runtime.

## Fixture lowering

For `crates/sash/fixtures/interactive-counter.gscene` the admitted values are:

| Field | Value |
| --- | --- |
| action_id | `action.add` |
| state_id | `count` |
| input_id | `amount` |
| initial_state | 0 |
| initial_input | 1 |
| width_dip, height_dip | 320, 160 |
| copy, paste, cut | true, false, `default` |
| filesystem, network, subprocess, web_content | `none`, false, false, false |

Elements, in order, rectangles as x, y, width, height in DIP:

| id | role | text | rectangle |
| --- | --- | --- | --- |
| window.root | client | Count | 0, 0, 320, 160 |
| window.amount | edit | 1 | 8, 64, 120, 32 |
| window.add | button | Add | 136, 64, 80, 32 |
| state.count | result | 0 | 8, 112, 160, 32 |

`cut` is `default` because the scene does not author cut. `default` is Sash `CUT_DEFAULT` (0). `allow` is `CUT_ALLOW` (1). `deny` is `CUT_DENY` (2). `default` does not mean deny. The system cut command stays available on the Sash image. This profile must not rewrite `default` to `deny`.

Any NIR in this profile with `filesystem` other than `none`, or with `network`, `subprocess`, or `web_content` true, is rejected by the oracle. The counter scene does not declare those effects.

## Runtime check

`oracle` parses the canonical JSON into `NativeIr`. The schema and profile strings must match exactly. It then opens `goya_parity::keys::KeyController` on `scene`. The session's initial count must equal `initial_state`. The edit text must equal the canonical decimal of `initial_input`. Copy and paste flags must match `capabilities`. A mismatch is an error in the lowering, not a new arithmetic rule.

The oracle does not open a window. Its `presented` count is 0 and its Wayland status is `not-requested`. That result is not a launch.

## Generated Rust

`build` writes `embed.rs` beside `nir.json`. The file defines `ELEMENTS`, `SCENE`, `NIR_JSON`, `SCENE_SRC`, and `embedded()`. `NIR_JSON` is the canonical JSON escaped as a Rust string. `SCENE_SRC` is the original scene escaped the same way. Escapes cover quotes, backslashes, newlines, and other control characters. The scene is not placed in a format string that a shell will parse. `CompiledScene.elements` points at `ELEMENTS`. `assert_scene_matches` checks every `CompiledScene` field and each element against the parsed NIR before a run that uses the embed.
