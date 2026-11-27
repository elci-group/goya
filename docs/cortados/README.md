# Cortados Phase 0 architectural reconciliation

Directive: **GOYA-DIS-CORT-001 v1.0**, supplied 26 November 2026.
Status: **Phase 0 closed for architectural reconciliation.** Cortados implementation
has not started. The owner's instruction to close Phase 0 was reviewed by three
Luna workers. The [decision contract](architecture.md), [compatibility profile](profile.json),
[closure record](phase0-closure.json) and [live feasibility evidence](feasibility-results.json)
resolve the architectural findings below. Shared FAIR/GSSH contracts are unchanged.

Closure identifies missing native semantics and rejects them explicitly in the
initial profile. It does not claim those semantics are implemented. All production
release gates remain unassessed, and no converted application is certified.

Goya provides reusable scene admission, evidence recording, deterministic scalar
execution and graphics infrastructure. It does not yet provide an established
`.gscene` execution contract for the directive's minimum viable Java application.
General object semantics, collections, exception flow, file operations and Java
concurrency need representability work before native conversion can be certified.

This audit covers the root workspace, not an exhaustive review of every target
compiler or the separate `arachnid` checkout. The base revision is
`b408ee19a15037f440db49f1cabd5ad93f660452`. [phase0-audit.json](phase0-audit.json)
binds 21 archived source files under `evidence/`, distinguishing committed baseline
bytes from retained working-tree artifacts. Two graphics artifacts disappeared
during concurrent edits: their earlier observations are retained as unavailable,
and the initial native profile rejects graphics rather than relying on them.

During review the shared checkout temporarily moved to `238825a8` on
`iberia/contract-v0.1`, with a conflicted workspace manifest and missing resolver
sources, then returned to the baseline revision on `Casa`. Source hashes were
captured after that restoration. During closure, further concurrent edits made
the shared build inconsistent. The resolver witness therefore runs in a detached
baseline worktree under `/tmp`; no shared branch or conflict was changed. The
validator checks archived bytes rather than requiring an unchanged live checkout.

## Existing architecture and reuse

| Responsibility | Existing evidence | Cortados integration |
| --- | --- | --- |
| Foreign application evidence | [FAIR/2](../disparates/contracts/fair-2.md), [goya-fair](../../crates/goya-fair/src/lib.rs) | Reuse source manifests, elements, relationships, evidence references, canonical serialization and representability states. |
| Transformation ownership | [Disparates roadmap](../disparates/roadmap.md) | Keep extraction, representability and reconstruction separate. The roadmap still withholds reconstruction pending corpus evidence; this audit does not change that status. |
| Source admission | [goya-lang](../../crates/goya-lang/src/lib.rs), [handoff](../../crates/goya-lang/src/handoff.rs), [GSSH](../gssh/GSSH-2.md) | Emit only an explicitly selected supported profile and admit through its shared handoff. Permissive parsing is insufficient. |
| Runtime execution | [resolver admission](../../crates/goya-resolver/src/admission.rs), [execution](../../crates/goya-resolver/src/execution.rs) | Resolver admission enumerates GSSH/1 through GSSH/4-live. GSSH/5 parser availability does not establish resolver support. |
| State and bounded logic | [scene expressions](../../crates/goya-lang/src/scene.rs), [transition compiler](../../crates/goya-lang/src/transition.rs), [D1 VM](../../crates/goya-executable/src/d1_vm.rs) | Reuse typed scalar transitions when equivalence is established. These are not a general Java execution model. |
| Effects and permissions | [D2 VM](../../crates/goya-executable/src/d2_vm.rs), [executable capability validation](../../crates/goya-executable/src/lib.rs) | Reuse explicit effect requests and capability checks where admitted. D2 consumes supplied journal results; it does not itself invoke a filesystem or network service. |
| Layout and interaction | [scene](../../crates/goya-lang/src/scene.rs), [gestures](../../crates/goya-lang/src/gestures.rs), [intents](../../crates/goya-lang/src/intents.rs) | Reuse proportional geometry, authored identities and admitted actions. Parsed experimental intent declarations must not be mistaken for supported target execution. |
| Graphics | `goya-lang/src/graphic.rs` and `goya-scene-graphics/src/lib.rs` (historical, unavailable), [graphics roadmap](../graphics/README.md) | Reuse GIR, GVA, GRU and GVI where eligible. Scene lowering was observed in the initial working tree but later disappeared during concurrent edits. Treat this as historical evidence; graphics is rejected in the frozen native profile. Do not infer target support. |
| Correctness integration | [Diablos Inquisition integration](../../crates/goya-diablos/src/certification/inquisition.rs) | The inspected entry point explicitly identifies itself as a stub. Its report type is not Java differential-execution evidence. |
| Existing ingestion precedent | [Electron extractor](../../crates/goya-disparate-electron/README.md) | Reuse design lessons: offline analysis, budgets, path checks, exact source hashes and explicit unknowns. Its ASAR reader and JavaScript parser do not constitute JAR or Java support. |
| State continuity | [romántico protocol](../romantico/protocol.md), [state store](../../crates/goya-state/src/lib.rs) | Audit serialization and lifecycle eligibility per Java state model. A mutex-protected Goya store does not implement Java monitor semantics or establish portable Java object serialization. |

