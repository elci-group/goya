# DELECTABLE-0 — Phase 0: architecture reconciliation

Status: **Phase 0 evidence, not a gate pass.** Date: 26 November 2026. Written in a
`nebula/v0.1` checkout and not committed. Directive: *Delectable — Electron-to-Goya
Semantic Transpilation*, DEL-TD-001 v1.0.0 (pasted by the owner, same day).

The directive's closing note says its subsystem names, the DIR and DEG schemas, the
crate boundaries, the three profiles, the native-first policy and the performance
targets are its own proposals, not established contracts. It says implementation
starts with an architectural audit. This file is that audit. No code was written.

## 1. Headline

* **Delectable already has a parent programme with signed-off rules.** The Disparates
  programme (GY-ENT-001, 18 November 2026) and its contract GY-FAIR/1 (GY-FAIR/2 since 26 November)
  (`crates/goya-fair`) existed on `main` and `arachnid/v0.1` only. They were
  brought onto `nebula/v0.1` on 26 November (decision 2). Disparates Phase 1 is already "one bounded Electron + React
  reference application fully extracted". Its roadmap reserves the crate name
  `goya-disparate-electron`. Delectable is that crate's directive, not a new
  subsystem. **The owner confirmed this on 26 November** (§6, decision 1).
* **FAIR already covers most of the DEG and DIR.** FAIR has SHA-256 source manifests,
  per-element provenance with no invented spans, canonical byte-stable JSON, an
  "unknown" kind, and its own rule that FAIR keeps what it does not understand. The
  directive's §8 and §14 would set up a second foreign-application IR beside it.
  §14.3 forbids DIR from competing with `.gscene`, and the same reasoning applies
  to FAIR.
* **GY-ENT-001 withholds reconstruction.** Disparates Phase 4, "Native
  reconstruction", is *Blocked (authorisation withheld)*. Until the owner lifts it,
  Delectable's Phase 6 (generate `.gscene`) and gate DEL-G06 are blocked.
* **`.gscene` can express very little application logic today.** D1 has exactly
  five instructions: `Constant`, `ReadState`, `ReadInput`, `AddInt` and
  `WriteState` (`crates/goya-executable/src/d1_vm.rs`). There are no branches,
  comparisons, multiplication, floats or collections, and templates are rejected
  (`GY-E1002`). D2, which covers effects, is refused by the resolver (ADR Decision
  4). Of the ten benchmark tiers in §27, **only DEL-B01 (Counter) can be expressed**
  (§3). Even Goya's own calculator keeps its logic in Rust host code
  (`crates/goya-hello/src/bin/calculator.rs`).

## 2. Reconciliation: directive names against the repository

