# Reason-Code Register (Disparates programme)

Register: 1. Date: 2026-11-19. Status: Candidate.

## Purpose

Single, versioned register of every reason code in use across the Disparates
programme. GY-FAIR/1 requires every non-`extracted` representability state to
carry a machine-readable reason code in register form (uppercase snake case);
free-form explanation goes in `detail`, never the code. This document is the
register those codes cite.

## Governance

- Codes are **never renamed, renumbered or retired**. A code's meaning is fixed
  by the revision that introduced it.
- New codes join **by contract revision only** and must be added to this
  register **in the same revision** that introduces them. A code emitted by any
  crate before it appears here is a defect in that revision.
- Every code belongs to exactly one category (below). Codes are added, never
  replaced; superseding a code means new code + a revised map/extractor
  version, evidenced by a fresh corpus run (see ADR 0001).
- Proposed codes (ADR 0001, capability proposals) are **not** registered until
  their introducing revision lands (see "Pending proposals").

## Categories

| Category | Meaning |
|---|---|
| `goya-capability-deficit` | Blocked because a Goya capability is absent; Goya-side engineering demand. |
| `capability-decision-pending` | Blocked pending a Goya-side scoping/authority decision, not yet a deficit claim. |
| `programme-debt` | Tooling or map immaturity (assessment-method deficit); not Goya demand. |
| `mapping` | `representable` outcome; the code records the Goya mapping used. |
| `extraction-limit` | Extractor-recorded limit at extraction/assessment time; ignorance preserved. |

## Register

Evidence paths are relative to `evidence/disparates/`. "No corpus occurrence"
means no evidence run has emitted the code yet; first evidence is then the
defining source and its test.

