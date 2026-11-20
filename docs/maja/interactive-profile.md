# Experimental interactive profile `maja.web.local.v2`

Status: **experimental**, 20 November 2026. Not a production or release
qualification. It builds on the GSSH/2 and GSSH/3 handoff
([`docs/gssh/GSSH-2.md`](../gssh/GSSH-2.md)), which is still a **Candidate**
contract awaiting architecture approval, so M2 is not closed by this profile
([wave 3](wave-3.md)).

```sh
maja check app.gscene --profile maja.web.local.v2
maja build app.gscene --profile maja.web.local.v2 --out new-output-directory
```

The rule is the same as Aragon's ([Android scene](../aragon/android-scene.md)):
every construct GSSH/3 admits is either emitted or refused with `MAJA-E0200`
at that construct's exact source range. Admission enumerates source entries
against a closed allow-list (`maja_semantic::admit_local_interactive`), so
nothing authored is ignored. Absolute lengths are refused by GSSH/3 itself
(`GY-E1009`); use `goya-proportional` to convert older scenes. `--release`
is refused (`MAJA-CLI-E0006`). The v1 static profile is unchanged and still
refuses GSSH/2 declarations (`GY-E1006`).

## Construct coverage

| Construct | Browser output | Reference it must match |
| --- | --- | --- |
| `@cell`, nesting | One absolutely positioned `div` per element, nested as authored, clipped | `goya_lang::layout` draw order |
| `position_p`, `extent_p` | Millipoint rects from `layout.js`'s `resolve`; one point is one CSS pixel; the viewport is the root element's client size | `goya_lang::layout` (Node differential) |
| `text-size-p` | `font-size` in CSS px | `DrawNode::text_size_mp` (Node differential) |
| `stroke` with `stroke-width-p` | A border overlay inside the element, drawn above its content | `DrawNode::stroke_width_mp` (Node differential) |
| `text` on `@cell` and `@button` | Literal text, `white-space: pre`, clipped | Reference frame text |
| `wrap` | The reference wrapper's output, embedded as literal text | `goya_lang::layout` wrap transform |
| `fill` | Opaque background | — |
| `@button` + D1 `@interaction` | Native `<button type="button">`; activation below | Canonical D1 executor (Node differential) |
| `@input` | Native `<input type="text">`, initially empty | — |
| `from` bindings | Snapshot, convert, dispatch | `handoff::convert_field` (Node differential) |
| `@state … session` | Initial values in a runtime that lives for one page load | `initial_state()` |
| `scroll` | The element keeps its fill, text and stroke; its children live in an inner `overflow: auto` scroller with hidden scrollbars (wheel, touch and focused-keyboard scrolling are the browser's). Direct children may extend past it, clamped only to >= 0. | `goya_lang::layout` and `Frame::scroll_limits` (Node differential); `Frame::scrolled_nodes` translation and clipping (browser) |

Refused at the construct's own range:

| Construct | Why |
| --- | --- |
| `text:` on `@input` | Placeholder or initial content is undecided (decision D3). |
| `scroll` on `@button` or `@input` | They cannot have children. |
| `scroll` inside another scroll viewport | The reference counts nested content in the outer limit; browsers count only what the inner viewport shows. |
| `scroll` on an element whose structural name is not unique | The reference keys scroll offsets by name. |
| `text-anchor` | The reference renderer ignores it. |
| `gestures` | Clipboard policy is not implemented for the web. |
| `stroke` without `stroke-width-p`, or the reverse | A border needs both. |
| Elements nested in `@button` or `@input` | The reference does not treat nested buttons as triggers. |
| Text containing U+0000 or carriage return | The browser renders them differently. |
| More than 1024 elements; element text above 16 KiB; initial text above 64 KiB; initial snapshot above 1 MiB | Profile and D1 limits. |
| `@state … durable` | Needs a browser storage adapter and lifecycle contract. |
| D2 interactions | No browser capability adapters. |

There is no display binding in GSSH/2, so **state changes are not visible
on screen**. Showing state needs the contract extension recorded as open in
[decision D3](../gssh/decisions.md#d3--field-bindings).

## Activation (GSSH/2 §Execution)

1. Read the current text of every field the interaction binds, before
   converting any of them.
2. Convert each with `convertField` (mirror of `convert_field`). On the first
   failure, mark that field `aria-invalid`, report it with the browser's
   native validity message, and stop. State is unchanged. A lone UTF-16
   surrogate, which cannot reach Rust, is refused as `INVALID_TEXT`.
3. Dispatch to the generated D1 runtime. A failure leaves state and
   revision unchanged and is shown as `Action failed: <code>` in a host
   `role="status"` region. Dispatch is synchronous, so activations are
   serialized.

Each activation publishes a read-only `goya:transition` event on `document`
with the action, outcome, error code, revision and, after dispatch, a copy of
the state. It is an observation channel for test harnesses, not a second
state authority. The page CSP (`script-src 'self'`) admits no other script.

## Output

All files are generated deterministically: no timestamps, host paths or
randomness. Module scripts must be served over HTTP(S); they do not run from
`file:`. The output directory must not exist and its parent must.

| File | Contents |
| --- | --- |
| `index.html` | Document, CSP `default-src 'none'; script-src 'self'; style-src 'self'; img-src data:; base-uri 'none'; form-action 'none'` |
| `app.css` | Static styles |
| `scene.js` | Generated data: elements, initial state, activations |
| `layout.js` | Static `resolve` (layout mirror) and `convertField` |
| `d1.js` | `MAJA-D1-JS/1-draft` runtime, only when the scene has interactions |
| `app.js` | Static shell |
| `manifest.json` | `MAJA-INTERACTIVE/1-draft`: profile, handoff contract, source SHA-256, element and action source ranges, SHA-256 of every other file |

The manifest is an integrity aid, not an authenticity guarantee.

## D1 runtime

`maja_runtime::emit_d1_runtime` validates every interaction with the
canonical D1 compiler and emits one ES module with no imports, timers,
storage, network or `eval`. It mirrors `D1Program::execute` step by step:
input text validity and limits, input schema and types, combined snapshot
limit, instructions in canonical order with canonical register names (so the
register budget fails at the same instruction), checked i64 addition, the
next touched snapshot, then the revision. Error codes are `UNKNOWN_ACTION`,
`INPUT_SCHEMA`, `INPUT_TYPE`, `STATE_SCHEMA`, `STATE_TYPE`, `INVALID_TEXT`,
`TEXT_LIMIT`, `SNAPSHOT_LIMIT`, `REGISTER_LIMIT`, `ARITHMETIC_OVERFLOW` and
`REVISION_OVERFLOW`. The canonical executor reports input schema and type
failures as one error, so the differential compares them as one class.

## Verification

```sh
cargo test --offline -p maja -p maja-semantic -p maja-runtime -p maja-frampton
cargo clippy --offline -p maja -p maja-semantic -p maja-runtime -p maja-frampton --all-targets --no-deps -- -D warnings
cargo run --offline -p maja -- build fixtures/nevada/counter.gscene --profile maja.web.local.v2 --out OUT
python3 tests/maja/browsers/maja_browser.py chrome counter OUT    # and firefox
cargo run --offline -p maja -- build tests/maja/fixtures/scroll.gscene --profile maja.web.local.v2 --out OUT2
python3 tests/maja/browsers/maja_browser.py chrome scroll OUT2     # and firefox
```

The Rust tests need Node (`MAJA_SKIP_NODE=1` skips those parts, and says so)
and cover:

* `crates/maja-runtime/tests/d1_differential.rs`: generated runtime vs
  canonical executor over 523 cases across int, text and bool actions,
  multi-assignment, i64 and u64 boundaries, the 64 KiB text limit with
  multibyte text, and schema/type failures; failures must leave state and
  revision unchanged. Four seeded mutations of the emitter were each caught.
  A generated 17-key scene straddles the 1 MiB boundaries and reaches
  `REGISTER_LIMIT` and `SNAPSHOT_LIMIT` at the input, combined and
  next-state checks, matching the canonical executor at each boundary.
* `crates/maja/tests/interactive.rs`: byte-identical repeated builds with
  manifest hashes and no host paths; refusal ranges and `--release`
  rejection; `resolve` vs `goya_lang::layout` at eight viewports including
  clamping, defaults, wrap, text size and stroke width, and on the scroll
  fixture, overflowing children and `scrollLimits` vs
  `Frame::scroll_limits` (three seeded mutations caught); `convertField` vs
  `convert_field` over the GSSH/2 conversion corpus.
* `crates/maja-semantic/src/interactive.rs`: admission and per-construct
  refusal ranges.

`maja_browser.py` (headless, Linux, 20 November 2026) drives Chrome
through the DevTools Protocol and Firefox through WebDriver BiDi, with no
separate driver binary:

| Check | Chrome 157.0.8103.78 | Firefox 150.0 |
| --- | --- | --- |
| DOM geometry equal to `resolve` (within 1/30 px) | pass | pass |
| Tab order field → button | pass | pass |
| Enter and Space activation | pass | **not verified**: keys reach the focused native button but headless Firefox synthesizes no click (`document.hasFocus()` stays false, even with `focusmanager.testmode`) |
| Pointer activation | pass | pass |
| Non-canonical field text refused before dispatch | pass | pass |
| `3 + i64::MAX` refused with `ARITHMETIC_OVERFLOW`, state unchanged, reported in `role=status` | pass | pass |
| Accessibility tree: button "Add", title text, one textbox | pass (textbox has no name) | not exposed by BiDi |
| Reload restarts session state | pass | pass |
| No console errors | pass | pass |

Scroll fixture (`maja_browser.py BROWSER scroll`):

| Check | Chrome 157.0.8103.78 | Firefox 150.0 |
| --- | --- | --- |
| DOM geometry equal to `resolve` at offset 0 (9 elements) | pass | pass |
| Scroll extents equal `Frame::scroll_limits` within 1 px (overflowing and fitting viewports) | pass | pass |
| Wheel over the viewport scrolls it | pass | pass |
| At the limit every descendant moves by exactly the offset; the viewport's own text does not | pass | pass |
| Content scrolled past the edge is clipped from hit testing | pass | pass |
| No console errors | pass | pass |

Browsers stop scroll offsets at whole pixels, so the maximum offset can be
up to 1 px short of the reference limit (492 px vs 492.48 px in the run
above). Keyboard and touch scrolling, and offset reconciliation on resize,
are not verified.

## Not qualified

* **The input has no accessible name.** GSSH/2 has no label construct and
  Maja does not invent one. This is an accessibility failure for any real
  application and blocks M4.
* Other browsers, assistive technology, IME composition, touch, high-DPI
  pixel snapping, font scaling and visual typography (default font, line
  height and colours are the browser's).
* Durable state, nested scroll viewports, display bindings, D2 effects.
