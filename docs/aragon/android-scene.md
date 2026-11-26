# Aragon scene compiler (Phase 3)

Status: **experimental**, 17 November 2026. `goya_sdk::compile_android_scene`
compiles a GSSH/4 scene ([`docs/gssh/GSSH-2.md`](../gssh/GSSH-2.md#gssh4--platform-ambiguity-and-explicit-exceptions)) to Kotlin
for Android. It replaces the single-construct static-text and root-button
previews as Aragon's main path; those previews remain only because their
APK hashes are recorded qualification fixtures.

```sh
cargo run --offline -p goya-sdk --bin aragon-android-project -- \
    scene.gscene generated-scene --scene
```

Since 18 November 2026 Aragon accepts GSSH/4, which inherits GSSH/3's
proportional-length requirements (decision D7): every
length is proportional, and an absolute `position`, `extent`, `text-size` or
`stroke-width` is refused by admission with `GY-E1009` at its range. Use
`goya-proportional` ([migration ledger](../gssh/proportional-migration.md))
to convert older scenes.

A GSSH/4 scene may declare persistence targets (`@persistence`, decision
D11); it is optional, and without it durable state would use Android's default
local store (`android.preferences.v1`). Aragon still refuses durable state
until its Android lifecycle and storage adapter is wired to the platform
runtime.

Every `@input` must declare `@iden { label: "..."; }` (GSSH/4, decision D14).
Aragon emits it through Android's native accessibility (`GoyaCell.identify`):
an editable field's label becomes its node hint text (API 26+; folded into the
node text on API 23–25), other elements get a content description, and the
optional `hint` becomes the node's tooltip text (API 28+; appended to the name
before). This is generated and source-mapped but not yet run on a device.

The rule is that every construct GSSH/4 admits is either emitted or refused
with the exact source range of that construct, through
`AndroidSceneError { reason, origin, range }`. Nothing is ignored.
An authored `@desktop` is refused (`desktop window`, decision D16) until
Aragon places it. Omitting `@desktop` stays the contract's fullscreen
default, which this compiler does not apply to the Android window.

## Construct coverage

