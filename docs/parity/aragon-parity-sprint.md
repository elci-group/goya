# Sash and Emperor sprint directive

Bring the Windows and Linux scene compilers to [Aragon's measured scene baseline](../aragon/release-readiness.md) using the same GSSH/3 fixtures and canonical layout, field conversion and D1 execution. This sprint extends the [Parity Lab](programme.md) and preserves the existing desktop counter and static profiles under their current contracts.

Baseline at `fbe186b`. Aragon emits a broad scene subset, has paired offline unsigned APK builds and reports 72 checks on each of two Android emulator versions. Sash has cross-linked Windows executables and needs native Windows execution evidence. Emperor has a recorded debug ELF Wayland first frame and a separate headless counter trace. The desktop compilers still lack the broad GSSH/3 scene and its measured native interactions. See the [Sash roadmap](../sash/roadmap.md) and [Emperor ledger](../emperor/gap-ledger.md).

## Work packages

| Owner | First delivery | Subsequent work |
| --- | --- | --- |
| Shared integration | Use the Aragon qualification sources as common inputs. Emit reference layout and scroll observations keyed by authored element ID. | Compare each target against the same fixtures, with the first divergent element, action or source range recorded. |
| Sash | Deliver an opt-in GSSH/3 native static scene slice using the existing Win32 emitter. Keep current admission behavior for existing callers. | Make the layout responsive, then add scrolling, native edits and D1 dispatch. Capture Windows-side observations and qualify on two named Windows configurations. |
| Emperor | Deliver a feasible increment toward GSSH/3 native scenes or native qualification on Wayland. Keep the counter path available. | Lower the shared scene directly from the canonical handoff. Add scroll, editing and D1 to the full scene path. Qualify on two named Wayland configurations. |
| Aragon | Make its recorded checks easier to compare with desktop evidence. | Keep the common fixture inputs and reference results available for regression comparison. |

The feature floor is nested cells, proportional positions and extents, text and proportional text sizing, inside strokes, wrap, scroll, fill, buttons, field initial text and bindings, and session D1 state. Report unsupported constructs with exact source ranges. Match Aragon's GSSH/3 refusals for D2, durable state and `text-from`. Keep GSSH/3-live, native clipboard extensions, signed releases, installers, pixel equivalence and screen reader certification under their own profiles or release gates.

## Acceptance gates

| Gate | Required evidence for each desktop target |
| --- | --- |
| P1 Scene admission | Compile the shared [Aragon device fixtures](../aragon/android-device-qualification.md) and static nested scenes. Compare exact UTF-8 byte ranges for refusals. Source maps tie controls, bindings and generated action branches to source identity and digest. Reject silently omitted constructs. |
| P2 Host comparison | Run the implementation against canonical Rust layout and scroll limits, field conversion, and D1 state and revision. Target Aragon's recorded scale of 61 scenes × 7 viewports, 331 conversions and 40 D1 programs × 12 dispatches. Include invalid input, overflow and rollback, and show that deliberately changing a result fails comparison. |
| P3 Repeatable builds | Build unsigned release artifacts twice offline from clean copies at different paths. Pin the toolchain, native SDK or sysroot, dependencies and target. Record the artifact hashes and executable structure. A different hash fails the reproducibility check and requires a recorded diagnosis. |
| P4 Native behavior | Run the shared scene on two named native environments per target. Observe multiple viewport sizes and DPI or scale settings, accessible geometry, scrolling to limits, native focus, field initialization, keyboard and pointer activation, and valid and invalid actions. Windows needs a native Windows session. Emperor needs a Wayland session and identified compositor. |
| P5 Lifecycle | Record state and revision before and after recreating the view in one process and then restarting the process. Verify session retention during view recreation and reset on relaunch. Record any field-restoration differences from Android. |

Record the command and exit status, compiler revision, toolchain, source and artifact digests, environment, observations and logs. Follow the [desktop evidence rules](../emperor/evidence.md). A missing environment stays BLOCKED. An observed mismatch is FAIL. Report structural inspection, headless traces, first frames and native interaction separately. The sprint reaches Aragon-level parity when both desktop targets pass P1–P5.

## Reference observations

Run the reference for each fixture and logical viewport. It emits one tab-separated row per authored element in document order, with its parent ID, kind, resolved rectangle, and maximum scroll offsets. Geometry is expressed in millipoints before platform scaling. ID fields escape backslashes, tabs, carriage returns and line feeds.

```sh
cargo run --locked --offline -q -p goya-parity --bin parity-scene -- \
  crates/goya-sdk/tests/android-device/layout.gscene 360x640 640x360
cargo run --locked --offline -q -p goya-parity --bin parity-scene -- \
  crates/goya-sdk/tests/android-device/counter.gscene 360x640 640x360
```

For Sash's initial static slice, 250 millipoints equal one DIP, so a 640 × 480 DIP client uses a 160 × 120 point reference viewport. Aragon uses one point per dp. Compare each platform in its own physical coordinates after checking agreement in source units. Keep native observed bounds separate from reference rectangles, which include offscreen content inside scroll containers.

## First sprint results

| Slice | Observed result | Gate still open |
| --- | --- | --- |
| Shared reference | The layout and counter fixtures produced 126 and 49 geometry rows respectively over seven viewports, including zero and negative dimensions. | The larger differential corpus, input conversion and D1 comparisons remain. |
| Sash static scene | `sash expectations crates/sash/fixtures/proportional-static.gscene --viewport-dip 640x480 --format json` reported GSSH/3. Both label rectangles matched the reference at `160x120` points. The x64 Win10 cross-link passed PE inspection and returned exit 3 with runtime and release blocked. | P1 needs the complete scene subset. P3 needs paired builds. P4 and P5 need native Windows evidence. |
| Aragon evidence | The device qualifier accepts `--report PATH` and records platform identity, source and APK hashes, and expected and observed geometry. A fresh run needs an Android device or emulator. | Collect equivalent desktop observations before comparing results. |

## Integration order

1. Establish the common fixture and observation contract.
2. Integrate nested proportional layout and static text, then add scrolling and editable D1 controls. Complete each slice through the normal target command.
3. Compare both outputs with the reference before paired release builds and native qualification. Track qualification per target and per gate.
