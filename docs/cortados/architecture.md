# Cortados Phase 0 decisions and interface contract

Contract family: `GY-CORTADOS/0.1`. Directive: GOYA-DIS-CORT-001.
Decision authority: the owner's 26 November 2026 instruction to use a Luna swarm
to close Phase 0. These decisions govern Cortados design; they do not amend shared
FAIR, GSSH, compiler or runtime contracts and do not certify a converted application.

## Phase boundary

Phase 0 establishes what the existing Goya implementation can express, which
components Cortados reuses, and how unavailable semantics are rejected or referred
to later governed extensions. The directive's minimum viable Cortado is a later
operational milestone. Requiring that entire milestone before architectural closure
would conflate Phase 0 with implementation phases. Production gates COR-G01–G14
remain unassessed until Cortados code and corresponding evidence exist.

## Decision register

| ID | Decision | Closure evidence |
| --- | --- | --- |
| COR-D01 | Keep FAIR/2 unchanged; CSIR is an independently versioned sibling artifact bound to its source manifest and optional FAIR document. | CSIR schema, hand-authored round-trip fixture and reference/digest/span rejection checks. |
| COR-D02 | Use the Java 17 non-preview, UTF-8, offline source-analysis profile in `profile.json`. Analysis eligibility never implies native representability. | All 34 initial requirements have explicit native dispositions and diagnostic reasons. |
| COR-D03 | Emit only existing GSSH/4-live semantics supported by the selected destination. Reject general Java behavior outside proved scalar mappings. No Java-specific additions to `goya-ir`. | Requirement matrix, arithmetic restrictions and real resolver witness. |
| COR-D04 | Place the first analysis library and `cortados` CLI in a future `crates/goya-cortados` root-workspace member. Split crates only when needed. | Input/output/API boundaries below. No placeholder workspace or crate is introduced. |
| COR-D05 | Static analysis never executes input or retrieves dependencies. Reference execution is a separate authorized harness. | Exact budgets and capability/isolation rules below; hostile-input acceptance estate specified for implementation. |
| COR-D06 | Use a hand-paired safe-range Java counter and GSSH/4-live resolver fixture to establish destination feasibility. Reserve automated transformation, differential Java execution and the substantive application milestone for later gates. | Machine-recorded resolver observations. Java execution is explicitly skipped when the JDK is absent. |
| COR-D07 | Treat C0–C5 as cumulative, scoped evidence admission. No level follows from confidence, syntax validity or this architectural closure. | Certificate boundary below; Phase 0 report requires null application certification. |

The initial audit's recommendations are resolved by this register. Rejected
features remain gaps in native execution, with their original closure conditions
preserved for future work. Recording a rejection closes an architectural decision;
it does not implement the missing semantic feature.

## Source analysis and native profile

The source inventory targets Java SE 17 source grammar without preview features.
This is a parser coverage requirement for the next phase, not an assertion that a
Java parser exists. Decode only UTF-8; retain exact original bytes, comments and
byte ranges. Apply Java Unicode escape translation with an offset map back to
those bytes before tokenization. Record unresolved names, module boundaries,
build-derived sources and dependency versions rather than guessing them. Parser
recovery must retain opaque spans and diagnostics. A parse failure prevents a
complete-analysis claim. Parser selection remains a Phase 1 implementation choice
constrained by this profile, source-span fidelity and the conformance corpus.

The native feasibility subset has session scalar state, explicit input bindings,
straight-line assignments and addition whose range is proved not to overflow the
source width over the complete declared input/state scope. D1 reads the original
state snapshot and permits one write per key: Java sequential read-after-write
dependencies require proof-preserving substitution into parallel updates or
conversion refusal. Numeric input must have the same canonical decimal/source-width
domain as the source, or an explicitly narrowed verification scope: Java-valid
`+1`, `01` and `-0` cannot silently become rejected Goya interactions. Identity-bearing objects, general calls, collections, exceptions,
threads and effects are rejected until their listed contracts exist. Constants or
object abstractions may be eliminated only with recorded proof covering every
observable behavior; recognizing a class or method name is insufficient.

The resolver witness runs from a clean detached worktree at baseline
`b408ee19a15037f440db49f1cabd5ad93f660452`, independent of concurrent root edits.
Archived audit sources distinguish committed baseline code from observed external
working-tree artifacts. Native graphics is rejected because its initially observed
integration is absent from that baseline and later disappeared from the root.

