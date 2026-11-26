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
| 0     | Contracts                     | In progress | GY-FAIR/1 contract (superseded by GY-FAIR/2, 26 November) + `goya-fair` crate with validation, canonical serialisation, fixtures and evidence |
| 1     | Semantic extraction           | In progress | One bounded Electron + React reference application fully extracted; every known construct represented or explicitly classified; no Goya generation |
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

## Forward planning: replace Oxc (high priority)

**Owner decision, 26 November 2026:** keep Oxc 0.146 for current JavaScript,
JSX and TypeScript extraction. Replacing it with a Goya-owned implementation
is a **high-priority planned work item**. Implementation has not started;
current extraction continues using Oxc.

The replacement must cover the syntax and AST traversal used by
`goya-disparate-electron`, including exact source spans and explicit parse-error
reporting. Plan the language scope, conformance corpus and staged integration
before implementation. Validate the replacement against that corpus and compare
its FAIR output with Oxc on the reference applications; account for every
difference before switching. Remove Oxc and its unused dependency tree only
after these checks pass. The existing Goya HTML and CSS crates remain in use.

This priority does not lift the Phase 4 reconstruction authorisation block.

## Proposed crate family (names reserved, not yet implemented)

`goya-fair` (implemented), `goya-disparates` (orchestration), `goya-disparate-react`,
`goya-disparate-electron` (v0.1.0 implemented, 26 November 2026; see below), `goya-disparate-typescript`, `goya-disparate-coverage`,
`goya-disparate-replay`, `goya-disparate-reconstruct`. Only components required by the
 authorised phases are implemented; the rest are placeholders, not assertions of support.

## Phase 1 progress (26 November 2026)

Directive DEL-TD-001 (Delectable) was adopted as the directive for
`goya-disparate-electron` (owner decision, 26 November;
[DELECTABLE-0](../delectable/DELECTABLE-0.md)). This programme was brought onto
`nebula/v0.1` the same day.

* **First corpus entry:** `fixtures/disparates/electron-counter` (DEL-B01). It
  was written in this repository under the workspace licence and meets
  criteria 1–4 above. It has no React, so the Phase 1 exit condition (an
  Electron + React application) is not yet met.
* **Extractor:** `crates/goya-disparate-electron` v0.2.0. It reads a directory
  or an `app.asar` and is static, offline and deterministic. It emits FAIR/2:
  every element is `extracted` or `undetermined`, with typed relationships and
  a project record. It is tested against a golden file and the fixture's
  oracle.
* **Contract:** [GY-FAIR/2](contracts/fair-2.md), approved by the owner and
  implemented on 26 November. It covers relationships, four kinds, a project
  record and the [reason-code register](contracts/reason-codes.md). FAIR/1
  documents remain readable.
