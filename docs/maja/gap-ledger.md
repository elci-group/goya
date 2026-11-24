# Wave 0 gap and release-blocker ledger

Snapshot: Goya `38c2f6132ef532e5b9b58b6a017e1f03cc1b3f5a`, Frampton `fcce0274fd7aa3f320fff0d9c3da917144b2fb93`; see baseline and ADR for checkout limitations. All entries remain open. Severity describes programme impact, not a measured vulnerability.

| ID / owner / contract | Observation and reproduction | Expected / actual; remediation and verification |
|---|---|---|
| M0-01 / B / Frampton boundary / blocker | Inspect Frampton HEAD README, HIR and runtime, as in ADR-001. | Browser presentation and execution interface required / none identified. Obtain authoritative implementation and pinned API; document coverage and run browser equivalence before enabling output. |
| M0-02 / A + integration / frontend / blocker | Inspect `crates/goya-lang/src/handoff.rs`, `parser.rs`, `scene.rs`, `transition.rs`. | Counter has authored initial state, input and view bindings / not defined by current source contract. Propose and approve semantics; verify complete typed and identity coverage. |
| M0-03 / A / origins / high | Inspect parser `SourceEntry` and strict diagnostics. | Precise origins for all target failures / only available enclosing or fallback origins in some cases. Report actual precision; extend via approved contract when necessary and test Unicode ranges. |
| M0-04 / G / baseline / high | Run CI commands at a controlled source revision; see baseline. | All mandatory CI passes / results individually recorded, any failure remains open. Reproduce on CI environment and address independently of Maja. |
| M0-05 / G / browser qualification / blocker | Inspect available browser and automation, then run planned browser fixture. | Executable Maja artifact, keyboard, accessibility and oracle evidence / no compiler or harness exists. Build independent harness after approved boundary; record browser versions and results. |
| M0-06 / integration / ownership / medium | `git worktree list --porcelain`; inspect dirty checkout. | Isolated assigned implementation worktrees / only shared main checkout currently exists. Assign and create as workers actually start, preserving existing changes and locking shared resources. |
| M0-07 / H / orchestration / medium | `playbook --help`. | Arbitrary programme submission / installed CLI exposes a fixed demonstration. Use documented packages as handoff until a suitable orchestration interface is identified. |

## Status update, 20 November 2026 ([wave 3](wave-3.md))

| ID | Status |
|---|---|
| M0-01 | Superseded for local builds by ADR-002 (no Frampton in local execution). Still open for any remote operation. |
| M0-02 | Implementation available: GSSH/2/3 author initial state, lifetimes and input bindings, and `maja.web.local.v2` consumes them without inference. **Open** until architecture approves GSSH/2; display bindings and `@input` `text:` remain undecided (decision D3). |
| M0-05 | Partially addressed: a generated counter ran in headless Chrome 157 and Firefox 150 against canonical D1 expectations (`tests/maja/browsers/maja_browser.py`). **Open**: harness is not independent, keyboard activation unverified in Firefox, no assistive technology, and inputs have no accessible name (new entry M3-01). |
| M3-01 / F / accessibility / blocker | `@input` elements have no accessible name in `maja.web.local.v2` output; reproduce with `maja_browser.py chrome counter` (reports the textbox name). Expected a programmatic name for every control; actual empty. GSSH/2 has no label construct and Maja must not invent one. Remedy: a label or accessible-name construct in a later GSSH profile, then emit it. |

## Status update, 21 November 2026