## CSIR and FAIR boundary

FAIR/2 supplies a versioned evidence graph with closed element and relationship
vocabularies. It has no typed Java instruction, object identity, exception-region
or happens-before model. Its general semantics payload can preserve observations,
but accepting a payload is not semantic validation of Java behavior.

The selected design is a Java-specific, versioned CSIR internal to the frontend,
linked to FAIR source and element identities. Keep implementation semantics intact;
record recovered application intent as a separate evidence-backed view. CSIR must
not replace FAIR's shared provenance or introduce Java types into `goya-ir`.
The boundary is now resolved by the independently versioned CSIR sidecar in
[architecture.md](architecture.md) and [csir.schema.json](csir.schema.json), leaving
FAIR/2 unchanged. The detached hand-authored fixture tests structural validation
and preservation of unknowns; real extraction and attached FAIR validation remain
Phase 1 implementation requirements.

Existing FAIR kinds can describe methods as `logic_unit`, UI controls as
`component`, handlers as `event_handler`, and opaque constructs as `unknown`.
Do not invent Java-specific FAIR enum variants or reason codes without a revision.
`COR-E*` diagnostics belong to a proposed Cortados diagnostic namespace; they do
not automatically become registered FAIR reason codes. Hypothesized button meaning
must stay separate from established facts: the label “Save” alone does not prove
persistence, its scope or its side effects.

| CSIR domain | Evidence available now | Missing Cortados contract |
| --- | --- | --- |
| Type | Scalar state schemas | Java primitives, references, arrays, generics, conversions and runtime type checks |
| Identity | Authored scene IDs | Heap identity, aliases, null, reference equality and identity hash behavior |
| Control | Straight-line assignments and checked addition | General calls, dispatch, branching, loops, recursion and abrupt completion |
| Memory | Transactional scalar store | Allocation, heap mutation, reachability and visibility |
| Effects | Typed static D2 journal requests | Java I/O semantics, real host execution, failures and capability policy |
| Concurrency | Host synchronization implementation | Java Memory Model, monitors, interruption, atomics and wait sets |
| Lifecycle | Resolver lifecycle and state lifetimes | Java class initialization and framework lifecycle correspondence |
| Interface | Scene tree, buttons, input and state bindings | Swing EDT, JavaFX bindings, custom subclass behavior and complex widgets |
| Graphics | Bounded native geometry and GVI lowering | Java painting recovery, text fidelity and evidence of finite reachable states |
| Dependency | FAIR import and manifest evidence | Java resolution, licenses, reachable implementation and JSER coverage |
| Provenance | Source digests, spans and evidence links | Pass versions, transformation contracts and output-to-source trace |

The machine-readable audit tracks each of these eleven domains and thirty-four
Java semantic requirements with evidence and closure conditions. These are
findings about the inspected contracts, not a proof that no possible lowering
could express a given Java program.

## Java semantic deficits

