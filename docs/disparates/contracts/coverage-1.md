# GY-COVERAGE/1 — Representability Assessment and Capability-Deficit Report

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19. Status: Candidate.
Assessment authority only; no reconstruction is authorised under this revision.

## Purpose

GY-COVERAGE/1 upgrades GY-FAIR `extracted` elements to honest representability
classifications by assessing them against a versioned, serialisable **capability
map** — and derives the programme's engineering-intelligence output: the
capability-deficit report, aggregating what actually blocks migration by reason code.

## Provides

- `crates/goya-disparate-coverage` — `pub const COVERAGE_CONTRACT: &str = "GY-COVERAGE/1"`.
- `CapabilityMap` — versioned rule set (`version` + ordered `rules`, first match
  wins), serialisable, loadable from JSON (`from_json` / `load_map`), `validate`.
- `Rule` — selector (optional element kind + required semantics facets) and
  disposition (status, reason code, `goya_evidence`, detail).
- `assess(&FairDocument, &CapabilityMap) -> Assessment` — upgraded document plus
  upgrade counts.
- `DeficitReport::derive(&FairDocument)` — aggregated, sorted deficit report with
  canonical serialisation.
- `disparate-assess` CLI — FAIR document in, assessed document + deficit report out.
- `default_map()` — the v1 map, grounded in the contract register as of
  2026-11-19 (see below).

## Consumes

- `goya-fair` (GY-FAIR/2). Assessed documents must re-validate under GY-FAIR/2,
  so every code a map emits must be in the FAIR/2 register
  ([reason-codes.md](reason-codes.md)); `CapabilityMap::validate` refuses others.

## Rules

- Only `extracted` elements are assessed. Existing `undetermined` /
  `unrepresentable` classifications are preserved; assessment never downgrades or
  silently re-decides them.
- **Every rule outcome carries a reason code** — including `representable`
  outcomes, whose codes record the mapping itself
  (`MAPPED_TO_SCENE_DECLARATION`, `MAPPED_TO_ACTION_DECLARATION`,
  `MAPPED_TO_ACTION_DISPATCH`, `MAPPED_TO_SCENE_STYLING`,
  `MAPPED_TO_NATIVE_WINDOW_HOST`, `MAPPED_TO_CLIPBOARD_CAPABILITY`;
  `MAPPED_TO_URI_DELEGATION` and `MAPPED_TO_MODAL_DIALOG` are reserved,
  pending GY-DESKINT/1 ratification and may not be emitted until then).
- `representable` is claimed **only** with named Goya contract evidence.
  Absence claims (`NO_GOYA_CAPABILITY_*`) require a stated check date.
  Elements with no matching rule become `undetermined` with
  `NO_COVERAGE_RULE`; the map must grow by contract revision, never silently.
- Assessment preserves the FAIR semantics payload; `goya_evidence` is
  recorded as an added facet, not a replacement.
- Two distinct blocking categories must not be conflated, and consumers of the
  report must be able to tell them apart:
  - **Goya capability deficits** (`NO_GOYA_CAPABILITY_*`,
    `PERSISTENCE_CAPABILITY_UNRESOLVED`) — Goya-side engineering demand.
  - **Assessment-method deficits** (`HOOK_DATAFLOW_NOT_EXTRACTED`,
    `NO_COVERAGE_RULE`, extractor-level `DYNAMIC_REQUIRE_UNRESOLVED`) —
    extractor or map immaturity. These are programme debt, not Goya demand.

## V1 capability-map findings (grounding)

| Foreign construct | Disposition | Goya evidence |
|---|---|---|
| Component structure | Representable | GY-LANG/1 scenes; GY-IR/1 nodes |
| Interaction handlers | Representable | GY-IR/1 actions; GY-RUNTIME/1 |
| Electron IPC channels | Representable (architectural simplification) | GY-IR/1 actions; GY-RUNTIME/1 |
| Static styling | Representable | GY-CHROMA/1; GY-LAYOUT/1 |
| Window lifecycle | Representable | GY-WAYLAND/1; goya-window-management; Sash/Emperor parity |
| Hooks | Undetermined | GY-STATE/1 covers value state; effect scheduling unassessed |
| Filesystem access | Undetermined | no filesystem contract in register (checked 2026-11-19) |
| Key-value persistence | Undetermined | GY-STATE/1 + GY-HOTSWAP/1 declare no disk persistence |
| Child processes | Unrepresentable | no process-spawn capability (checked 2026-11-19) |
| Desktop OS integration | Unrepresentable | no notification/dialog/shell/tray capability (checked 2026-11-19) |

## Invariants

- Assessed documents validate under GY-FAIR/2.
- Reports are deterministic: identical inputs yield byte-identical canonical
  output; deficits sort by blocking count descending.
- The map is data: serialisable, versioned, reviewable, testable — assessment
  policy changes are map revisions, not code changes.
- No `verified` status is producible under this revision; representability is
  not equivalence.

## Stability

Candidate. Reason-code register grows by revision; existing codes never renamed.
The `Hook` disposition (`HOOK_DATAFLOW_NOT_EXTRACTED`) is expected to be
 superseded once extraction records data-flow facets; that is a map revision
 with a new extractor version, evidenced, not a silent upgrade.

## Map revision history

- **v1 (2026-11-19)** — initial map; first corpus runs (Reactotron, fiddle).
- **v2 (2026-11-19)** — inserted `os_api_call api=clipboard → representable /
  MAPPED_TO_CLIPBOARD_CAPABILITY` (GY-CLIPBOARD/1 evidence) before the coarse
  desktop-OS rule. Format version stays `1` (no schema change). Pending rules
  and the scope-exclusion schema amendment are recorded in
  [revisions/map-v2-proposal.md](../revisions/map-v2-proposal.md); the reserved
  shell/dialog codes may not be emitted until GY-DESKINT/1 is ratified.
  Embedded fixture `fixtures/capability-map-v1.json` regenerated in the same
  revision; a test enforces byte-equality with `default_map()`.
