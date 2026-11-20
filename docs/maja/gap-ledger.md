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
| M0-05 | Partially addressed: a generated counter ran in headless Chrome 157 against canonical D1 expectations (`tests/maja/browsers/chrome_counter.py`). **Open**: harness is not independent, single browser, no assistive technology, and inputs have no accessible name (new entry M3-01). |
| M3-01 / F / accessibility / blocker | `@input` elements have no accessible name in `maja.web.local.v2` output; reproduce with `chrome_counter.py` (reports the textbox name). Expected a programmatic name for every control; actual empty. GSSH/2 has no label construct and Maja must not invent one. Remedy: a label or accessible-name construct in a later GSSH profile, then emit it. |

Every future failure adds source revision, exact command, expected and actual result, owner, severity, remediation, verification and status. A passing document check does not close an implementation or release blocker.