| Requirement | Current finding and necessary closure |
| --- | --- |
| Fixed-width arithmetic | `InteractionValue::Int` is `i64`; D1 and D2 use `checked_add`. Java `int` overflow, for example `2147483647 + 1`, must yield `-2147483648`, not `2147483648`. Java `long` overflow must not become a Goya overflow diagnostic. Need fixed-width lowering or a proved range restriction for every relevant execution. |
| Floating point | Scene geometry uses floating point, but interaction scalars have no float variant. Need an execution contract for Java float/double rounding, NaN, signed zero, conversions and observable formatting. |
| Objects and identity | Scene IDs identify interface nodes. They do not supply allocation, aliases, `null`, `==`, `equals`, identity hashes or cyclic object graphs. Need explicit object semantics or justified elimination. |
| Inheritance and interfaces | No general method-dispatch instruction in the inspected D1/D2 sets. Need overload resolution, virtual/default dispatch, superclass construction and interface contracts. |
| Generics and arrays | No collection/array interaction value. Need erasure, casts, runtime component types, covariance, bounds, evaluation order and applicable exceptions. |
| Collections and text | A Rust map in the host implementation is not a Java `HashMap` replacement. JSER must establish null handling, equality/hash behavior, iteration effects and mutation rules. Java UTF-16 `char`/String indexing cannot silently become UTF-8 byte or Unicode scalar indexing. |
| Branches, loops and calls | Current authored expressions are literal/state/input/effect reads and addition. No general control-flow graph or call stack. Need a governed language/execution contract or evidence-backed specialization that preserves all reachable behavior. |
| Exceptions and cleanup | Transition errors refuse execution; they are not Java exceptions. Need typed propagation, source evaluation order, `finally`, try-with-resources and suppressed exceptions across abrupt exits. |
| Class initialization | Resolver launch is not Java class initialization. Need initialization triggers, ordering, recursive initialization, synchronization and failure propagation. Never execute initializers during static ingestion. |
| Lambdas and annotations | Need capture, invocation and meaningful annotation effects. Annotation processors remain disabled during ordinary ingestion. |
| Reflection and loading | FAIR can record unresolved constructs. No finite Java reflection inventory or dynamic loading policy is implemented here. Need bounded resolution with explicit closed-world assumptions, or a declared residual boundary. |
| JNI and generated code | No verified Cortados native replacement registry. Need ABI, authority and side-effect evidence; fail conversion when no authorized correct replacement exists. |
| Lifetime and references | Host Rust memory management does not prove Java reachability equivalence. Need weak/soft/phantom reference treatment, reference queues, native resource lifetimes and supported GC-sensitive observations. |
| Java Memory Model | No Java concurrency contract in the inspected scene/VM interfaces. Need monitor ownership/reentrancy, happens-before, volatile/atomic operations, interruption, wait/notify, executors/futures, thread locals and supported virtual threads. A single-thread lowering requires proof of equivalence. |
| Persistence and networking | Durable Goya scalar state and replayed effect responses do not establish Java filesystem, socket or SQL behavior. Need real capability-scoped host adapters, ordering, failure and security evidence, without broadening original authority. |
| UI and graphics | Basic buttons and relative layout are reusable. Need framework lifecycle/event semantics, custom painting fidelity and conservative recovery. The existing graphics lowerer uses product reachability; Cortados must prove that inventory appropriate or supply an approved more precise path. Unbounded graphics cannot become a finite inventory by assumption. |

## Reconciled implementation and output plan

Use the existing root Rust workspace when implementation begins. Add only the
crates needed by the next admitted phase. The selected first crate is
`crates/goya-cortados`, with an analysis library and a `cortados` binary; split
parser/CSIR/frontend crates as actual interfaces and build boundaries emerge.
Do not create the directive's nineteen-crate nested workspace as empty scaffolds.
Rust is selected for the first analysis crate; no new crate is added in Phase 0.

The first implementable slice is static Java analysis: bounded directory ingestion,
exact source manifests, Java syntax parsing with source spans, explicit unresolved
types/dependencies, and FAIR-compatible evidence reporting. The [Java 17 profile](profile.json) fixes
source encoding, preview policy, source/module/symbol inventory rules, parser
requirements and resource budgets. Parser implementation and its conformance
evidence belong to Phase 1. Parsing Maven/Gradle metadata must not execute build scripts,
plugins or annotation processors, retrieve dependencies or guess computed values.
JAR/WAR/AAR decompression and class/DEX decoding need their own hostile-input tests.

Keep `analyse` as the first planned CLI command. Reserve `convert`, `verify`,
`report` and `targets` until their implementations and schemas exist. An unavailable
conversion must return an actionable diagnostic; it must not emit an empty scene
or manufacture a certificate. Programmatic and CLI interfaces must share analysis
and diagnostic implementations.

`application.gscene` must use the existing scene grammar and strict handoff, with
an explicit profile. `logic/` must not introduce a separate unapproved source
language, embed Java bytecode or hide handwritten host logic needed for execution.
The directive's `manifest.json`, capability, provenance, dependency, SBOM and
certification filenames remain proposed packaging. Reconcile them with the chosen
target's manifest and artifact contracts before freezing an output schema.

An analysis bundle should contain source hashes, frontend/profile/pass versions,
diagnostics, FAIR evidence, a dependency inventory and unresolved requirements.
Conversion bundles later add canonical scenes, asset mappings, capability
requirements, replacement ledger, verification evidence and certification. All
supporting formats need independent schema versions. This audit's
`GY-CORTADOS-AUDIT/0` JSON is a local audit format, not a conversion contract.

## Correctness and certification plan

Maintain separate evidence for ingestion, syntactic validity, runtime behavior,
native independence and target conformance. FAIR progression and C0–C5 certification
serve different purposes and have no automatic one-to-one mapping. No application
is assigned C0 by this repository audit; no Java application was ingested.

