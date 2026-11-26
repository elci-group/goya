# Disparates reconnaissance

Directive: GY-ENT-001 (Disparates programme). Date: 19 November 2026. Status: inspection only. This file does not admit a release.

This is the programme's audit-facing state-of-the-world record. Every observation
below was taken on 2026-11-19 between 11:23 and 11:31 UTC on the host described in
Repository topology. This file records facts. It does not propose anything, decide
anything, or authorise anything.

## Repository topology

`git worktree list`, run 2026-11-19 11:23 UTC:

| Worktree | Branch | Head | Head commit date |
| --- | --- | --- | --- |
| `/home/sal/goya` | `nevada/phase0` | `aa7d320f6bb86b101ec2b0dbeb8d1b110041ecab` | 2026-11-19 00:12:15 +0000 |
| `/home/sal/goya-disparates` | `main` | `64862983c276c8e537f53bd3cc34c5e4f19760ef` | 2026-11-18 23:45:21 +0000 |
| `/home/sal/goya/.doubledoor/sessions/dd_d53e7695-5f1f-49fc-bbfa-2167d7074fe9/workspace` | detached HEAD | `bc43ac4` | not inspected further |

| Fact | Observation |
| --- | --- |
| Programme location | `/home/sal/goya-disparates`, a linked git worktree of the repository at `/home/sal/goya` |
| Primary checkout | `/home/sal/goya` was on branch `nevada/phase0` at inspection time, hosting a different programme (IE contract: `crates/ie-gate`, `crates/ie-loopback`, `docs/adr/ADR-IE-001-contract-v0.1.md` are present on that tree and absent from the Disparates programme's work) |
| Separate live session | The primary checkout showed activity concurrent with this inspection: its head commit `aa7d320` ("fix: apply code changes (lib.rs, lib.rs.tmp.3688.adc51c8f8d90, +2 more)") is dated 2026-11-19 00:12:15 +0000, and `/home/sal/goya/.kaptaind/audit.jsonl` and `.kaptaind/decisions.jsonl` have same-day mtimes |
| Isolation consequence | Both trees are worktrees of one repository. Disparates commits land on `main` via `/home/sal/goya-disparates` only. The primary checkout's `nevada/phase0` branch neither receives nor interferes with Disparates commits; the two programmes cannot collide on a working index because they use separate working trees |
| Worktree dirty state | At inspection, `git status` in `/home/sal/goya-disparates` showed modified: `Cargo.toml`, `Cargo.lock`, `crates/goya-fair/src/lib.rs`, `crates/goya-fair/fixtures/example.fair.json`, `docs/contracts.md`, `docs/disparates/roadmap.md`; untracked: `crates/goya-disparate-electron/`, `crates/goya-disparate-coverage/`, `crates/goya-disparates/`, `docs/disparates/adr/`, `docs/disparates/capabilities/`, `docs/disparates/contracts/coverage-1.md`, `docs/disparates/corpus/`, `evidence/GY-DISP-001.json`, `evidence/GY-DISP-002.json`, `evidence/disparates/`. Substantial programme state is therefore uncommitted at inspection time |
| Workspace crate version | `0.1.8` in the workspace `Cargo.toml` (all four Disparates crates inherit it) |

## Corpus inventory

Foreign-application clones live at `/home/sal/corpus/`, outside the repository.
`ls /home/sal/corpus` on 2026-11-19: `fiddle`, `reactotron`. Clone heads verified
against pins on 2026-11-19 11:23 UTC:

| Clone | Pinned commit | Pin recorded in | Role |
| --- | --- | --- | --- |
| `/home/sal/corpus/reactotron` | `acdd7c73e7ecf9d8d79f6fcf9201a48aefaf270a` (HEAD verified) | `docs/disparates/roadmap.md` nomination 2026-11-18; `evidence/disparates/reactotron-phase1/run.json` | Phase 1 reference corpus: `infinitered/reactotron`, `apps/reactotron-app` slice, MIT |
| `/home/sal/corpus/fiddle` | `82fb8290c386aac5654ce5fdb4eb4a6c7faa1654` (HEAD verified), tag `v0.4.0` | `docs/disparates/corpus/electron-fiddle.md`, nomination 2026-11-19 | Phase 2 nomination: `electron/fiddle`, whole repository at `v0.4.0`, MIT |

Untrusted-source handling: the clones are untrusted third-party code and stay
outside the repository. Extraction reads source bytes only and never executes
foreign code (declared in both `run.json` extractor records and the roadmap's
non-negotiable constraints). Only derived artefacts — FAIR documents, deficit
reports, run records — are committed, under `evidence/disparates/`.

## Crate inventory

`ls /home/sal/goya-disparates/crates` on 2026-11-19 lists 66 crates; the
Disparates programme owns the four below. Test counts are from `cargo test -p
<crate> --offline`, run 2026-11-19 ~11:25 UTC; all results `ok`, nothing failed.

| Crate | Contract | Crate version | Tests passed | Role |
| --- | --- | --- | --- | --- |
| `goya-fair` | `GY-FAIR/1` (`FAIR_CONTRACT`) | 0.1.8 (workspace) | 12 (lib) + 0 (doc) | FAIR schema, validation, deterministic canonical serialisation; serde-only, depends on no other Goya crate |
| `goya-disparate-electron` | emits `GY-FAIR/1` documents; `EXTRACTOR_VERSION` 0.1.0 | 0.1.8 (workspace) | 9 (lib) + 0 (integration) + 0 (doc) | Electron/React extractor: discovery plus line-oriented static scanner, `disparate-extract` CLI |
| `goya-disparate-coverage` | `GY-COVERAGE/1` (`COVERAGE_CONTRACT`) | 0.1.8 (workspace) | 11 (lib) + 0 (integration) + 0 (doc) | Representability assessment: `CapabilityMap`, `assess`, `DeficitReport`, `disparate-assess` CLI |
| `goya-disparates` | none of its own; drives the two above | 0.1.8 (workspace) | 6 (lib) + 0 (integration) + 0 (doc) | `GY-DISP-003` pipeline driver: extraction → assessment → deficit report as `Pipeline::run` + `disparate run` CLI; present and compiling in the tree at inspection time |

Recorded at time of writing (2026-11-19 ~11:25 UTC): `goya-disparates` exists,
is workspace member line 69 of `Cargo.toml`, and has no evidence file of its own
yet (no `evidence/GY-DISP-003.json`; the glob `evidence/GY-DISP-*.json` returns
exactly two files). The roadmap also reserves the names `goya-disparate-react`,
`goya-disparate-typescript`, `goya-disparate-replay`, `goya-disparate-reconstruct`
as placeholders — no such crates existed at inspection time. Components may land
concurrently with this inspection; counts above are valid only for the stated
timestamp and head `64862983c276c8e537f53bd3cc34c5e4f19760ef` plus the uncommitted
working tree.

## Evidence inventory

`ls /home/sal/goya-disparates/evidence` (2026-11-19): the two `GY-DISP-*.json`
work-package records, `disparates/`, and pre-existing programme files from other
waves (`GY-APP-001.json`, `GY-AUDIT-DDH.json`, `demo-a/`, `goya-wave2-drive.jsonl`,
and others — not Disparates scope here).

| File | One-line observation |
| --- | --- |
| `evidence/GY-DISP-001.json` | Work package GY-DISP-001, "Phase 1 — Electron + React semantic extraction of the nominated reference corpus", wave `disparates-phase1`, dated 2026-11-18; no representability claims, no Goya generation |
| `evidence/GY-DISP-002.json` | Work package GY-DISP-002, "GY-COVERAGE/1 — representability assessment and capability-deficit report", wave `disparates-phase1`, dated 2026-11-19; assessment authority only, no reconstruction |
| `evidence/disparates/reactotron-phase1/reference.fair.json` | GY-FAIR/1 extraction of pinned Reactotron: 345 elements, 58 manifest files (per `run.json`) |
| `evidence/disparates/reactotron-phase1/reference.assessed.fair.json` | The same document after GY-COVERAGE/1 assessment, contract `GY-FAIR/1`, with per-element dispositions and `goya_evidence` facets |
| `evidence/disparates/reactotron-phase1/reference.deficit.json` | GY-COVERAGE/1 deficit report: totals 249 representable / 75 undetermined / 21 unrepresentable; largest deficit `HOOK_DATAFLOW_NOT_EXTRACTED` (63) |
| `evidence/disparates/reactotron-phase1/run.json` | Run record dated 2026-11-18: extractor `goya-disparate-electron` 0.1.0 with declared scanner limits, two-run determinism evidence (canonical JSON byte-identical), honesty notes |
| `evidence/disparates/electron-fiddle-phase2/extraction.fair.json` | GY-FAIR/1 extraction of pinned `electron/fiddle` v0.4.0: 401 elements, 254 manifest files (per `run.json`) |
| `evidence/disparates/electron-fiddle-phase2/extraction.assessed.fair.json` | The same document after GY-COVERAGE/1 assessment |
| `evidence/disparates/electron-fiddle-phase2/extraction.deficit.json` | GY-COVERAGE/1 deficit report: totals 169 representable / 156 undetermined / 76 unrepresentable; largest deficit `PERSISTENCE_CAPABILITY_UNRESOLVED` (93 `filesystem_operation` + 34 `state_store`) |
| `evidence/disparates/electron-fiddle-phase2/run.json` | Run record dated 2026-11-19: extractor 0.1.0 with an added declared limit (no scan-scope mechanism), determinism evidence (byte-identical; CLI stdout differed only in echoed output path), honesty notes on template-data inflation |
| `evidence/disparates/electron-fiddle-phase2/extract-stdout.txt` | Captured `disparate-extract` stdout for the fiddle run |
| `evidence/disparates/electron-fiddle-phase2/comparison.md` | Phase 2 vs Reactotron deficit comparison, per-kind table; notes template-data inflation of `os_api_call`/`filesystem_operation`/`window_lifecycle` |

## Phase status snapshot

From `docs/disparates/roadmap.md` (Directive GY-ENT-001, dated 2026-11-18, status
Candidate), read from the working-tree copy — which carries uncommitted
modifications — on 2026-11-19:

| Phase | Name | Status at snapshot |
| --- | --- | --- |
| 0 | Contracts | Complete |
| 1 | Semantic extraction | First milestone met (2026-11-18); representability milestone met (2026-11-19, GY-COVERAGE/1); extractor limits recorded |
| 2 | Reference corpus expansion | Not started |
| 3 | Behavioural replay | Not started |
| 4 | Native reconstruction | Blocked (authorisation withheld) |
| 5 | Windows/Linux equivalence | Not started |
| 6 | Adoption demonstrator | Not started |
| 7 | Ecosystem integration | Not started |

Phase 1's exit condition ("one bounded Electron + React reference application fully
extracted; every known construct represented or explicitly classified") is not met
in full: the roadmap itself records the milestone as met for extraction and
representability with extractor limits still open. This snapshot records the
roadmap's own words; it does not adjudicate them.

## Known debts and open questions

Each item is a recorded fact with its evidence pointer. None is resolved here.

| Debt / open question | Evidence pointer |
| --- | --- |
| Extractor scanner limits: 0.1.0 is a line-oriented lexical scanner — no AST parse, computed IPC channel names unresolved, hooks recognised as occurrence sites only, linear-in-bytes with no scope mechanism | `evidence/disparates/reactotron-phase1/run.json` and `evidence/disparates/electron-fiddle-phase2/run.json`, `extractor.declared_limits` |
| Hook data-flow deficit: `HOOK_DATAFLOW_NOT_EXTRACTED`, 63 hook elements with no data-flow, so `useState`→GY-STATE and `useEffect` scheduling cannot be assessed; classified as programme debt (assessment-method deficit), not Goya demand | `evidence/disparates/reactotron-phase1/reference.deficit.json`; `docs/disparates/adr/0001-ast-grade-extraction.md` context |
| Template scan-scope gap: the extractor cannot exclude files shipped as application data; `electron/fiddle` `static/templates/**` and `src/content/*.ts` constructs are extracted alongside app code, so the 401-element count is an upper bound on app-slice semantics | `evidence/disparates/electron-fiddle-phase2/run.json` honesty notes; `docs/disparates/corpus/electron-fiddle.md`, "Scan-scope finding recorded for the programme" |
| ADR 0001 (AST-grade extraction, oxc-based extractor 0.2.0) is a Proposal — no ratified decision, no implementation; `EXTRACTOR_VERSION` remained 0.1.0 at inspection | `docs/disparates/adr/0001-ast-grade-extraction.md`, status line; `crates/goya-disparate-electron/src/lib.rs` `EXTRACTOR_VERSION` |
| Capability decision pending — desktop OS integration: `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` (12 Reactotron elements); the analysis finds 3 of 12 (clipboard) mis-classified against existing GY-CLIPBOARD/1 and no notifications/tray usage observed; document is "Proposal (decision required)" | `docs/disparates/capabilities/desktop-os-integration.md`; `evidence/disparates/reactotron-phase1/reference.deficit.json` |
| Capability decision pending — persistence scoping: `PERSISTENCE_CAPABILITY_UNRESOLVED` mixes key-value config (3) with unstructured file access (9, of which 6 in build tooling); no filesystem or disk-persistence contract exists in the register; document is "Proposal (decision required)" | `docs/disparates/capabilities/persistence-scoping.md`; `evidence/disparates/reactotron-phase1/reference.assessed.fair.json`; `evidence/disparates/electron-fiddle-phase2/extraction.deficit.json` (fiddle top deficit, 127 elements) |
| GY-REPLAY/1 is a Draft: not ratified, no execution authorisation for foreign code under replay, so the GY-FAIR/1 `verified` state remains unreachable | `docs/disparates/contracts/replay-1-draft.md` |
| Phase 4 reconstruction authorisation withheld | `docs/disparates/roadmap.md` phase table |
| Uncommitted programme state: three Disparates crates, the coverage contract, both capability analyses, ADR 0001, the fiddle corpus nomination, and all `evidence/disparates/` artefacts are untracked in git at inspection time | `git status` in `/home/sal/goya-disparates`, 2026-11-19 11:23 UTC, listed under Repository topology |

## Non-admission

This file does not admit a release. It is an inspection record of one host at one
timestamp. No phase transition, contract ratification, capability decision,
reconstruction authorisation, or migration claim is made or implied by anything
above. Facts change; re-inspect before relying on any row here.