| Directive name | In this repository | Consequence |
| --- | --- | --- |
| Disparates (parent) | GY-ENT-001, `arachnid/docs/disparates/roadmap.md`. Candidate. Phases 0–7. Only `goya-fair` is implemented. | Delectable is the Electron extractor inside it. GY-ENT-001's constraints apply: static offline extraction, no LLM in extraction, no `goya-ir` contamination, reconstruction withheld. |
| DEG, the evidence graph (§8) | GY-FAIR/1 `FairElement`: kind, provenance, representability, open semantics payload. **FAIR has elements but no typed edges.** | **Done:** GY-FAIR/2 (approved and implemented 26 November) adds relationships (subject, relation, object, basis, evidence). No second graph format. |
| DEG evidence classes (§8.3, eight states) | FAIR has seven frozen representability states (`extracted` … `undetermined`). | These are different axes. DEG classes say how certain an assertion is. FAIR states say how far conversion has progressed. A FAIR/2 assertion needs its own certainty field. FAIR's states stay frozen. |
| DIR (§14) | No equivalent. FAIR is the input side, and "the FAIR→Goya boundary is a future contract". | DIR *is* that future boundary contract. It should be specified as the FAIR→`.gscene` mapping, not as a standalone IR with its own ontology and presentation domains. |
| "Ontology" (§12) | In Goya, `@ontology` is the **scene root** of a `.gscene` file. Goya has no construct for entities, relationships or invariants. | There is a name collision, and §12's output has nowhere to go in `.gscene`. It can only appear in `ontology.json` as a report. The proposed name is "domain model" to avoid confusion with `@ontology`. |
| BrowserWindow → Canvas (§15) | No `Canvas` element. The strict elements are `@cell`, `@button`, `@input`, `@iden`, `@state`, `@interaction` and `@persistence` under one `@ontology`. | A window maps to one scene. No multi-window construct was found. |
| Notification → Card | No `Card` element. | Capability gap. |
| HTML/JSX hierarchy → scene tree | `@cell` / `@button` / `@input` nesting. | Representable for static trees. Lists rendered from data (`.map()`) are not representable because templates are rejected. |
| CSS layout → proportional geometry (§15.1) | GSSH/3 D7 makes every length proportional. D1 (layout) rules out anchors, margins, padding, min/max, intrinsic sizing and responsive rules. | Flexbox and grid intent **cannot be kept**, only their computed result at one reference viewport. §15.1 asks for that loss to be reported, and it will happen on every non-trivial layout. |
| DOM interaction → gestures | `button.<name>.activate` triggers, plus `gestures { copy; paste }` policy. | Only click-to-activate and clipboard policy are available. Hover, drag and focus events are not. The `focus`, `blur`, `select` and `submit` gesture keys were removed (D6). |
| Keyboard shortcuts | `shortcut` is a **reserved** `@iden` key, refused by strict admission (`GY-E1017`). | Capability gap until a profile defines it. |
| IPC → capability contract; Node fs → persistence | Durable `@state` (D4, D8, D11–D13) holds typed scalars per (ontology, key). There is no file, path or dialog capability. D2 is refused. | `localStorage` with scalar values is representable. Filesystem, network and child processes are not. |
| Application termination → Adios | No code or specification (as VENTADA-0 also found). | Gap. |
| Caprichios, GVI / GVI-A / GVI-L, romántico | All exist: `goya-caprichios`, `goya-gvi`, `goya-romantico`. | §16 can reuse these, with no new formats. |
| Inquisition (§20.3), Tauromaquia (§21) | Neither exists (`docs/arachnid/roadmap.md`). | §20.3's independent validation has nothing to validate against. Execution isolation for §13 would be local to Delectable. |
| Pelele | **Not mentioned anywhere.** | The §34 comparison has nothing to refer to. |
| nino | `crates/nino` (untracked on `nebula/v0.1`). It is at N0, has done its own Phase 0 reconciliation, and emits no `.gscene`. | This is the precedent: nino asked for a core capability rather than emit a `.gscene` wrapper that would misstate what the program does. Delectable should do the same. |
| Electron comparison (§26) | `goya-parity` row `electron_comparison` is `NotCompared`. GY-EMP-001 §11 defines a benchmark protocol, unmet (EMP-M1-14). | Reuse that protocol rather than define a second one. |

### Rules that conflict

| Directive | GY-ENT-001 / FAIR | Needs |
| --- | --- | --- |
| §13: run the source application under instrumentation | "No execution of untrusted foreign application code during static extraction". Phase 3 behavioural replay is allowed. | No conflict if runtime evidence is its own stage (`goya-disparate-replay`) and never feeds static FAIR. It still needs an isolation design, because Tauromaquia does not exist. |
| §19: bounded AI inference produces `Inferred` assertions | "No LLM inference participates in extraction, validation or serialisation". LLMs only in remediation proposals. | **Owner decision.** Either keep inferred assertions out of FAIR (in a separate hypothesis store), or amend GY-ENT-001. |
| §18: a "Partial" conversion still emits valid `.gscene` | GY-ENT-001: no generation until representability is shown on a corpus. | Follows from lifting reconstruction authority. |
| §22: 24 crates in a separate `delectable/` workspace | GY-ENT-001 reserves `goya-disparate-*` names in this workspace. | See below. |

