# Quinta import and migration flow

**Conceptual — phase 7 target; nothing in this document is implemented. Quinta
itself does not exist in the workspace (closest: goya-studio).**

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19. Status: Conceptual.

This document describes how a Quinta UI would present the Disparates pipeline to a
developer importing and assessing a foreign application. It maps the implemented
pipeline (phases 0–1 outputs only) to UI steps and derives UI rules from programme
invariants. Every UI element below is aspirational; the named CLIs and artifacts are
the implemented reality the UI would render. Nothing here changes any contract.

## Implemented pipeline → Quinta UX mapping

What exists today, and nothing else:

| Implemented artifact | Producer | Quinta UI step (aspirational) |
|---|---|---|
| Canonical GY-FAIR/1 document | `disparate-extract` (`crates/goya-disparate-electron`, GY-DISP-001) | Import |
| Framework detections, entry points, extractor version, SHA-256 source manifest | Document-level provenance in the FAIR document | Detection evidence display |
| Assessed FAIR document (per-element status, reason code, detail, `goya_evidence`) | `disparate-assess` (`crates/goya-disparate-coverage`, GY-COVERAGE/1) | Coverage explorer |
| Deficit report (GY-COVERAGE/1 `DeficitReport::derive`) | `disparate-assess` | Remediation worklist |
| Run provenance (corpus identity, pinned commit, dates) | `evidence/disparates/<run>/run.json` | Import summary |

Reference instance (Reactotron phase 1, `evidence/disparates/reactotron-phase1/`):
`reference.fair.json`, `reference.assessed.fair.json`, `reference.deficit.json`,
`run.json` — 345 elements; 249 `representable`, 75 `undetermined`, 21
`unrepresentable`.

**Import** (aspirational). The developer runs `disparate-extract` against a pinned
foreign source tree and imports the canonical FAIR document plus its `run.json`.
Quinta would render the source-manifest binding (SHA-256), extractor identity, and
pinned commit as the evidence that the import is reproducible. Extraction stays
offline, static, and LLM-free; the UI performs no extraction itself.

**Detection evidence display** (aspirational). Quinta would render the FAIR
document's framework detections with their recorded detection evidence, entry
points, and extractor version — not a bare framework label. Detection claims are
shown as claims with evidence, never as facts without provenance.

**Coverage explorer** (aspirational). Renders `reference.assessed.fair.json`
element-by-element: element kind, provenance (workspace-relative `file`, 1-based
`line`, byte span where the extractor has it, `fallback_origin` flag),
representability status, reason code, `detail`, and semantics facets including the
assessment-added `goya_evidence`. Aggregate counters may accompany this list; every
figure links into it.

**Remediation worklist** (aspirational). Renders `reference.deficit.json`: per
reason code — blocking count, `goya_evidence`, kind breakdown, affected files — in
the report's deterministic order (blocking count descending).

## Honesty-preserving UI requirements

Derived from GY-FAIR/1, GY-COVERAGE/1 and roadmap constraints. These are
requirements on any future Quinta implementation, not descriptions of existing code.

1. **No percentage without the element report.** Aggregate coverage figures may be
   displayed but are never the unit of truth, are never persisted as authoritative,
   and are always accompanied by the per-element report (GY-FAIR/1 coverage rules).
2. **Progression, not progress.** Representability is a seven-state evidence
   progression, not a progress percentage or bar toward completion.
3. **`representable` is visually distinct from `translated`/`verified`.**
   `representable` means named Goya contract evidence exists (`MAPPED_TO_*` reason
   codes), assessment sense only. No pipeline today can produce `translated` or
   `verified` (reconstruction authority is withheld; `verified` requires replay
   evidence under a future contract); the UI must not offer, simulate, or preview
   those states.
4. **Two worklist categories, never merged.** Goya capability deficits
   (`NO_GOYA_CAPABILITY_*`, `PERSISTENCE_CAPABILITY_UNRESOLVED`) and assessment-method
   deficits (`HOOK_DATAFLOW_NOT_EXTRACTED`, `NO_COVERAGE_RULE`, extractor-level
   `DYNAMIC_REQUIRE_UNRESOLVED`) are displayed as the distinct categories GY-COVERAGE/1
   defines. Programme debt must not be presented as, or prioritised as, Goya demand.