| ID | Status |
|---|---|
| M4-01 / A + integration / durable storage format / high | `maja.web.local.v2` now admits `@state … durable` and stores it in `localStorage` under a Maja-chosen record (`goya.state.v1/<ontology>`, see [interactive profile](interactive-profile.md#durable-state)). Reproduce with `maja_browser.py chrome durable` on a build of `tests/maja/fixtures/durable-counter.gscene`. Expected: a record format shared by every GSSH/2 target; actual: GSSH/2 defines semantics only, and Aragon still refuses `durable` (ARG-G07). Remedy: record the format (or a per-target freedom) in the contract when GSSH/2 is approved. **Open.** |
| M4-02 / F / durable multi-tab / medium | Two tabs of one durable app keep separate in-memory state; the last writer wins and the other tab's next commit overwrites it. Expected: a decided policy (reload on `storage` event, refuse, or document); actual: undecided and untested. **Open.** |

Later the same day the project owner approved the record format and chose
refuse-on-conflict for concurrent writers ([decision
D8](../gssh/decisions.md#d8--durable-record-format-and-concurrent-writers)):

| ID | Status |
|---|---|
| M4-01 | **Closed for Maja** by D8; stays open for Aragon, which must adopt the same record (ARG-G07). |
| M4-02 | **Closed**: the second writer gets `STORAGE_CONFLICT` and nothing is written (Node test, plus `maja_browser.py BROWSER durable` with the other window's write made from DevTools). Residual: no lock between the re-read and the write; a second real tab is not tested. |

The durable scenario now exercises two real tabs on the same origin. The
second tab commits through the app, then the stale tab gets
`STORAGE_CONFLICT` and adopts the winning value on reload. This covers
ordered writes. Simultaneous writes between the re-read and the write remain
outside this check.

GSSH/2 approval, same day:

| ID | Status |
|---|---|
| M0-02 | Contract approval **done** (GSSH/2 Approved; D3 `text:` = initial content, implemented in Maja). **Open** only for display bindings (D3). |
| M3-02 / A / GY-IR divergence / medium | `goya_lang::lowering` and `goya_sdk::Project::input_nodes` still present `@input` `text:` as a placeholder, contrary to D3. Not GSSH/2 consumers, so Maja is unaffected; remedy in their next contract revision. **Open.** |

Display bindings, same day ([decision D9](../gssh/decisions.md#d9--display-bindings-text-from-gssh3-live), proposed):

| ID | Status |
|---|---|
| M0-02 | Display bindings implemented as `text-from` in `maja.web.local.v3` (GSSH/3-live), passing in Chrome and Firefox. **Open** until D9 is approved and the Arachnid proposal is merged. |
| M4-03 / A + integration / shown-value wrapping / medium | Arachnid wraps `text-from` values (`pre-wrap`); Maja does not and refuses `text-from` with `wrap`. Expected: one rule in the contract. **Open** (D9 b). |
| M4-04 / F / announcements / medium | Changed values are not announced to assistive technology. Expected: a decided rule (live region or none). **Open** (D9 c). |

D9 approved, same day, with wrapping and announcement rules for every target:

| ID | Status |
|---|---|
| M0-02 | **Closed** for Maja: display bindings approved (D9) and implemented in `maja.web.local.v3`. |
| M4-03 | **Closed**: Arachnid's `pre-wrap` is the rule for all targets; Maja follows it and admits `text-from` with `wrap`. |
| M4-04 | **Closed for Maja**: polite announcements of what changed and how, worded by `goya_lang::display_binding` (Node differential and both browsers). Speech by a real screen reader is not verified. Arachnid must add announcements before its live profile merges (D9 compliance table). |

## Status update, 24 November 2026

An uncommitted cross-target change of 23 November made `@persistence` mandatory in every GSSH/2+ ontology. Reproduction: `cargo test -p maja --test interactive` at that tree; expected 9 passes, actual 9 failures (`MAJA-E0200 … persistence` on the v2 fixtures, `GY-E1014` on the v3 fixtures). It also named IndexedDB as the only browser adapter, contradicting D8. The project owner decided (D11) that approved profiles do not change, that `@persistence` is GSSH/4 only and optional, and that the browser defaults to `localStorage` but supports a declared IndexedDB.

| ID | Status |
|---|---|
| M5-01 / A / GSSH/2 regression / blocker | **Closed** by D11: GSSH/2, GSSH/3 and GSSH/3-live reject `@persistence` (`GY-E1015`) and their corpora are unchanged. All Maja tests pass, and every `maja_browser.py` scenario passes in Chrome 157 and Firefox 150. |
| M5-02 / A + integration / browser store / high | **Closed**: `maja.web.local.v2` resolves the `browser` target (default `browser.local-storage.v1`, declared `browser.indexeddb.v1`, anything else `MAJA-E0200`) and emits a matching `storage.js`; the IndexedDB path commits through the runtime's new `prepare`/`commit`. Verified by a Node test against a fake IndexedDB and `maja_browser.py BROWSER durable-idb` in both browsers. |
| M4-02 | Residual unchanged for `localStorage`. For IndexedDB the re-read and write share one transaction (Node fake only; not yet raced in a real browser). |
| M5-03 / A / live + persistence / medium | **Closed** by D12: `maja.web.local.v3` admits GSSH/4-live when a scene uses GSSH/4 declarations; `maja_browser.py BROWSER display-idb` passes in Chrome and Firefox. |
| M5-05 / A / default-store warning / medium | **Closed**: D11 ruling 3. `MAJA-W0001` on stderr when durable state uses the default browser store (Node CLI test, semantic test). Other targets must add the same warning when they implement durable state. |
| M5-04 / F / store migration / low | **Closed** by D13: `copy` by default, `move`/`none` by explicit ruling. Node test of six cases over both stores; `maja_browser.py BROWSER durable-idb` adopts a `localStorage` record into IndexedDB in Chrome and Firefox. |
| M3-01 | **Closed for Maja** by D14: every `@input` needs `@iden` (GSSH/4, `GY-E1017`); its label is the field's accessible name (Chrome accessibility tree: textbox "Amount"; Firefox: DOM). Screen-reader speech not verified. |
| M5-06 / A / profile base / medium | **Closed** by D15: `maja.web.local.v2`/`v3` admit GSSH/4 and GSSH/4-live directly. |

Every future failure adds source revision, exact command, expected and actual result, owner, severity, remediation, verification and status. A passing document check does not close an implementation or release blocker.