### Crate layout (§22)

| Proposed | Reconciled to |
| --- | --- |
| core, evidence, dir, report | `goya-fair` (existing) plus the FAIR/2 revision |
| ingest, asar, electron, javascript, typescript, html, css, frameworks, graphs | **`goya-disparate-electron`**, with Oxc for JavaScript and two Goya crates, `goya-html-tokenizer` and `goya-css-syntax`, for HTML and CSS (owner decision, 26 November). |
| runtime, security (sandboxing) | `goya-disparate-replay`, Disparates Phase 3 |
| functional, logical, semantic, ontology, goya | `goya-disparate-reconstruct`, blocked |
| vectoriser | Reuse `goya-gvi`, `goya-gva` and `goya-scene-graphics` from inside reconstruct |
| equivalence, inference, cli | Created only when a phase needs them. The CLI is a binary in the extractor crate. |

## 3. Expressiveness audit (§33 risk 1)

The question for each benchmark tier is whether its core logic can be written in
strict `.gscene` (GSSH/4-live, the latest profile with live bindings) without host
code. This is about the language, not about any converter.

| Tier | Needs | Representable | First blocker |
| --- | --- | --- | --- |
| DEL-B01 Counter | int state, add, display binding, durable state | **Yes.** `crates/sash/fixtures/interactive-live.gscene` is this application. | Decrement or reset works only with a negative or constant literal. Not checked against the parser. |
| DEL-B02 Calculator | ×, ÷, conditionals, decimals | No | D1 has only `AddInt`. No comparison, branching or floats. |
| DEL-B03 Task manager | collections, data-driven lists | No | Templates rejected (`GY-E1002`). State is scalars only. |
| DEL-B04 Text editor | file open and save, dialogs | No | No filesystem capability. D2 refused. Text state is capped at 64 KiB. |
| DEL-B05 File explorer | directory listing, authority | No | As B03 and B04. |
| DEL-B06 Media | streaming, decoding | No | No media element or capability. |
| DEL-B07 Markdown editor | rich text rendering | No | No rich text model. |
| DEL-B08 Networked | network requests | No | D2 refused. |
| DEL-B09 Multi-window | several windows, shared state | No | One scene per application. No window construct found. |
| DEL-B10 Developer tooling | all of the above | No | — |

Reading: a Delectable that only extracts can be built now and is useful, because
Disparates expects unsuccessful migrations to produce "capability deficit"
evidence. A Delectable that generates code would, today, produce `.gscene` only
for counter-shaped applications. Everything else would be a capability-gap
report, and that is the correct output under §15 and §31. The shortest route to
DEL-B02 is a **D1 revision** (comparison, a conditional write or select,
subtraction and multiplication, perhaps fixed-point decimals). That is a GSSH and
GY-D1-VM contract change, not Delectable work.

## 4. Gate status

| Gate | Status |
| --- | --- |
| DEL-G00 Baseline and architecture audit | **FAIL (incomplete).** The audit is this file. One fixture exists (DEL-B01, own licence) but has not been run under Electron. There is no benchmark protocol binding yet. |
| DEL-G01 Deterministic material extraction | **FAIL (partial).** Met for directories and `app.asar`: an in-memory read, entry-name and bounds checks, integrity verification, budgets, a SHA-256 manifest, a byte-stable golden file, and directory–ASAR equivalence (15 tests). Not met: dependency classification (`node_modules` is recorded but not inventoried), the P1 distributions (Windows, Linux, macOS and AppImage), source maps. |
| DEL-G02 Structural and IPC reconstruction | **FAIL (partial).** Process topology (main, preload, renderer) and both sides of every literal IPC channel are reconstructed for DEL-B01. There are no control-flow, data-flow or call graphs, and no scope analysis. Since FAIR/2, the joins that are found are first-class relationships: 30 on DEL-B01, from the click handler to `count`. |
| DEL-G03 … DEL-G05 | NOT ASSESSED. Nothing implemented. |
| DEL-G06 Native `.gscene` conversion | **BLOCKED.** GY-ENT-001 withholds reconstruction, and the language cannot express tiers B02–B10 (§3). |
| DEL-G07 … DEL-G09 | NOT ASSESSED |