| Construct | Android output | Reference it must match |
| --- | --- | --- |
| `@cell`, nesting | One `ViewGroup` per element, nested as authored | `goya_lang::layout` draw order |
| `position_p`, `extent_p` | Millipoint rects computed by the generated spec; fractions clamped to [0, 1] of the parent, or only to >= 0 inside a scroll viewport | `goya_lang::layout` |
| `text-size-p` | Em size in pixels on the element's `TextView`, `Button` or `EditText` | `DrawNode::text_size_mp` |
| `stroke` with `stroke-width-p` | A border drawn inside the cell, above its content | `DrawNode::stroke_width_mp` |
| `text` on `@cell` | `TextView`, no automatic wrapping, clipped to the cell | Text of the reference frame |
| `wrap` | The reference wrapper's output, embedded as literal text | `goya_lang::layout` (wrap transform) |
| `scroll` | Children may overflow; drag scrolls, clamped to the limits; exposed to accessibility as a `ScrollView` with page scroll actions | `Frame::scroll_limits` |
| `fill` | Opaque background (the 8-digit form's alpha is dropped by the parser) | — |
| `@button` + D1 `@interaction` | `Button`; a click runs the activation protocol below | Canonical D1 executor |
| `@input` | `EditText`, initialized from `text:` or empty when omitted | GSSH/2 D3 field-content rule |
| `from` bindings | Snapshot, convert, then dispatch | `handoff::convert_field` |
| `@state … session` | Initial values in a process-lifetime runtime | `initial_state()` |
| `z` components | Not used, as in the reference evaluator | `goya_lang::layout` |

Refused, at the construct's own range:

| Construct | Why |
| --- | --- |
| `text-anchor` | The reference renderer ignores it. |
| `stroke` without `stroke-width-p`, or the reverse | A border needs both a colour and a width. |
| `gestures` | Clipboard policy is not implemented on Android. |
| Elements nested in `@button` or `@input` | The reference does not treat nested buttons as triggers. |
| `scroll` on an element whose structural name is not unique | The reference keys scroll offsets by name, so it is ambiguous. |
| `@state … durable` | Needs the Android storage adapter (gate ARG-G07), storing the record fixed by [decision D8](../gssh/decisions.md#d8--durable-record-format-and-concurrent-writers). |
| D2 interactions | No Android capability adapters. |
| `text-from` | Refused by GSSH/3 (`GY-E1012`). Adopting GSSH/3-live requires D9's wrapping and announcement rules (`goya_lang::display_binding`). |
| `@platform android` or `@platform android_tv` exceptions | Refused at the exception's exact range until the matching compiler can resolve the named feature and use rule. Exceptions for other platforms do not affect Android or Android TV. |
| More than 1024 elements; element text above 16 KiB; initial text above 64 KiB; an initial snapshot above 1 MiB | Generated-code and D1 limits. |

Aragon admits GSSH/4, not GSSH/3-live, so **state changes are not visible on
screen**. Adopting `text-from` requires Aragon to implement the display-binding
profile and its wrapping and announcement rules ([decision D9](../gssh/decisions.md#d9--display-bindings-text-from-gssh3-live)).

## GSSH/4-live (25 November 2026)

A scene that needs the live profile is admitted by `admit_v4_live`
(`src/android_live.rs`). Scenes without live constructs compile byte for
byte as before.

* **Live text.** `text-from` shows the authored text and the value. Changed
  displays are announced once to TalkBack (`announceForAccessibility`), in
  the D9 wording.
* **Presence.** `present-when` makes an element `GONE`. Hidden variants
  take no touch, key focus or accessibility focus.
* **Motion.** `fill-to` uses a reversing ARGB animator. `drift-p` is an
  elliptical orbit, and `appear-s`/`appear-from-p` fade and slide. All of
  it is off when the system disables animations.
* **Digits.** `remote-digit` maps to the digit keys.
* **Graphics.** `@graphic` and `@draw` use inventories compiled and
  verified by `goya_scene_graphics`, decoded by the generated pure-Kotlin
  `GoyaGvi.kt` and painted with `android.graphics`. The JVM conformance
  test reproduces the reference decoder on every shared vector.
* **Durable state.** `durable` state is stored in `android.preferences.v1`
  (`SharedPreferences`, committed synchronously):
  * restore happens before the first frame, and a record that fails to
    restore is never overwritten (D4);
  * a durable write is staged on a scratch runtime and saved before it is
    shown (D12).
* **Generated runtime.** `GoyaD1Runtime.dispatch` now calls one private
  function per action, so no JVM method nears the 64 KB limit. The pinned
  APK hashes in `scripts/check-android-apk-build.sh` must be re-recorded.
* **Refused until Android platform services exist:** `host` state, `@link`
  and `image-from`.

Goya Pairs ran on an Android 15 emulator (12 of 12 checks, played through
the accessibility tree) and on an Amazon Fire HD 10 (KFTRWI, Fire OS
7.3.3.1, API 28).

## Layout and pixel rule

`<Spec>.kt` is pure Kotlin with no Android imports. `resolve(widthPt,
heightPt)` repeats `goya_lang::layout` step by step in `Long` millipoints:
absolute values are precomputed by the compiler with the reference's
conversion, proportional values are applied at run time with the same
`Double` multiply and truncation, and clamping uses saturating arithmetic.

On Android, one point is one dp. The viewport is the area inside the system
bars and display cutout. `GoyaSceneView` applies those insets as padding,
and scene projects use a theme without an action bar (Phase 4; Android 15
enforces edge-to-edge drawing). The viewport in points is that area in pixels
divided by `displayMetrics.density`. Each rect edge is converted on
its own as `round(mp × density / 1000)`, half up, and a cell's size is the
difference of its edges. Adjacent cells therefore never gap or overlap. This
is Aragon's resolution of the dp rounding rule left open in decision D1. Font
scaling is not applied: text without `text-size-p` uses the platform default
size, and `text-size-p` text follows its element, so the user's font-size
setting changes neither. That is an accessibility gap tracked
under ARG-G08.

## Activation

A click on a button whose interaction is D1 follows GSSH/2 §Execution:

1. Read the current text of every field the interaction binds, before
   converting any of them.
2. Convert each field with `convertInt` or `textWithinLimit`, which mirror
   `convert_field`. On the first failure, show an error on that field
   (`EditText.error`) and stop; state is unchanged.
3. Dispatch to `GoyaD1Runtime`, the canonical D1 runtime emitter's output.
   A failure leaves state and revision unchanged and is reported in a toast
   with the D1 error code. Clicks run on the UI thread, so activations are
   serialized.

`GoyaSession.runtime` lives as long as the process, so `session` state
survives activity recreation and is lost at process death. This has not been
run on a device (ARG-G07).

## Source map

`AndroidScene::source_map` maps generated byte ranges to GSSH/2 origins
(elements, properties, interactions, bindings and state keys) and their
source ranges. `lookup(file, offset)` returns the innermost entry, for
example to map a Kotlin compiler error back to the scene.
`verify(source, files)` re-admits the source and checks the digest, the
generated ranges and every origin. The generator writes it as
`aragon-source-map.json` (contract `ARAGON-SOURCE-MAP/1`). In the generated
D1 runtime file, each generated action branch is mapped to its interaction
declaration. Individual statements within a branch do not yet have separate
source mappings.

## Evidence (17 November 2026)

* `crates/goya-sdk/tests/android_scene.rs`: construct coverage, exact-range
  refusals for every refused construct, source-map verification, and
  deterministic output.
* `scripts/check-android-scene-kotlin.sh` compiles the generated Kotlin with
  kotlinc 2.0.21 and runs it on the JVM. It compares the results with the
  Rust references for:
  * layout and scroll limits: 61 scenes (60 seeded random trees with
    absolute, proportional, negative, huge and overflowing geometry), each
    at 7 viewport sizes, including 0, negative and saturating sizes;
  * field conversion: 331 inputs;
  * the D1 runtime: 40 random D1 programs × 12 sequential dispatches, against
    the canonical executor, including overflow failures.

  To confirm the test can fail, three bugs were planted one at a time
  (rounding instead of truncation, accepting leading zeros, and dropping the
  overflow check), and each was reported at the first diverging line.
* `scripts/check-android-apk-build.sh` builds a compiled scene with nested
  cells, proportional geometry, wrap, scroll, two bound fields and a D1
  action into an unsigned release APK, twice, offline, with strict
  verification. See [`android-toolchain-lock.md`](android-toolchain-lock.md).

Phase 4 ran these apps on Android 15 and Android 6.0 emulators; see
[`android-device-qualification.md`](android-device-qualification.md). Not
established: pixel output, TalkBack, IME composition, and physical devices.
