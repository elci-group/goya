# Wave 3 status

Date: 20 November 2026. M2 is **not complete**, but its implementation
blocker has moved. Wave 2 was blocked on three things: no authored initial
state or bindings (gap M0-02), no browser D1 runtime, and no generated
application tested against the canonical oracle in a real browser. ADR-002
had already removed Frampton from local execution.

What changed:

* Goya-lang now provides GSSH/2 and GSSH/3 (`handoff::admit_v2`,
  `admit_v3`): authored `@state` with mandatory initial values and
  lifetimes, `from` input bindings and a reference field conversion. Maja
  does not infer any of these.
* `maja_runtime::emit_d1_runtime` emits a JavaScript state owner for every
  canonical D1 interaction and is differentially tested against the
  canonical executor under Node.
* The experimental profile [`maja.web.local.v2`](interactive-profile.md)
  compiles GSSH/3 scenes with nested elements, buttons, inputs, session
  state and D1 interactions into a static, deterministic, CSP-restricted
  package. Its layout, scroll-limit and field-conversion mirrors are
  differentially tested against `goya_lang::layout`,
  `Frame::scroll_limits` and `convert_field`. Scroll viewports are admitted
  except when nested.
* A generated counter (`fixtures/nevada/counter.gscene`) ran in headless
  Chrome 157 and Firefox 150, with pointer activation (and keyboard
  activation in Chrome), conversion and overflow rejection with state
  unchanged, reload, and no console errors
  (`tests/maja/browsers/maja_browser.py`).

What still blocks M2 and later gates:

* **Contract approval.** GSSH/2 is a Candidate. M2 requires approved
  binding and initial-state semantics, so this evidence cannot close it
  until architecture approves GSSH/2 (`docs/contracts.md`).
* **Decision D3.** `@input` `text:` (placeholder or initial content) and
  display bindings are undecided. Without display bindings, state changes are
  not visible on screen, so no useful application ships on this profile.
* **Accessibility.** Inputs have no accessible name because the language
  has no label construct. M4 requires accessibility evidence; this fails it.
* **Independence.** The differential tests and the Chrome harness were
  written alongside the implementation. G1 requires independent
  qualification; none has been done.
* Only Chrome and Firefox on Linux, headless. Keyboard activation is not
  verified in Firefox (see the [profile document](interactive-profile.md#verification)).
  No assistive technology, IME, touch, Safari or Windows/macOS.

The v1 static profile and the reserved production profile `maja.web.v1`
are unchanged; `maja.web.v1` still refuses every build.

Verification run for this wave: `cargo test --offline -p maja -p
maja-semantic -p maja-runtime -p maja-frampton` (36 tests) and clippy with
`-D warnings` passed; the `tests/maja` and `tests/maja/browsers` Python
suites and the host differential in `tests/maja/differential` passed;
`maja_browser.py` (counter and scroll) passed in both browsers as described in the
[profile document](interactive-profile.md#verification).