## 5. Proposed next steps (none started)

1. ~~Bring Disparates onto the working branch~~ **Done** (uncommitted):
   `crates/goya-fair` and `docs/disparates` were copied from `main` unchanged,
   the crate was added to the workspace, and its contract entry was added to
   `docs/contracts.md` (now GY-FAIR/2). Its 12 tests pass on `nebula/v0.1`.
2. ~~Author fixture DEL-B01~~ **Written** (uncommitted):
   `fixtures/disparates/electron-counter/`. It is a counter with
   `contextIsolation`, `sandbox`, a preload bridge, two IPC channels and session
   state in the main process. It matches `interactive-live.gscene`, which uses
   `session`, not `durable`, so it does not use `localStorage`. It is under the
   workspace licence. Its README gives the expected extraction (the v0 oracle)
   and three known differences in the pair. The main-process logic passes 11
   Node cases. **Electron itself has not been run** because it is not installed
   here.
3. ~~FAIR/2~~ **Approved by the owner and implemented** (26 November,
   uncommitted): [`fair-2.md`](../disparates/contracts/fair-2.md) and
   [`reason-codes.md`](../disparates/contracts/reason-codes.md). It covers
   relationships with a basis and evidence, four kinds, a project record, and
   a closed register. FAIR/1 documents still validate and keep their bytes.
   `goya-fair` has 25 tests. The blast radius and how each part was adapted
   are in `evidence/GY-FAIR-002.json`.
4. ~~`goya-disparate-electron` v0~~ **Written** (uncommitted):
   `crates/goya-disparate-electron`, now v0.2.0 and emitting FAIR/2. It uses Oxc 0.146 for
   JavaScript and Goya's own `goya-html-tokenizer` and `goya-css-syntax`, which
   replaced html5ever and cssparser on 26 November (owner decision). It adds
   48 packages to `Cargo.lock`, all from Oxc. It meets the
   README oracle for DEL-B01, and clippy `-D warnings` is clean. Its README
   lists what it records as undetermined and its known limits. Next: an
   Electron + React fixture (the Disparates Phase 1 exit condition) and
   source maps.
   **Forward planning (owner decision, 26 November 2026):** retain Oxc for
   now; replacing it with a Goya-owned JavaScript/JSX/TypeScript implementation
   is high priority. Scope, conformance and FAIR parity gates are recorded in
   the [Disparates roadmap](../disparates/roadmap.md#forward-planning-replace-oxc-high-priority).
   Replacement implementation has not started.
5. **Benchmark binding:** adopt GY-EMP-001 §11 for §26 and move the `goya-parity`
   `electron_comparison` row only with measured data.

## 6. Decisions for the owner

1. ~~Parentage~~ **Decided** (owner, 26 November 2026): Delectable is the
   `goya-disparate-electron` part of GY-ENT-001. Its DEG and DIR are folded
   into FAIR/2. It is not a separate subsystem and has no IR of its own.
2. ~~Branch~~ **Decided** (owner, 26 November 2026): `nebula/v0.1`, with
   `goya-fair` and `docs/disparates` brought across.
3. **Inference:** §19 allows bounded LLM inference to produce assertions, and
   GY-ENT-001 forbids LLMs in extraction. Which rule wins, or should inferred
   hypotheses live outside FAIR?
4. **Reconstruction authority:** does this directive lift the GY-ENT-001 Phase 4
   block, or does it stay blocked until a representability corpus exists?
5. **Language scope:** will D1 gain comparison, branching and multiplication
   (§3)? Without them, Delectable can only regenerate counter-shaped applications.
6. **Pelele, Inquisition, Tauromaquia, Adios:** none is specified here. Where are
   they?