| Level | Evidence required before assignment |
| --- | --- |
| C0 | A particular Java revision was ingested and assessed under a declared profile. |
| C1 | Emitted source passed the selected GSSH admission; completeness and residuals disclosed. |
| C2 | A declared behavioral acceptance suite passed against the original under controlled conditions. |
| C3 | C2 evidence plus dependency/runtime evidence establishing no JVM execution requirement within the certified scope. |
| C4 | C3 plus separate named target/configuration conformance evidence. |
| C5 | C4 plus raw, reproducible performance measurements, explicit objectives and material regressions. |

Certification must bind source/dependency/resource digests, transformation versions,
configuration, Java and GSSH profiles, declared scope, target configuration and
test evidence. Lower levels cannot be promoted by a heuristic confidence score,
a successful parser invocation or a report struct containing a positive verdict.
Strict-native mode must reject residual JVM execution; compatibility-assisted
boundaries require explicit developer policy. Incremental artifacts must disclose
uncertified reachable behavior.

Differential execution should compare ordered state/effect/exception traces for
identical inputs, not only final output. Fixtures need independent oracles for
overflow, aliasing, initialization, exception cleanup, concurrent observations,
file failures, permission denials and lifecycle transitions. Java reference runs
belong in explicitly authorized isolated execution with bounded resources and
controlled networking. Static extraction never runs application code.

Begin B0 with arithmetic/control-flow counterexamples, B1 with collections and
UTF-16 text cases, and B2 with scoped persistence and exceptions. A counter that
uses only safe addition can test infrastructure but cannot satisfy the minimum
viable Cortado: that milestone also requires multiple classes/interfaces,
collections, mutable state, persistence, exceptions and interaction. Prove all
these through one real JVM-free Goya path before claiming the milestone.

No Cortados performance measurements exist. The directive's 50/50/60/80/100
objectives remain workload-specific targets. Benchmark protocols must report JVM
version/configuration and warmup, installed/bundled runtime scenarios, repeated
runs, raw samples, memory, energy where measured, fidelity and regressions.

## Release gate assessment

All production gates are **not assessed** for Cortados. Existing dependency tests
support the reuse audit; they are not Cortados conformance or release admission.

| Gate | Evidence still required |
| --- | --- |
| COR-G01 | An implemented Cortados workspace member builds. |
| COR-G02 | Formatting and lint checks on the implemented component. |
| COR-G03 | Mandatory Java semantic conformance suite for a declared profile. |
| COR-G04 | Repeated byte-identical conversions under documented conditions. |
| COR-G05 | Actual emitted scenes pass the selected shared handoff. |
| COR-G06 | Original and converted applications pass differential scenarios. |
| COR-G07 | Complete dependency, license, source and transformation manifests. |
| COR-G08 | Reachability evidence and explicit classification of unknown functionality. |
| COR-G09 | Hostile source/archive tests, bounds enforcement and authority tests. |
| COR-G10 | Strict-native fixture execution without JVM dependencies. |
| COR-G11 | Real destination execution integration, rather than syntax admission alone. |
| COR-G12 | Reproducible benchmark methodology and raw measurements. |
| COR-G13 | Versioned profile documenting unsupported features and diagnostics. |
| COR-G14 | Certification independently checked against its bound evidence. |

## Decision register and Phase 0 closure

All seven decisions COR-D01–D07 are resolved in [architecture.md](architecture.md).
The [closure record](phase0-closure.json) binds their evidence and records the
three Luna review roles. [profile.json](profile.json) gives an explicit conditional
mapping or native refusal for every initial semantic requirement, retaining the
original gap evidence and future implementation closure conditions.

The earlier audit incorrectly placed the substantive minimum viable application
inside COR-D06's Phase 0 criterion. Directive §25.1 requires architectural audit
and identification of expressive gaps; §25.2 defines a subsequent operational
milestone. The closure now distinguishes those scopes. The manual counter proves
only a headless resolver feasibility path. Its Java source was not run, and it is
not automated conversion, differential parity, platform UI conformance or MVP evidence.

Source analysis and native representation profiles are fixed. Unavailable semantics
are explicitly rejected, CSIR/FAIR and output boundaries are decided, and the existing
resolver path has a runnable feasibility witness. This closes architectural Phase 0.
Implementation, parser conformance, reconstruction authority, substantive native
conversion, Java differential execution and production release admission remain
later requirements.

Recreate the detached baseline worktree if it is absent, then validate the
artifacts and rerun the live witness:

```sh
git worktree add --detach /tmp/goya-cortados-phase0 b408ee19a15037f440db49f1cabd5ad93f660452
deliver --spec docs/cortados/deliver.toml --base . --strict --allow-commands --format json
```

The check validates hashes, all 34 requirement dispositions, all 7 decisions, the
CSIR fixture and 20 tamper cases, and runs 5 resolver scenarios. It does not
certify Java conversion. The original infrastructure checks remain recorded in
the audit separately from the new Phase 0 execution evidence.
