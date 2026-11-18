# Aragon scene compiler (Phase 3)

Status: **experimental**, 17 November 2026. `goya_sdk::compile_android_scene`
compiles a GSSH/2 scene ([`docs/gssh/GSSH-2.md`](../gssh/GSSH-2.md)) to Kotlin
for Android. It replaces the single-construct static-text and root-button
previews as Aragon's main path; those previews remain only because their
APK hashes are recorded qualification fixtures.

```sh
cargo run --offline -p goya-sdk --bin aragon-android-project -- \
    scene.gscene generated-scene --scene
```

The rule is that every construct GSSH/2 admits is either emitted or refused
with the exact source range of that construct, through
`AndroidSceneError { reason, origin, range }`. Nothing is ignored.

## Construct coverage

| Construct | Android output | Reference it must match |
| --- | --- | --- |
| `@cell`, nesting | One `ViewGroup` per element, nested as authored | `goya_lang::layout` draw order |
| `position`, `extent` (points) | Millipoint rects computed by the generated spec | `goya_lang::layout` |
| `position_p`, `extent_p` | As above, fractions clamped to [0, 1] | `goya_lang::layout` |
| `text` on `@cell` | `TextView`, no automatic wrapping, clipped to the cell | Text of the reference frame |
| `wrap` | The reference wrapper's output, embedded as literal text | `goya_lang::layout` (wrap transform) |
| `scroll` | Children may overflow; drag scrolls, clamped to the limits; exposed to accessibility as a `ScrollView` with page scroll actions | `Frame::scroll_limits` |
| `fill` | Opaque background (the 8-digit form's alpha is dropped by the parser) | — |
| `@button` + D1 `@interaction` | `Button`; a click runs the activation protocol below | Canonical D1 executor |
| `@input` | `EditText`, initially empty | — |
| `from` bindings | Snapshot, convert, then dispatch | `handoff::convert_field` |
| `@state … session` | Initial values in a process-lifetime runtime | `initial_state()` |
| `z` components | Not used, as in the reference evaluator | `goya_lang::layout` |

Refused, at the construct's own range:

| Construct | Why |
| --- | --- |
| `stroke`, `stroke-width`, `text-size`, `text-anchor` | The reference renderer defines no semantics (it ignores `text-anchor`; text size and stroke are renderer-specific). Sash refuses them for the same reason. |
| `gestures` | Clipboard policy is not implemented on Android. |
| `text:` on `@input` | Placeholder or initial content is still undecided (decision D3). |
| Elements nested in `@button` or `@input` | The reference does not treat nested buttons as triggers. |
| `scroll` on an element whose structural name is not unique | The reference keys scroll offsets by name, so it is ambiguous. |
| `@state … durable` | Needs the Android storage adapter (gate ARG-G07). |
| D2 interactions | No Android capability adapters. |
| More than 1024 elements; element text above 16 KiB; initial text above 64 KiB; an initial snapshot above 1 MiB | Generated-code and D1 limits. |

There is no display binding in GSSH/2, so **state changes are not visible on
screen**. Showing state needs a contract extension
([decisions D3](../gssh/decisions.md#d3--field-bindings)).

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
scaling is not applied: text uses the platform default size, so the user's
font-size setting does not change it. That is an accessibility gap tracked
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
`aragon-source-map.json` (contract `ARAGON-SOURCE-MAP/1`). The D1 runtime
file is not mapped yet.

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
