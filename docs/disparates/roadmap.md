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
| 0     | Contracts                     | In progress | GY-FAIR/1 contract + `goya-fair` crate with validation, canonical serialisation, fixtures and evidence |
| 1     | Semantic extraction           | Not started | One bounded Electron + React reference application fully extracted; every known construct represented or explicitly classified; no Goya generation |
| 2     | Reference corpus expansion    | Not started | Progressively complex applications expose architectural assumptions and missing capabilities |
| 3     | Behavioural replay            | Not started | Original-application interaction scenarios captured as normalised state traces |
| 4     | Native reconstruction         | Blocked (authorisation withheld) | Goya source generated only for supported constructs, every semantic decision recorded |
| 5     | Windows/Linux equivalence     | Not started | Reconstructed applications compiled through Sash and Emperor and validated against behavioural contracts |
| 6     | Adoption demonstrator         | Not started | Substantial third-party FOSS application migrated with reproducible equivalence and performance evidence |
| 7     | Ecosystem integration         | Not started | Integration with Inquisition / Quinta / Studio as their contracts mature |

Behavioural replay (phase 3) remains a first-wave commitment even though the phase 1
extraction milestone completes before any reconstruction begins.

## Reference corpus selection (phase 1)

Corpus selection is deferred; no reference application is nominated at contract time.
Selection criteria, which must be recorded with the nomination:

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

A first-class output of the programme: aggregates of `unrepresentable` reason codes
across the corpus identify which missing Goya capabilities block adoption, prioritising
Goya development by observed migration blockers rather than speculative demand. Every
unsuccessful migration produces engineering intelligence.

## Proposed crate family (names reserved, not yet implemented)

`goya-fair` (implemented), `goya-disparates` (orchestration), `goya-disparate-react`,
`goya-disparate-electron`, `goya-disparate-typescript`, `goya-disparate-coverage`,
`goya-disparate-replay`, `goya-disparate-reconstruct`. Only components required by the
 authorised phases are implemented; the rest are placeholders, not assertions of support.
