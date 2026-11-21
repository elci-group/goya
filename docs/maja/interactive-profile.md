# Experimental interactive profile `maja.web.local.v2`

Status: **experimental**, 20 November 2026 (updated 21 November). Not a
production or release qualification. It builds on the GSSH/2 handoff
([`docs/gssh/GSSH-2.md`](../gssh/GSSH-2.md)), **Approved** on 21 November
2026, through the GSSH/3 profile, which is still a Candidate. M2 is not
closed by this profile ([wave 3](wave-3.md#status-21-november-2026)).

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

The CLI accepts regular source files up to 1 MiB. Larger files fail as input
errors before parsing.

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
| `@input` | Native `<input type="text">`, empty unless `text:` gives its initial content (decision D3); `autocomplete="off"`, so a reload shows the authored text again | GSSH/2 rule 5 |
| `from` bindings | Snapshot, convert, dispatch | `handoff::convert_field` (Node differential) |
| `@state … session` | Initial values in a runtime that lives for one page load | `initial_state()` |
| `@state … durable` | Restored from `localStorage` before the first frame; written before each commit that changes it ([below](#durable-state)) | GSSH/2 rule 4, decision D4 |
| `scroll` | The element keeps its fill, text and stroke; its children live in an inner `overflow: auto` scroller with hidden scrollbars (wheel, touch and focused-keyboard scrolling are the browser's). Direct children may extend past it, clamped only to >= 0. | `goya_lang::layout` and `Frame::scroll_limits` (Node differential); `Frame::scrolled_nodes` translation and clipping (browser) |

Refused at the construct's own range:

| Construct | Why |
| --- | --- |
| `wrap` on `@input`, or a line break in its `text:` | A single-line field strips line breaks, so the authored content could not be shown exactly. |
| `scroll` on `@button` or `@input` | They cannot have children. |
| `scroll` inside another scroll viewport | The reference counts nested content in the outer limit; browsers count only what the inner viewport shows. |
| `scroll` on an element whose structural name is not unique | The reference keys scroll offsets by name. |
| `text-anchor` | The reference renderer ignores it. |
| `gestures` | Clipboard policy is not implemented for the web. |
| `stroke` without `stroke-width-p`, or the reverse | A border needs both. |
| Elements nested in `@button` or `@input` | The reference does not treat nested buttons as triggers. |
| Text containing U+0000 or carriage return | The browser renders them differently. |
| More than 1024 elements; element text above 16 KiB; initial text above 64 KiB; initial snapshot above 1 MiB | Profile and D1 limits. |
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
`REVISION_OVERFLOW`, plus two with no canonical counterpart:
`STORAGE_FAILED` (an optional host `beforeCommit` hook, called after every
canonical check passed, refused the commit), `STORAGE_CONFLICT` (the hook
reported that storage changed elsewhere) and `REENTRANT_DISPATCH` (the hook
tried to dispatch). The canonical executor reports input schema and type
failures as one error, so the differential compares them as one class.

## Display bindings: `maja.web.local.v3`

Added 21 November 2026, **experimental**. The construct is approved
(decision D9); the contract `GSSH/3-live` is a Candidate because GSSH/3 is.

```sh
maja build app.gscene --profile maja.web.local.v3 --out new-output-directory
```

`maja.web.local.v3` is `maja.web.local.v2` admitted through
`handoff::admit_v3_live` instead of `admit_v3`, so it also accepts
`text-from: state.<key>;`. Every v2 scene builds the same way under v3
(the manifest's `profile` and `handoff` fields differ). v2 still refuses
`text-from` (`GY-E1012`).

* The element's authored `text:` is followed by a `<span
  data-goya-text-from>` holding the value: canonical decimal for `int`,
  the text itself for `text`, `true`/`false` for `bool`. It is set with
  `textContent`, so a value is never parsed as markup. On `@button`, the
  value is part of the label and so of the accessible name.
* Values are set in the module task that creates the scene's elements,
  after durable restore, so the scene is never shown without them. After
  a committed transition every display is updated before the click
  handler returns; a failed or refused transition changes nothing.
* A scene whose state is used only by display bindings has no D1 runtime;
  its values are the authored (or restored) ones.

* **Wrapping** (D9, as Arachnid): the element's text container gets
  `white-space: pre-wrap`, so authored text and value keep spaces and line
  breaks and wrap at the element's edge; overflow is clipped. `wrap` on the
  same element applies to the authored text as usual.
* **Announcements** (D9): a visually hidden `<p id="goya-announce"
  aria-live="polite" aria-atomic="true">` receives one message per
  committed transition that changes shown values, built by the
  `layout.js` mirrors of `goya_lang::display_binding` (for example "Count
  decreased by 12, from 0 to -12"). Nothing is announced at load or for a
  refused transition, whose code already goes to `role=status`. An
  identical repeated message is cleared and re-set on the next frame so it
  is spoken again. v2 scenes have no region.

Refused with `MAJA-E0200` at the property: `text-from` on `@input` (the
field's text is what the user types).

Known gaps: whether a screen reader speaks the region is not verified (no
assistive technology on the test host); a `text` value containing U+0000
or carriage return, which Maja refuses in authored text, is shown as the
browser renders it.

## Durable state

Added 21 November 2026. GSSH/2 rule 4 fixes the semantics (restore before
the first frame, all-or-nothing, a bad record is reported and not
overwritten until the user acts, namespace = ontology name and state key)
but no storage format. The format and the concurrent-writer rule below were
approved by the project owner on 21 November 2026 as
[decision D8](../gssh/decisions.md#d8--durable-record-format-and-concurrent-writers)
for every target. Aragon still refuses `durable` (gate ARG-G07), so Maja is
the only implementation.

Scenes with at least one durable key get `storage.js`:

* **Storage.** `localStorage`, because it is synchronous: restore finishes
  before `app.js` returns (so before the first frame), and the write can
  sit inside the D1 runtime's `beforeCommit` hook.
* **Record.** One item per ontology, key `goya.state.v1/<ontology name>`,
  value `{"format":"goya.state.v1","values":{"<key>":["<type>",<value>]}}`;
  ints are canonical decimal strings, text is the string, bools are JSON
  booleans. Because one record holds every durable key, a transition that
  writes several of them is stored by one `setItem` and cannot be torn.
* **Restore.** A missing record, or a missing entry, gives that key's
  authored value. Anything else wrong fails the whole restore and every
  key starts from its authored value: storage that throws on read
  (`STORAGE_UNAVAILABLE`); bad JSON, a wrong envelope, an unknown type tag,
  a non-canonical or out-of-range int, a non-boolean bool, text with a lone
  surrogate or above 64 KiB (`RECORD_UNDECODABLE`); an entry whose type
  differs from the declaration (`RECORD_TYPE_MISMATCH`); a restored
  snapshot over D1's 1 MiB (`RECORD_LIMIT`). The code goes to
  `<html data-goya-restore>` (`ok` on success) and a notice goes to the
  `role=status` region. The record is not touched on load.
  A raw record above 8 Mi UTF-16 code units is rejected as `RECORD_LIMIT`
  before JSON parsing, bounding work on corrupted or hostile local storage.
* **Write.** After the runtime has passed every canonical check, the hook
  writes the whole durable snapshot as it will be after the transition.
  A transition that writes no durable key never touches storage. If
  `setItem` throws (quota, disabled storage) the commit is refused with
  `STORAGE_FAILED` and state, revision and record are unchanged. So the
  stored value never trails the visible one; if the page dies between the
  write and the commit, the next load shows the written value.
* **After a failed restore**, the first durable commit replaces the record
  ("until the user acts"). Entries of a decodable record that the scene
  does not declare durable (for example a renamed key) are written back
  unchanged; GSSH/2 defines no migrations (decision D4).
* **Other windows.** Each page remembers the record text it last read or
  wrote, and re-reads it just before writing. If it differs (another tab
  wrote or cleared it), the commit is refused with `STORAGE_CONFLICT`,
  nothing is written, and the status asks the user to reload; reloading
  restores the other tab's state. Transitions that touch only session keys
  still run.
* **Revision** is not stored: it restarts at 0 on every page load.

Known gaps: there is no lock between the re-read and the write, so two tabs
committing in the same instant can still race (browsers do not implement
the storage mutex). A tab does not learn of another tab's write until it
tries to commit. Storage is per origin, so two apps
with the same ontology name on one origin share a record, as GSSH/2's
namespace implies. There is no sensitive or secure storage, and browsers
may evict `localStorage` (no `navigator.storage.persist()` request).

## Verification

```sh
cargo test --offline -p maja -p maja-semantic -p maja-runtime -p maja-frampton
cargo clippy --offline -p maja -p maja-semantic -p maja-runtime -p maja-frampton --all-targets --no-deps -- -D warnings
cargo run --offline -p maja -- build fixtures/nevada/counter.gscene --profile maja.web.local.v2 --out OUT
python3 tests/maja/browsers/maja_browser.py chrome counter OUT    # and firefox
cargo run --offline -p maja -- build tests/maja/fixtures/scroll.gscene --profile maja.web.local.v2 --out OUT2
python3 tests/maja/browsers/maja_browser.py chrome scroll OUT2     # and firefox
cargo run --offline -p maja -- build tests/maja/fixtures/durable-counter.gscene --profile maja.web.local.v2 --out OUT3
python3 tests/maja/browsers/maja_browser.py chrome durable OUT3    # and firefox
cargo run --offline -p maja -- build tests/maja/fixtures/initial-content.gscene --profile maja.web.local.v2 --out OUT4
python3 tests/maja/browsers/maja_browser.py chrome initial OUT4    # and firefox
cargo run --offline -p maja -- build tests/maja/fixtures/display-counter.gscene --profile maja.web.local.v3 --out OUT5
python3 tests/maja/browsers/maja_browser.py chrome display OUT5    # and firefox
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
  `convert_field` over the GSSH/2 conversion corpus; `shownText`,
  `displayContext`, `describeChange` and `announcement` vs
  `goya_lang::display_binding` over 10 contexts and 12 value pairs (three
  seeded mutations caught; a fourth, `d >= 0n`, is equivalent); `storage.js` with the
  generated `d1.js` against a fake `localStorage`: missing record, a
  session-only transition that never writes, two durable keys in one
  `setItem`, reload, a throwing `setItem` refused as `STORAGE_FAILED`, 15
  corrupt or retyped records each failing restore with the record left
  as it was, all-or-nothing on a partly bad record, unreadable storage,
  replacement after a failed restore that keeps undeclared entries
  (including `__proto__`, without prototype pollution), two windows (the
  second writer gets `STORAGE_CONFLICT`, the first writer's record
  survives, a reload adopts it; a cleared record also conflicts), and
  `RECORD_LIMIT` at 17 × 64 KiB text keys (15 restore).
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

Display fixture (`maja_browser.py BROWSER display`, a `maja.web.local.v3`
build of `tests/maja/fixtures/display-counter.gscene`, 21 November 2026):

| Check | Chrome 157.0.8103.78 | Firefox 150.0 |
| --- | --- | --- |
| Hidden polite live region, silent at load | pass | pass |
| On load: `Count: 0`, `none`, button `Raised: false` | pass | pass |
| Committed int shown as canonical decimal (`Count: -12`), announced "Count decreased by 12, from 0 to -12" | pass | pass |
| Text value `<b>&amp; γειά 🌍  x` shown verbatim, not as markup; announced with the key as context | pass | pass |
| A 40-word value wraps at the element's edge (`pre-wrap`, no line box past it) | pass (7 lines) | pass (8 lines) |
| Bool shown as `true` after a button's static label, announced "Raised changed from false to true" | pass | pass |
| `ARITHMETIC_OVERFLOW` changes nothing on screen and announces nothing | pass | pass |
| Reload shows the restored durable count; session values authored | pass | pass |
| Button accessible name includes the value | pass | not exposed by BiDi |
| No console errors | pass | pass |

Initial-content fixture (`maja_browser.py BROWSER initial`, on
`tests/maja/fixtures/initial-content.gscene`, 21 November 2026):

| Check | Chrome 157.0.8103.78 | Firefox 150.0 |
| --- | --- | --- |
| The field holds `7` as its value, with no placeholder | pass | pass |
| Activating without typing converts it: 0 + 7 = 7 | pass | pass |
| After editing to `3`, reload shows `7` again | pass | pass |
| No console errors | pass | pass |

Durable fixture (`maja_browser.py BROWSER durable`, 21 November 2026):

| Check | Chrome 157.0.8103.78 | Firefox 150.0 |
| --- | --- | --- |
| No record: authored value; commit stores `{"format":"goya.state.v1","values":{"count":["int","5"]}}` | pass | pass |
| Reload restores 5 before any input (5 + 1 = 6) | pass | pass |
| `ARITHMETIC_OVERFLOW` leaves the record unchanged | pass | pass |
| Throwing `setItem` refuses the commit with `STORAGE_FAILED`, reported; reload still gives 6 | pass (failure simulated) | pass (failure simulated) |
| Undecodable record: `RECORD_UNDECODABLE` reported, record kept on load, first commit starts from 0 and replaces it | pass | pass |
| Retyped record: `RECORD_TYPE_MISMATCH` | pass | pass |
| Second tab commits 40 through the app. The stale tab gets `STORAGE_CONFLICT`, preserves its state and revision, and reload adopts 40 (40 + 1 = 41). | pass | pass |
| No console errors | pass | pass |

Open verification cases include a real quota error, simultaneous writes
between the storage re-read and write, and a crash between write and commit.

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
* Nested scroll viewports, D2 effects. Display bindings exist only in the
  experimental `maja.web.local.v3` on the Candidate GSSH/3-live (construct approved as D9). Durable state is
  implemented under decision D8, but GSSH/2 itself is still a Candidate.