5. **Absence claims carry their check date.** `NO_GOYA_CAPABILITY_*` outcomes state a
   check date in `goya_evidence`; the UI displays it wherever the claim appears.
6. **Semantic decisions require acknowledgement.** Mappings that are architectural
   simplifications — IPC channels mapped to `MAPPED_TO_ACTION_DISPATCH` — are shown as
   decisions requiring explicit user acknowledgement, and recorded as user-approved,
   attributed decisions. Silent acceptance is prohibited: no silent functionality
   omission or behavioural simplification.
7. **Preserve ignorance.** `undetermined` and `unrepresentable` populations are always
   visible and counted; no view, filter, or collapse may remove them from totals.
8. **Provenance honesty.** Absent byte spans are shown as absent (fabricated spans
   are prohibited); fallback origins are marked; reason codes are displayed verbatim
   with `detail` as separate free-form text; codes are never paraphrased or renamed.
9. **The UI never edits status.** Element states change only by re-running the
   deterministic pipeline (re-extraction from revised source, or a versioned
   capability-map revision followed by `disparate-assess`); results must re-validate
   under GY-FAIR/1. Assessment never downgrades an existing classification.

Where Quinta is built, it would inherit goya-studio's existing serve conventions —
loopback-only binding, validation-gated writes, revision-token conflict detection —
aspirational for Quinta itself.

## Remediation workflow

Nothing automated exists today. For a `remediation_required` or `unrepresentable`
element, the developer works directly with the artifacts:

1. **Locate.** From a deficit-report entry (reason → kinds → files), find the element
   IDs in the assessed FAIR document, then its provenance (`file`, `line`, span).
2. **Consult evidence.** Read the `goya_evidence` facet (naming contract-register
   state and check date), the `detail` field, and the capability contracts in the
   register (`docs/contracts.md`).
3. **Decide**, one of:
   - **Replace the dependency** — change the foreign source; re-pin the corpus
     commit; re-run `disparate-extract` and `disparate-assess` (a new source
     manifest results).
   - **Declare a wall** — the element stays `unrepresentable` with its reason code;
     the deficit aggregate is the record, and it feeds the programme's
     capability-priority intelligence (every unsuccessful migration produces
     engineering intelligence).
   - **Extend Goya** — a capability contract enters the register; the `CapabilityMap`
     is revised (the map is data: versioned, reviewable, testable); `disparate-assess`
     is re-run; the element upgrades only through assessment.
4. **Record** (aspirational). Quinta would record the decision as an explicit,
   attributed revision entry — author, date, decision, referenced reason code and
   evidence — applied only by re-running the deterministic pipeline with a versioned
   input change (revised source tree or revised map). In-place status mutation is
   prohibited; revisions are honest and attributed, never silent rewrites.

## LLM policy

- **Prohibited**: extraction, validation, serialisation, assessment, verdicts, and
  coverage counts. Extraction is fully deterministic and offline; no LLM inference
  participates (GY-FAIR/1 determinism). The programme has no mandatory dependence on
  LLM inference for deterministic extraction or reconstruction.
- **Permitted (aspirational, phase 7)**: remediation proposals; suggestions for
  triaging ambiguous semantics (`undetermined` elements).
- **Subordination**: a proposal never changes an element status, a count, or a
  verdict. Acceptance means a human decision that takes effect only through the
  deterministic pipeline, and results are re-verified by `disparate-assess`.
  LLM assistance is always subordinate to deterministic verification.

## Out-of-scope

Pending their phases; not designed here:

- **Replay comparison views** — pending phase 3 (behavioural replay); no normalised
  state traces exist.
- **Reconstruction review UI** — pending phase 4 (blocked; reconstruction
  authorisation withheld; no Goya source is generated today).
- **Build-target selection** — pending phase 5 (Windows/Linux equivalence through
  Sash and Emperor).

Integration with Inquisition / Quinta / Studio is itself the phase 7 exit condition
and matures with their contracts.
