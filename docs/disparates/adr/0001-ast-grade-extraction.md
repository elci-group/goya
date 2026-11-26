# ADR 0001 — AST-grade extraction for goya-disparate-electron

Status: Proposal.

## Context

The first corpus run (Reactotron, `evidence/disparates/reactotron-phase1/run.json`,
extractor `goya-disparate-electron` 0.1.0) produced 345 elements, all `extracted`.
The coverage assessment (`reference.deficit.json`, GY-COVERAGE/1) names
`HOOK_DATAFLOW_NOT_EXTRACTED` as the largest deficit: 63 hook elements recorded as
occurrence sites only, with no data-flow, so `useState` → GY-STATE mapping and
`useEffect` side-effect scheduling cannot be assessed. The cause is declared in the
extractor's identity: 0.1.0 is a line-oriented lexical scanner. It cannot be extended
into data-flow; hooks must be parsed as syntax trees. Per `fair-1.md`, a full-parser
extractor is a **new extractor version, not a silent rewrite**.

## Decision

Upgrade `goya-disparate-electron` to AST-grade extraction as extractor version
`0.2.0`, using **oxc** (`oxc_parser`, `oxc_ast`, `oxc_semantic`) as the TS/TSX parser.
The scanner remains as the fallback path for files oxc cannot parse and for CSS
(which no ECMA parser covers). The upgrade is delivered incrementally; the first
step is parse-only discovery producing an identical-or-superset element set,
verified by a differential test against 0.1.0.

### 1. Parser choice

Candidates evaluated for TS/TSX parsing in a deterministic, offline, Rust-native
extractor.

| Criterion | oxc | swc (`swc_ecma_parser`) | tree-sitter-typescript | rust-sitter / other |
|---|---|---|---|---|
| TSX parse fidelity | Full TypeScript, JSX/TSX, Stage 3 decorators; `SourceType::from_path` infers TS+JSX+ESM (verified, docs.rs 0.159.0) | Full TS/TSX behind `typescript`/`tsrx` feature flags (verified, docs.rs 47.1.0) | Grammar-level TS/TSX; lossy concrete-syntax tree, no typed AST | Weak TSX story; research-grade |
| Error recovery | Built in: `ParserReturn { program, diagnostics, fatal_error }`; partial AST returned with all diagnostics; explicit `fatal_error` flag distinguishes unrecoverable input (verified) | Partial: recovers from some error classes (e.g. enum comma), recovered errors via `take_errors()`; less systematic than oxc (verified) | Best-in-class: any-prefix recovery is tree-sitter's core design; but produces a generic tree, not a typed AST | n/a |
| Dependency weight | `oxc_parser` + `oxc_ast` + `oxc_allocator` (arena); self-contained oxc workspace crates, no C toolchain (verified crate topology; compile-cost figure **estimated**: single-digit crates, comparable to serde-scale builds) | Pulls `swc_atoms`, `swc_common`, `swc_ecma_ast` plus `num-bigint`, `phf`, `tracing`, etc. (verified dependency list, docs.rs). Heavier; more transitive surface | Requires `tree-sitter` runtime + C compilation of generated grammars via `cc`; adds a C toolchain dependency to a pure-Rust build | Varies |
| Maintenance risk | oxc-project actively maintained, frequent releases (0.159.0 as of this writing); pre-1.0 API churn is the main risk | swc-project actively maintained (47.1.0 published 2026-11-18, verified); large version numbers indicate rapid churn | Mature, slow-moving; grammar versions are stable | High risk / low trust |
| License | MIT (verified) | Apache-2.0 (verified) | MIT (typical; **not individually verified**) | — |
| Rust-native API | Yes, arena-allocated typed AST, u32 spans (4 GiB file cap) | Yes | C API bound from Rust; tree walking is manual and untyped | Yes |

**Recommendation: oxc.** It is the only candidate that combines full TSX fidelity,
systematic error recovery with a partial-AST guarantee, and a pure-Rust typed AST
without a C toolchain — all four are hard requirements here. swc is the runner-up
and is rejected on dependency weight and less systematic recovery. tree-sitter is
rejected because its untyped, lossy tree pushes interpretation complexity back into
the extractor and its C build breaks the pure-Rust property of `goya-fair`
consumers. The pre-1.0 version churn of oxc is accepted: the extractor pins the
version in `Cargo.lock`, and FAIR's content-addressing means any parser-driven
output change is detectable by re-run.