The Java oracle intentionally uses allocation, method calls, an array, a loop
and console output solely to calculate the reference sequence. Those constructs
are not evidence of native mappings, and the Java program was not executed.

The witness has Java `int` count initially zero and exactly the input sequence
`1,0,2`; its reachable count set is `{0,1,3}`. The Goya scene's broader `i64`
input domain is not a claim of Java equivalence outside that declared scope.
Any future generally available input needs a proved int32 guard or correct
fixed-width operations. In particular, `2147483647+1` is a semantic rejection
counterexample even though checked `i64` addition itself accepts the value.

## CSIR source and evidence boundary

`GY-CSIR-JAVA/0.1` is a design contract defined by `csir.schema.json`.
It is not an executable format or an alternative Goya language. A document carries:

- Profile ID, frontend identity/version and construction mode (`manual_fixture`
  or `static_analysis`), and SHA-256-bound source inventory.
- A nullable FAIR attachment naming the canonical FAIR/2 artifact and its SHA-256.
  A detached architecture fixture has no FAIR attachment and no FAIR element IDs.
  Production analysis requires an attachment validated by `goya-fair` itself.
- Typed nodes with stable IDs, semantic domain, representation level
  (`implementation` or `application`), exact origin, payload and optional FAIR ID.
  Opaque nodes carry their original-byte digest and unresolved reason. Missing
  semantics must never become an empty implementation or omitted node.
- Typed edges with resolvable endpoints and explicit evidence. Application-level
  intent requires evidence links; it never overwrites implementation nodes.
- Transformation contracts with preconditions, postconditions, observable effects,
  equivalence obligations, limitations and evidence references. Empty evidence
  means unverified; no confidence value can promote it.

Canonical CSIR serialization is UTF-8 JSON with sorted object keys, two-space
indentation, LF and a final LF. Source entries sort by path; nodes/edges/passes
sort by ID; evidence lists sort and contain no duplicates. Hashes are lowercase
SHA-256 of exact bytes. The source-manifest digest hashes the canonical JSON
array of `{path,sha256}` records. This is Cortados canonicalization, not a rewrite
of FAIR's separate fixed-field-order canonical bytes.

Production checks must validate both documents, hash the exact FAIR canonical
bytes, match source inventories, resolve FAIR IDs, verify UTF-8 byte boundaries and
line provenance, preserve unresolved ranges, reject dangling evidence/cycles in
proof dependencies, and reject unknown schema versions. The Phase 0 validator
checks the detached fixture's structure, spans, hashes, reference integrity and
negative cases. It does not implement a Java parser or all future semantic proofs.

## API and bundle boundary

The future Rust library exposes `analyse(input, profile, limits, policy)` returning
an `AnalysisBundle` or structured fatal diagnostics. CLI `cortados analyse INPUT
--output OUTPUT --profile PROFILE` calls the same implementation. No network or
execution permission is implicit in `analyse`. No `convert`/`verify` success path
exists before those features are implemented and tested.

The following bundle, diagnostic, dependency and certification names are Phase 0
interface requirements with reserved contract identifiers. Concrete normative
serialization schemas and their independent admission validators are later
implementation deliverables; only the CSIR schema and detached-fixture verifier
are supplied here.

An analysis bundle contains `bundle.json`, `application.fair.json`,
`application.csir.json`, `diagnostics.json`, `dependencies.json` and
`capabilities.json`. Bundle interface `GY-CORTADOS-BUNDLE/0.1` records artifact path,
kind, contract and SHA-256, source/profile identity, producer/pass versions and
analysis completeness. It uses paths relative to its root, never absolute paths
or parent traversal. Do not copy arbitrary input resources into executable paths.
Assets/licenses remain traceable to source inventory and original notices.

Diagnostic interface `GY-CORTADOS-DIAGNOSTICS/0.1` has code, severity, source path,
optional exact byte span, subject IDs, requirement, attempted pass, failure and
remediation. COR-E100/110/120 cover parsing/type/dependency failures; COR-E200/210/220
cover unsupported semantics/dynamics/JNI; COR-E300/310/320 cover UI/layout/graphics;
COR-E400/410 cover equivalence/concurrency; COR-E500/600/700 cover security,
destination conflict and certification. Codes remain separate from FAIR's closed
reason register. Where FAIR has no specific kind, use `unknown` with observed
spelling and registered `NO_FAIR_KIND`; preserve detailed Cortados diagnostics
in the sibling artifact rather than changing FAIR vocabulary.

