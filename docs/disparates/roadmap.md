# Disparates programme roadmap

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-18. Status: Candidate.

The Disparates programme migrates foreign applications to native, maintainable Goya
source. Its architectural division of responsibility is:

- **Extraction** — understanding foreign applications (FAIR production).
- **Representability** — determining whether Goya can express the extracted semantics.
- **Reconstruction** — generating valid, maintainable Goya implementations.

These responsibilities are independently testable and versioned. Reconstruction is
withheld until representational feasibility is demonstrated on a reference corpus.

## Phases

| Phase | Name                          | Status      | Exit condition |
|-------|-------------------------------|-------------|----------------|
| 0     | Contracts                     | Complete | GY-FAIR/1 contract + `goya-fair` crate with validation, canonical serialisation, fixtures and evidence |
| 1     | Semantic extraction           | First milestone met (2026-11-18); representability milestone met (2026-11-19, GY-COVERAGE/1); extractor limits recorded | One bounded Electron + React reference application fully extracted; every known construct represented or explicitly classified; no Goya generation |
| 2     | Reference corpus expansion    | Not started | Progressively complex applications expose architectural assumptions and missing capabilities |
| 3     | Behavioural replay            | Not started | Original-application interaction scenarios captured as normalised state traces |
| 4     | Native reconstruction         | Blocked (authorisation withheld) | Goya source generated only for supported constructs, every semantic decision recorded |
| 5     | Windows/Linux equivalence     | Not started | Reconstructed applications compiled through Sash and Emperor and validated against behavioural contracts |
| 6     | Adoption demonstrator         | Not started | Substantial third-party FOSS application migrated with reproducible equivalence and performance evidence |
| 7     | Ecosystem integration         | Not started | Integration with Inquisition / Quinta / Studio as their contracts mature |

Behavioural replay (phase 3) remains a first-wave commitment even though the phase 1
extraction milestone completes before any reconstruction begins.

## Reference corpus selection (phase 1)

**Nomination (2026-11-18): `infinitered/reactotron`, app slice `apps/reactotron-app`,
MIT, pinned at commit `acdd7c73e7ecf9d8d79f6fcf9201a48aefaf270a`** (corpus location
`/home/sal/corpus/reactotron`; extraction evidence in
`evidence/disparates/reactotron-phase1/`). All four criteria verified at nomination:
single `BrowserWindow` site; no native addon dependencies; 5 `ipcMain` channels; MIT;
reproducible from the pinned commit. Corpus expansion (phase 2) may nominate further
applications against the same criteria.

Selection criteria, which must be recorded with each nomination:

1. Single window; no native modules (`.node` addons).
2. Small, bounded IPC channel set; no dynamic `require()` on hot paths — or, where
   present, they are expected to surface as `undetermined` elements.
3. Permissive license permitting derivation and attribution-preserving redistribution.
4. Reproducible build from a pinned commit.

## Non-negotiable constraints

- No contamination of `goya-ir` with foreign framework semantics.
- No silent functionality omission or behavioural simplification.
- No dependency on unimplemented Goya subsystems for the initial programme.
- No execution of untrusted foreign application code during static extraction.
- No successful migration declaration without reproducible evidence.
- No unauthorised removal of copyright, licensing or attribution obligations.
- No mandatory dependence on LLM inference for deterministic extraction or
  reconstruction.

## Capability deficit discovery

**Findings under capability map v2 (2026-11-19; v1 originals preserved
alongside in evidence; revision recorded in `revisions/map-v2-proposal.md`):**

| Reason code | Reactotron (345 el.) | Fiddle (401 el.) | Category |
|---|---|---|---|
| `HOOK_DATAFLOW_NOT_EXTRACTED` | 63 | 0 | programme debt (extractor) |
| `DYNAMIC_REQUIRE_UNRESOLVED` | 0 | 29 | programme debt (extractor) |
| `PERSISTENCE_CAPABILITY_UNRESOLVED` | 12 | 127 | Goya capability decision pending |
| `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` | 9 | 52 | Goya capability deficit |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | 9 | 10 | Goya capability deficit |

Map-v2 totals: Reactotron 252 representable / 75 undetermined / 18
unrepresentable; fiddle 183 / 156 / 62. v1 numbers (249/75/21 and 169/156/76)
remain in the phase evidence files as historical records.

Corrections applied in map revision 2, each evidence-backed: (1) 3 Reactotron
clipboard elements reclassified to `MAPPED_TO_CLIPBOARD_CAPABILITY` — the
capability already existed (`GY-CLIPBOARD/1`); (2) the fiddle deficit was
found template-inflated (shipped example fiddles extracted as if app source)
and partially tooling-inflated in Reactotron (`scripts/` build tooling) —
scan-scope and `OUT_OF_RECONSTRUCTION_SCOPE` dispositions are proposed, not
yet applied. The report deliberately separates Goya capability deficits from
programme debt so development is not prioritised by extractor immaturity.

A first-class output of the programme: aggregates of `unrepresentable` reason codes
across the corpus identify which missing Goya capabilities block adoption, prioritising
Goya development by observed migration blockers rather than speculative demand. Every
unsuccessful migration produces engineering intelligence.

## Proposed crate family (names reserved, not yet implemented)

`goya-fair` (implemented), `goya-disparate-electron` (implemented),
`goya-disparate-coverage` (implemented), `goya-disparates` (implemented —
pipeline orchestration, `disparate` CLI), `goya-disparate-react`,
`goya-disparate-typescript`, `goya-disparate-replay`,
`goya-disparate-reconstruct`. Only components required by the
 authorised phases are implemented; the rest are placeholders, not assertions of support.