### 2. Version bump semantics per contract

- `EXTRACTOR_VERSION` moves `0.1.0` → `0.2.0`; `EXTRACTOR_ID_RULE` is restated to
  describe AST-walk ordering (below). Both are recorded in every document, so a
  document's provenance always names the code that produced it.
- 0.2.0 ships with a new `declared_limits` block (hook data-flow resolution depth,
  computed-channel resolution rules, scope-bound `require` depth — see below).
  Declared limits are part of extractor identity, not documentation.
- Re-run and re-assessment procedure: extract the pinned corpus (`reactotron`,
  `acdd7c73…`) under 0.2.0; validate under GY-FAIR/1; produce a fresh evidence
  directory (`reactotron-phase2/`); re-run coverage assessment against the new
  document. The phase-1 deficit register is never mutated.
- Old documents remain valid. GY-FAIR/1 documents are content-addressed by the
  SHA-256 source manifest and carry the extractor identity of their own run: a
  phase-1 document stays a truthful record of what extractor 0.1.0 understood
  about those bytes. Nothing is rewritten or invalidated; comparisons across
  extractor versions are made by diffing evidence runs, not by migrating
  documents. FAIR's ignorance-preservation invariant makes this honest: 0.1.0's
  `undetermined` populations remain the record of its limits.

### 3. Hook data-flow extraction design (proposed, not implemented)

At AST level, a hook call is a `CallExpression` whose callee is an identifier
matching `use[A-Z]`, occurring inside a component/function body. FAIR elements
gain semantics facets:

- `hook` (existing): hook name.
- `state-key`: for `useState`, the bound variable name from the destructuring
  pattern (`const [open, setOpen] = useState(false)` → `open`), plus optional
  `state-initial` for a statically-literal initialiser. These are the anchors a
  future GY-STATE mapping needs.
- `deps`: for `useEffect`/`useLayoutEffect`, the serialised dependency-array
  contents (list of source-text expressions), or `absent`/`null-literal` when the
  array is omitted or `[]`.
- `effect-class`: coarse static classification of the effect body:
  `subscription` (event/listener registration), `ipc-invoke` (calls into
  `ipcRenderer`), `dom-mutation`, `timer`, `fetch`, `unclassifiable`. Classification
  is syntactic (member-expression prefixes and callee names), never heuristic or
  execution-based.
- `cleanup`: boolean — the effect returns a function.
- `setter-calls`: for `useState` setters, the sites where the setter is invoked,
  where resolvable within the file (bounded walk), enabling state→side-effect
  linkage.

New reason codes for the coverage register (proposed; the register grows by
contract revision, existing codes are untouched):

- `HOOK_DATAFLOW_PARTIAL` — hook parsed, but facets incomplete (e.g. state key
  bound to a tuple index rather than a name).
- `HOOK_DATAFLOW_UNRESOLVED` — hook call recognised but its enclosing scope is
  outside a parseable unit (recovered-parse fragment).
- `EFFECT_BODY_UNCLASSIFIED` — effect body parsed but matches no effect class;
  recorded rather than guessed (ignorance preserved).
- `COMPUTED_CHANNEL_UNRESOLVED`, `REQUIRE_SCOPE_UNRESOLVED`,
  `REQUIRE_DEPTH_EXCEEDED` — see §4.

### 4. Computed channels and scope-aware `require` resolution

- **Computed channel names.** The scanner 0.1.0 records only string-literal
  channels. 0.2.0 resolves `ipcMain.handle(obj.key)` and template-literal channels
  by bounded constant lookup: if the argument is a member expression whose object
  is a local `const` object literal or a `const` string variable declared in the
  same file, substitute the literal. Templates with all-literal substitutions are
  resolved; anything else is recorded as
  `resolution: unresolved`, representability `undetermined`, reason
  `COMPUTED_CHANNEL_UNRESOLVED` — matching the existing `DYNAMIC_REQUIRE_UNRESOLVED`
  discipline. No cross-file resolution is attempted in 0.2.0 (declared limit).