Dependency ledger interface `GY-CORTADOS-DEPENDENCIES/0.1` records identity/version,
source/artifact hash, license plus attribution, reachable members, reachability
assumptions, unknown reachability, attempted strategies in preference order,
selected strategy, verification evidence, residual requirements and authority.
Strategies are native equivalent, recursive transpilation, compatibility lowering,
explicit external boundary, or conversion failure. Unknown versions/licenses stay
unknown and block claims that require complete provenance. No JSER replacement is
admitted without boundary, exception, side-effect and applicable concurrency tests.

Conversion packaging later adds admitted `application.gscene`, resources and
verification/certification manifests. It must adapt to the chosen compiler's
existing manifest, not impose the directive's tentative filenames on compilers.
There is no Java bytecode interpreter, embedded JVM, unapproved `logic/` language
or hidden generated host implementation in this design.

## Ingestion and reference execution security

The initial limits are part of the profile, not claims of implemented enforcement:
20,000 entries, 32 MiB per source/resource, 512 MiB total material, path depth 64,
1 MiB per path, 8 million tokens per file, AST depth 256, and 60 seconds per file.
Budget exhaustion stops complete analysis and emits COR-E500 with the exhausted
limit. Use process-level CPU/RAM enforcement in implementation; a token/depth
counter does not enforce wall time or memory on its own.

Directories reject traversal, absolute names, NUL, conflicting normalized names
and filesystem races; do not follow symlinks or special files. Excluded material
is inventoried with a reason. Archives are deferred from the first directory
profile. Their admission requires 256 MiB compressed input, 512 MiB expanded total,
32 MiB per entry, 20,000 entries, compression ratio at most 100:1 and zero nested
archive expansion by default; reject encrypted/traversing/duplicate entries and
decompression overflow. These limits never authorize downloading absent packages.

Static ingestion never runs Gradle/Maven, annotation processors, class loading,
initializers or application code. Build files are metadata with computed values
explicitly unresolved. Reference execution needs an explicit fixture grant and
disposable OS isolation, read-only fixture input, no network or inherited
credentials, and CPU/wall/RAM/output/file caps. The authored Phase 0 Goya witness
is trusted repository fixture execution, not execution of untrusted Java input.
No reference JVM was available here; this is disclosed, never replaced by a fake
differential pass.

## Certification and target eligibility

Certificate interface `GY-CORTADOS-CERTIFICATION/0.1` binds source/dependency/resource
hashes, profile, configuration, pass versions, execution scope, target/configuration
and evidence artifacts. Admission is cumulative: C0 requires actual Java ingestion;
C1 adds emitted source and strict shared-handoff admission; C2 adds scoped original
versus converted behavior; C3 adds absence of JVM execution and declared residual
review; C4 adds two or more named target/configuration conformance results; C5 adds
raw reproducible optimization measurements and disclosed regressions. A missing,
stale, mismatched or failed prerequisite blocks promotion. Scores never substitute
for evidence. The architecture witness has no application certification.

Capabilities describe required operations and scope, not broad host permissions.
Each target is eligible only after its compiler/resolver admits the selected
GSSH profile and every capability with equivalent authority. D2 journal replay
does not establish real file/network I/O. Finite GVI representations require a
proved bounded reachable inventory. Graphics codecs, Caprichios, platform input,
state continuity and native effect ownership remain in their existing subsystems.

## Deferred implementation gates

Phase 1 must deliver the Java parser/profile conformance corpus, source offset
mapping, bounded ingestion and truthful FAIR/CSIR analysis. Later native work must
resolve shared-language gaps through the normal architecture process and obtain
the corresponding Disparates reconstruction authority. The owner request to close
this architectural phase does not alter the shared roadmap's reconstruction gate.
The minimum viable application, Java differential execution, real effects,
cross-platform conformance and fair performance benchmarks remain explicit later
deliverables; closing Phase 0 claims none of them.
