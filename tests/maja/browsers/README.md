# Browser qualification prerequisites

`maja.web.v1` currently
rejects builds; its policy does not admit a browser runtime or an initial
state and binding contract. Do not replace that rejection to make this suite
run. Chrome and Firefox executables were found locally; finding an executable
does not establish automation, supported versions, accessibility support, or
successful browser execution. Node, Playwright, and chromedriver were not
found on the initial PATH check. As of 20 November 2026, Node 18 is on PATH
and is used by the Maja Rust differential tests; Playwright and chromedriver
are still absent.

The five prerequisites below were written for `maja.web.v1`. The
experimental `maja.web.local.v2` profile addresses 2 and 3 through the GSSH/2
Candidate contract and the `goya:transition` observation event; 1, 4 and 5
remain open (see `docs/maja/wave-3.md`).

Before implementing the browser counter test, integration must supply:

1. An explicitly admitted profile and verified Frampton adapter, pinned
   runtime and reproducible static artifact, with documented browser matrix.
2. Canonical initial values and mappings from authored input and state to
   controls and visible text. These cannot be inferred from a D1 declaration.
3. A machine-observable transition result with exact typed state, revision,
   and rejection outcome, without creating a second state authority.
4. An independently selected browser automation driver and accessibility
   inspection method, with pinned versions and an offline execution plan.
5. An independently defined lifecycle policy, including reload and storage,
   and approved selectors based on stable authored identities.

The existing `crates/neptune/conformance/d1-increment.gscene` defines a D1
action with integer `count` and `previous`, plus input `amount`. Canonical D1
reads the original snapshot for both writes; checked integer overflow rejects
the transition. `goya-executable/src/d1_vm.rs` computes the next snapshot
without mutating the input, then increments the revision with overflow checking.
An independent browser test must compare the canonical outcome, not just a
number displayed in the DOM. It must exercise a successful increment, overflow
and invalid-input rejection with unchanged state and revision, keyboard and
pointer activation, focus, accessible naming, reload behavior, console errors,
and repeated artifact hashes. Assistive-technology checks require a separate
documented environment and actual interaction, not markup inspection alone.

Fixture generation, source maps, user-visible error semantics, and browser
instrumentation remain open design work. No result from this document is
evidence of conformance or accessibility.

`chrome_smoke.py OUTPUT_DIRECTORY EXPECTED_TEXT` serves an existing output
over loopback and uses Chrome DevTools Protocol to check visible root text,
the browser accessibility tree and console errors. It requires Chrome and
Python `websocket-client`. A hand-authored probe verified the harness; this
does not constitute a Maja build or browser qualification. The smoke check
does not test keyboard interaction, state, lifecycle, assistive technology,
or multiple browsers. The sandbox may require permission for loopback sockets.

`chrome_counter.py OUTPUT_DIRECTORY` exercises a `maja.web.local.v2` build of
`fixtures/nevada/counter.gscene` in headless Chrome through CDP: geometry
against the layout mirror, Tab order, Enter/Space/pointer activation,
field-conversion and overflow rejection with state unchanged, the
accessibility tree, reload and console errors. Outcomes are read from the
shell's read-only `goya:transition` event. It is written alongside the
implementation, so it is not independent qualification; see
`docs/maja/interactive-profile.md` for the last recorded run and its limits.

Run the independent local-build cases with
`MAJA_BIN=/absolute/path/to/maja PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/maja/browsers -p 'test_*.py' -v`.
These cover deterministic output, HTML text escaping, release rejection and
existing-output preservation. They are separate from browser verification.