- **Scope-aware `require`.** Static string-literal `require("x")` resolves to the
  module specifier (existing behaviour, now precise rather than lexical). For
  aliased requires (`const cp = require("child_process")`), the binding is recorded
  in a per-file scope map (oxc's semantic layer provides scope/symbol tables) and
  uses of the alias within the file are attributed to the resolved module, with
  the resolution noted in semantics (`resolution: scoped-alias`). The walk is
  bounded: at most one indirection, same file only. Non-literal or cross-file
  cases remain `undetermined` (`REQUIRE_SCOPE_UNRESOLVED`); exceeding the bound
  yields `REQUIRE_DEPTH_EXCEEDED` rather than a guess.

### 5. Risks

- **Determinism.** Canonical FAIR output is byte-stable; the AST walk must be
  order-stable too. Rule: elements are emitted by a deterministic traversal
  (depth-first, source order); where any map or set is consulted (scope tables,
  symbol tables), iteration is over a sorted key list, never hash order. IDs are
  assigned in (sorted relative path, source-order position) as in 0.1.0, restated
  in `EXTRACTOR_ID_RULE`. The two-run byte-identical determinism test is retained
  and extended to the fixture corpus. Parser-upgrade-driven output changes are
  legitimate only as a further version bump.
- **Performance bound.** Extraction remains linear-in-bytes with a hard cap per
  file (proposed: skip AST parsing above a declared byte limit, emit
  `FILE_TOO_LARGE_FOR_AST`-scoped elements via the scanner path, marked
  `fallback_origin`); oxc's u32 spans impose a 4 GiB ceiling (verified, docs.rs).
  No asymptotic blow-up is permitted: constant-propagation walks are depth-bounded
  and file-local.
- **Foreign code is never executed.** Constant propagation is syntactic
  substitution of literals from the AST; no evaluation, no interpretation, no
  build tooling is invoked, no `node_modules` is read beyond manifest dependency
  names. This invariant is unchanged from `fair-1.md` and is restated in the
  0.2.0 declared limits.
- **Error recovery honesty.** A recovered parse must never silently shrink the
  element set. Files with parse diagnostics still contribute scanner-path elements
  for the recovered region, plus an `undetermined` marker for unparseable regions.

### 6. Incremental delivery plan

One PR-sized step first:

**Step 1 — parse-only discovery, differential-tested.** Add oxc; parse every
TS/TSX/JS/JSX file; emit the *same* element kinds and facets 0.1.0 emits, from the
AST. Add a differential test: for the fixture app (and, in CI or locally, the
pinned Reactotron slice), assert the 0.2.0 parse-only element set is identical or
a superset of the 0.1.0 scanner set, matched on (kind, file, line). CSS and
native-module provenance keep the existing paths. Ship as `0.2.0-alpha.1` without
changing `EXTRACTOR_VERSION` of record until the differential holds on the corpus.

Subsequent steps, each separately reviewed:

2. Precise provenance: fill `span_start`/`span_end` (oxc spans), set
   `fallback_origin` correctly; restate `EXTRACTOR_ID_RULE`.
3. Hook data-flow facets and new reason codes (§3), with a contract-revision note
   for the register growth.
4. Computed channels and scope-aware requires (§4).
5. Evidence re-run: `reactotron-phase2`, fresh deficit register, superseding —
   not editing — phase 1.

## Consequences

The hook deficit (`HOOK_DATAFLOW_NOT_EXTRACTED`, 63 elements) becomes assessable:
`useState` state keys map against GY-STATE, and effect bodies carry a declared,
syntactic side-effect classification. Computed IPC channels and aliased requires
gain bounded resolution, with the residue preserved as `undetermined` under new
register codes, never dropped. The dependency graph of `goya-disparate-electron`
grows by the oxc crate family (pure Rust, MIT, no C toolchain); `goya-fair` itself
stays serde-only and untouched. Extraction output changes, so every future corpus
run names extractor 0.2.0 and its own declared limits; phase-1 documents remain
valid records of the phase-1 run. The determinism, offline, and never-execute
invariants carry over unchanged and are re-evidenced by the two-run byte-identity
test on every step that touches extraction output.