| Code | Category | Meaning | Introduced by | First evidence |
|---|---|---|---|---|
| `MAPPED_TO_SCENE_DECLARATION` | `mapping` | Component structure maps to GY-LANG/1 scene node declarations. | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.assessed.fair.json` (22 components) |
| `MAPPED_TO_ACTION_DECLARATION` | `mapping` | Interaction handlers map to declared GY-IR/1 actions with GY-RUNTIME/1 dispatch. | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.assessed.fair.json` (87 event handlers) |
| `MAPPED_TO_ACTION_DISPATCH` | `mapping` | IPC channel requests/responses become actions; architectural simplification, not 1:1 transport mapping. | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.assessed.fair.json` (9 IPC channels) |
| `MAPPED_TO_SCENE_STYLING` | `mapping` | Static visual declarations map to GY-CHROMA/1 scene styling and GY-LAYOUT/1 constraints. | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.assessed.fair.json` (130 style rules) |
| `MAPPED_TO_NATIVE_WINDOW_HOST` | `mapping` | Application window lifecycle maps to the native windowing host (GY-WAYLAND/1; goya-window-management; Sash/Emperor parity). | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.assessed.fair.json` (1 window lifecycle) |
| `MAPPED_TO_CLIPBOARD_CAPABILITY` | `mapping` | Clipboard write calls map to GY-CLIPBOARD/1 capability-gated clipboard write (Verbatim / Deep Copy behind ClipboardCapability/CapabilityPolicy). Map revision 2 (2026-11-19); coarse `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` remains for shell/dialog/notification/tray. | GY-COVERAGE/1 (2026-11-19, map revision 2) | `reactotron-phase1/reference.assessed.fair.json` (3 clipboard elements; fiddle: 14 incl. template/test copies) |
| `HOOK_DATAFLOW_NOT_EXTRACTED` | `extraction-limit` | Hook recorded as occurrence site without data-flow; `useState`→GY-STATE and effect scheduling cannot be assessed. Expected superseded by AST-grade extraction (ADR 0001) via map revision, never silently. | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.deficit.json` (63 hooks; largest deficit) |
| `COMPUTED_CHANNEL_UNRESOLVED` | `extraction-limit` | IPC channel argument is computed (template/call/member outside a file-local const map); the observed spelling is recorded under `channel-observed`. | goya-disparate-electron 0.2.0 (AST) | fixture test `computed_channels_resolve_and_preserve` (no corpus occurrence yet) |
| `REQUIRE_SCOPE_UNRESOLVED` | `extraction-limit` | `require()` argument binds to no same-file const require binding (or is a computed expression / dynamic `import()`). | goya-disparate-electron 0.2.0 (AST) | fixture test `dynamic_require_cases_are_all_represented` (no corpus occurrence yet) |
| `REQUIRE_DEPTH_EXCEEDED` | `extraction-limit` | `require()` argument chain exceeds the declared one-indirection bound (member access on non-const or nested objects). | goya-disparate-electron 0.2.0 (AST) | fixture coverage of the member path (no corpus occurrence yet) |
| `PERSISTENCE_CAPABILITY_UNRESOLVED` | `capability-decision-pending` | Persistence construct (filesystem access; key-value store) has no declared Goya equivalent; mapping requires a scoping and authority decision first. Split at a future map revision is anticipated (`capabilities/persistence-scoping.md`). | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.deficit.json` (9 filesystem operations + 3 state stores) |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | `goya-capability-deficit` | Goya provides no child-process model; foreign spawn/exec logic is blocked. Absence claim with stated check date (2026-11-19). | GY-FAIR/1 (2026-11-18, named as register example); assigned by GY-COVERAGE/1 map | `reactotron-phase1/reference.deficit.json` (9 child processes) |
| `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` | `goya-capability-deficit` | No notification/dialog/shell/tray capability exists; each specific desktop OS API is a candidate capability deficit. Absence claim with stated check date (2026-11-19). | GY-COVERAGE/1 (2026-11-19) | `reactotron-phase1/reference.deficit.json` (12 OS API calls) |
| `NATIVE_MODULE_OPAQUE` | `goya-capability-deficit` | Native addon dependency cannot be transliterated; must be replaced by a Goya capability or recorded as a migration wall. Extractor-assigned at extraction time with final `unrepresentable` status. | GY-FAIR/1 (2026-11-18, named as register example) | No corpus occurrence (both runs native-free); `crates/goya-disparate-electron` (`push_native_module`) and its tests |
| `DYNAMIC_REQUIRE_UNRESOLVED` | `extraction-limit` | `require`/`import` destination is computed; not statically resolvable by extractor 0.1.0; preserved as `undetermined` (FAIR preserves ignorance). | GY-FAIR/1 (2026-11-18, named as register example) | `electron-fiddle-phase2/extraction.fair.json` (29; reactotron phase 1 had none — all 345 elements `extracted`) |
| `DYNAMIC_CODE_EVALUATION` | `extraction-limit` | Dynamic code evaluation (`eval(`, `new Function(`) has no statically extractable semantics; observed spelling recorded, `undetermined`. | GY-FAIR/1 (extractor discipline; code introduced with `goya-disparate-electron` 0.1.0) | No corpus occurrence; `crates/goya-disparate-electron` scanner and its tests |
| `NO_COVERAGE_RULE` | `programme-debt` | No rule in capability map v1 covers the construct; assessment leaves it `undetermined`. The map must grow by contract revision, never silently. | GY-COVERAGE/1 (2026-11-19) | No corpus occurrence; `crates/goya-disparate-coverage` `assess()` fallback and its test |
| `UNREASONED` | `programme-debt` | Deficit-report aggregation sentinel for a blocked element carrying no reason code. Integrity marker: must never appear in a report derived from a validated GY-FAIR/1 document; any occurrence is a defect. | GY-COVERAGE/1 (deficit derivation; source-only) | No occurrence anywhere by design; `crates/goya-disparate-coverage` `DeficitReport::derive` |

## Not in this register

- **goya-fair validation error codes** (`CONTRACT_MISMATCH`,
  `MISSING_SOURCE_MANIFEST`, `BAD_SOURCE_DIGEST`, `MISSING_ID_RULE`,
  `EMPTY_ELEMENT_ID`, `DUPLICATE_ELEMENT_ID`, `UNKNOWN_KIND_UNRECORDED`,
  `ZERO_LINE`, `BAD_SPAN`, `BAD_REASON_CODE`, `MISSING_REASON_CODE`) — a
  separate namespace: structural validation outcomes, not representability
  reason codes. Governed by GY-FAIR/1.
- **`STYLED_COMPONENTS_DYNAMIC`** — test-local illustration in
  `crates/goya-disparate-coverage` (first-match-wins semantics-facet test).
  Never emitted by any production path; introduced by no contract revision;
  must not be used outside tests.

## Pending proposals (not registered)

Registered only when their introducing revision lands, in that revision:

- `capabilities/persistence-scoping.md` (Status: Proposal):
  `NO_GOYA_CAPABILITY_FILESYSTEM`, `OUT_OF_RECONSTRUCTION_SCOPE` (proposed
  spelling), plus the split of `PERSISTENCE_CAPABILITY_UNRESOLVED`.
- `capabilities/desktop-os-integration.md` (Status: Proposal):
  `MAPPED_TO_URI_DELEGATION`, `MAPPED_TO_MODAL_DIALOG` — reserved, pending
  GY-DESKINT/1 ratification; may not be emitted until then.
- `revisions/map-v2-proposal.md` (Amendment B, applied 2026-11-19):
  `MAPPED_TO_CLIPBOARD_CAPABILITY` registered above in the same revision.
- ADR 0001 codes **registered** (extractor 0.2.0, 2026-11-19):
  `COMPUTED_CHANNEL_UNRESOLVED`, `REQUIRE_SCOPE_UNRESOLVED`,
  `REQUIRE_DEPTH_EXCEEDED`. **Still pending** (not emitted by 0.2.0):
  `HOOK_DATAFLOW_PARTIAL`, `HOOK_DATAFLOW_UNRESOLVED`,
  `EFFECT_BODY_UNCLASSIFIED`, `FILE_TOO_LARGE_FOR_AST`.
- `evidence/disparates/electron-fiddle-phase2/comparison.md` (flagged for
  future register discussion): `TEMPLATE_DATA_NOT_APP_SOURCE`.

## Stability

Candidate. The register grows by contract revision; existing codes are never
renamed or renumbered. The register must be updated in the same revision that
introduces a code; codes observed in sources but absent here are a defect in
the revision that emitted them.
