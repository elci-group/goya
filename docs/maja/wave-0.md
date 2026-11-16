# Maja Wave 0 reconnaissance

Snapshot: 2026-11-15; Goya HEAD `38c2f6132ef532e5b9b58b6a017e1f03cc1b3f5a`. The working tree contains pre-existing unrelated Android changes, including `goya-sdk`; observations are not a clean-revision audit. No shared frontend or workspace manifest was changed.

## Findings

* The canonical strict entry point is `goya_lang::handoff::admit` (`crates/goya-lang/src/handoff.rs`). Its GSSH/1 handoff is provisional and in-process, with incomplete origins and no resolved bindings or persistence. `SceneProject::from_source` uses permissive parsing and cannot alone provide fail-closed admission.
* Retain the strictly admitted scene, interaction declarations and origins through target admission. The shared `goya-ir` projection and layout side table alone omit visual properties, expressions, origins and responsive semantics. Inspect `crates/goya-lang/src/{scene,parser,lowering,transition}.rs` when specifying the handoff.
* `goya-sdk/src/application.rs` supplies a D1 execution oracle with caller-provided typed initial state and transactional commits. The scene contract does not establish the initial state, view bindings or input-to-action bindings needed by the acceptance counter. Resolve them through an approved contract change or explicit profile inputs; do not invent source semantics. D1 bytecode is private; a validated AST lowering or approved export is needed. Exact signed 64-bit arithmetic and overflow require browser differential tests.
* The local Frampton browser interface is unverified; see [ADR-001](adr-001-frampton.md). No Maja compiler or Frampton browser integration is established.
* `.github/workflows/ci.yml` defines CI. The targeted command `cargo test --locked -p goya-lang -p goya-state -p goya-ir --offline` passed 83, 6 and 6 unit tests respectively, plus doc tests. This is not full CI. Chrome and Cargo were found; Node, npx and Playwright were not found on PATH. Real-browser automation remains an open requirement.
* Candidate existing fixtures include `crates/neptune/conformance/d1-increment.gscene` and `d1-text.gscene`; `crates/goya-sdk/tests/contract_conformance.rs` provides existing conformance tests. Applicability to Maja requires independent validation.

## Profile and gate

The integration authority has frozen a [fail-closed initial policy](contracts/maja-web-1.md), [frontend handoff boundary](contracts/frontend-handoff.md), and [admission matrix](admission-matrix.md) for planning and independently testable skeletons. `maja.web.v1` remains a reserved identifier, not an implemented or qualified profile. All features are **REJECTED for production builds**. No fallback is selected. Static text, hierarchy, controls, i64 D1 transitions and explicit initial state and binding are investigation candidates, not supported features. Network, storage, persistence, navigation, service workers and dynamic imports require individual admission; default deny.

M0 remains **open**: establish an authoritative Frampton browser interface; approve a complete handoff and binding/initial-state model; review an actual supported feature matrix and fixture corpus; resolve baseline failures; assign isolated worktrees and approve contracts. Browser, accessibility, security and reproducibility qualification have not run. See [work packages](work-packages.md), [ownership](ownership.md), [baseline](baseline.md), and [gap ledger](gap-ledger.md).

The installed `playbook --help` exposes a fixed acceptance demonstration and replay commands, not submission of arbitrary programmes. `playbook status --basic` reports the demonstration's state, not Maja's progress. These documents are a handoff for an appropriate orchestrator, not evidence that Playbook accepted or executed Maja.
