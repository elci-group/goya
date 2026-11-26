# GY-REPLAY/1 — Behavioural Replay and Equivalence Verification (draft)

Directive: GY-ENT-001 (Disparates programme). Date: 2026-11-19.
Status: Draft — Phase 3 groundwork, not ratified; authorisation for foreign-code execution under replay is explicitly pending.

## Purpose

GY-FAIR/1 defines `verified` — behavioural equivalence demonstrated — but its own
rules make the state unreachable: "Verification requires replay evidence produced
under a future equivalence contract; this revision cannot produce `verified`."
This document is that future contract, in draft. It defines how the original
foreign application and the reconstructed Goya application are exercised with
identical interaction scenarios, how each emits a normalised state trace, how
traces are compared, and under what rules an element may honestly be marked
`verified`.

Nothing here is ratified. No scenario has been captured, no trace collected, no
comparator built, and no execution of foreign application code is authorised by
this draft. `verified` remains unreachable until all three of: this contract
ratified, execution authorisation recorded, and the GY-FAIR/1 revision (below)
ratified.

Naming note: this contract is unrelated to D1 effect-journal replay
(GY-D1-REPLAY), which replays recorded effects inside a Goya VM. GY-REPLAY
replays whole applications to compare behaviour.

## Scope boundary and authorisation

The GY-FAIR/1 determinism rule ("Foreign application code is never executed
during extraction. Extraction is static.") and the roadmap constraint ("No
execution of untrusted foreign application code during static extraction") are
constraints on **static extraction only**. Replay is a different activity: it
executes the original foreign application and the reconstructed Goya
application, side by side, under identical scripted interaction. Replay does
not loosen extraction; extraction remains static under every revision.

Foreign execution under replay is acceptable only when **all** of the following
authorisation conditions hold, and each is recorded per run:

1. **Recorded authorisation.** A specific, dated authorisation decision for this
   run (scope, corpus pin, operator, approver) exists before the run. A replay
   run without a citable authorisation record is invalid evidence, not merely
   irregular. No such record exists today.
2. **Sandboxing.** Execution occurs in an OS-level sandbox: filesystem access
   confined to declared, disposable scenario roots (fresh application profiles
   per run); no access to the operator's environment; bounded CPU, memory and
   wall-clock resources; no effects outside the sandbox survive the run.
3. **No network.** All network egress denied by enforcement, not convention.
   v1 provides no network stubbing or service simulation: a scenario that needs
   a live third-party service is `inapplicable` (below). Stubbing changes the
   experiment — the app would be verified against the stub, not the service —
   so it is withheld pending an explicit contract revision.
4. **Pinned builds.** The foreign application is built from the corpus-pinned
   commit with a recorded, hashed build recipe; the reconstructed application
   is pinned the same way. Replay evidence binds to build hashes, never to
   "current checkout".
5. **Licensing preserved.** Execution and any recorded output respect the
   corpus application's license; attribution obligations survive into evidence.

Phase scoping: the roadmap phase 3 exit condition is capture of
original-application scenarios as normalised state traces. Capture exercises
the foreign application only and asserts nothing about Goya. Equivalence
comparison becomes meaningful once reconstruction (phase 4) produces a second
trace source; `verified` is reachable only after that.

## Provides (proposed; names reserved, not implemented)

- `crates/goya-disparate-replay` —
  `pub const REPLAY_CONTRACT: &str = "GY-REPLAY/1"`. No code exists under this
  draft; the name is reserved as in the roadmap crate family.
- `scenario` — parser and strict validator for the interaction-scenario grammar
  (below); compiles scenario text to canonical scenario JSON.
- `runner` — drives both applications under the recorded sandbox, enforces the
  quiescence discipline, aborts on any sandbox or pin violation.
- `collector` — hosts the observation harness for each application; emits
  normalised, canonical traces.
- `comparator` — aligns two traces and produces a complete diff.
- `report` — per-scenario verdicts, per-element replay coverage, canonical
  report serialisation.
- CLI (reserved): `disparate-replay run|compare|report`.

## Scenario format

Scenarios are deterministic interaction scripts: ordered action steps with
declared inputs, a timing discipline, and expected observable outcomes. The
format is versioned with the contract (`format: GY-REPLAY/1`, scenario format
revision 1) and follows the workspace's declarative conventions: an authoring
grammar in the GY-LANG `@`-block idiom, compiled to a canonical JSON form for
machine consumption, in the way scenes lower to IR. Authoring files use the
reserved extension `.gscenario`.

Proposed shape (illustrative; not a captured scenario):

```text
// reactotron-timeline-filter.gscenario
@scenario timeline_filter_show {
  target: reactotron-app;              // corpus short name, bound to the pin
  setup {
    state: fresh;                      // fresh sandbox profile, every run
    window { size: (1280, 800); }
    seed { file "arc/config.json" = payload "seed/config.json"; }
  }
  step open_timeline {
    trigger: button.timeline.activate; // v1 trigger vocabulary, below
    expect { ui { visible "Timeline"; } }
  }
  step filter_styles {
    trigger: input.filter.commit;
    input { value: text "styles"; }
    expect {
      ui    { input "filter" = "styles"; }
      store { file "arc/filters.json" contains key "styles" = true; }
    }
  }
}
```

Grammar rules:

- A scenario file contains one `@scenario`; names are unique within a scenario
  set. `//` comments; `key: value;` properties; blocks terminated by newline or
  `;` — GY-LANG conventions.
- `setup` fixes the deterministic start: fresh profile, fixed window geometry,
  optional seeded files copied verbatim from recorded payloads. Absent setup is
  rejected; a scenario without declared initial state cannot be deterministic.
- `step` blocks are ordered; order is execution order and comparison order.
  Each step has exactly one `trigger`, optional typed `input { }`, and optional
  `expect { }`.
- **Trigger vocabulary** (v1, open and versioned like the FAIR element-kind
  vocabulary): `button.<id>.activate`, `input.<id>.commit`,
  `ipc.send <channel>`, `window <event>`. Identifiers resolve through the
  binding map (below). Unknown triggers, unknown input types, and duplicate
  step names are validation errors — strict, never tolerated-and-ignored.
- **Timing discipline — quiescence only.** Steps advance at **quiescence
  barriers**: points where the application declares, via an observable
  quiescence oracle (no pending scheduled work and no further state change
  over the oracle's declared stability window), that it has settled. Wall-clock
  sleeps are prohibited and unrepresentable in the grammar. Each application
  under replay must declare its quiescence oracle in the run record; an
  application with no oracle for a behaviour cannot be replayed for that
  behaviour. Scenarios are human-authored and human-reviewed before capture;
  they are data, versioned alongside the corpus pin.
- `expect` blocks are authored observable outcomes: UI state, persistence
  effects, and side-effect outcomes phrased in the trace vocabulary (below).
  They are semantic guardrails, not the comparison itself (below).

## Trace normalisation

Each application under replay emits a **normalised state trace**: an ordered
sequence of observable state records, collected at quiescence barriers by a
declared observation harness. Traces are canonical JSON
(`trace_format: 1`): fixed field order, sorted map keys, trailing newline —
the GY-FAIR/1 serialisation conventions.

Record kinds (v1, deliberately drawn from the FAIR element-kind vocabulary so
trace records and extracted elements speak the same language):

| Kind          | Content                                                                 |
|---------------|-------------------------------------------------------------------------|
| `ui_state`    | Observable UI values at a barrier: text content, input values, control enabled/selected state — addressed by binding |
| `persistence` | Files/keys written or mutated: sandbox-relative path, normalised parsed value or canonical-byte SHA-256 |
| `side_effect` | Outward actions: IPC message sent, child process spawned (with argv), OS integration call issued — kind and payload |

Observation honesty: a behaviour observable only through the harness is
verified only relative to that harness's fidelity. The harness, its
instrumentation of the foreign application, and the observation surface
version are declared in the run record. An instrumentation that alters
application behaviour invalidates the run.

**Binding.** Cross-implementation alignment is keyed by FAIR element identity.
Each scenario identifier (`button.timeline`, `input.filter`) resolves, per
application, through a versioned **binding map** to that application's
observable address; both bindings must cite the same FAIR element ID. This
places a requirement on reconstruction (unauthorised, but designed for now):
generated Goya constructs must carry FAIR provenance, or their behaviour is
not comparable. A scenario step whose binding is unresolved for either
application makes the scenario `inapplicable` with
`REPLAY_BINDING_UNRESOLVED` — it produces no evidence, and the binding gap is
recorded, never worked around.

**Normalisation register.** Nondeterministic values are replaced by declared,
versioned substitutions: sandbox paths to scenario-relative paths, timestamps
omitted, ephemeral ports and generated identifiers omitted. Every applied
substitution is visible in the trace. A substitution may not mask a value that
is semantically load-bearing for the scenario; where it would, the scenario is
`inapplicable`. The register grows by contract revision, never silently.

**Comparability.** Two traces are comparable when they were produced by the
same scenario revision, the same corpus pin, and declared-compatible harness
versions. Alignment rules:

- Traces align **per step**, by step index — never by timestamp.
- Within a step, records align by (record kind, binding address). Where an
  application declares no ordering guarantee among same-address records, they
  compare as canonically ordered sets; where ordering is declared, order
  compares.
- Records present in one trace only are `unaligned` and count as mismatches.
  A comparator that cannot align a record never drops it from the report.
- Differences surviving alignment are classified `match` or `mismatch` with
  both values shown; `not_compared` marks records excluded by declared
  substitution or declared scenario exclusion. Every record appears in the
  report under exactly one classification.

**Deliberately not compared, and why:**

| Excluded                | Reason |
|-------------------------|--------|
| Pixel / visual output   | Rendering is presentation, not semantics. Goya legitimately simplifies architecture (IPC dissolves into actions; styling re-expresses). Pixel equality would render honest equivalence unreachable while proving nothing about behaviour. |
| Timing and performance  | Host-dependent and nondeterministic. Performance is a separate evidence class with its own conventions. |
| Focus, IME and window-manager decoration details | Host integration specifics, not application semantics; comparing them would verify the platform, not the migration. |
| Asynchronous delivery order within a barrier where no ordering is declared | Electron and Goya schedule differently; the scenario's observable outcomes at the barrier are the contract. |

## Verdict rules

Scenario verdicts (per scenario, per run):

| Verdict         | Meaning |
|-----------------|---------|
| `passed`        | Both applications completed all steps; authored expectations hold on each trace independently; traces align with no `mismatch` or `unaligned` records outside declared exclusions. |
| `failed`        | Recorded with the full diff. A mismatch in any exercised step fails the scenario — partial passes are not a verdict. |
| `error`         | Harness, sandbox or infrastructure failure. Evidentially equivalent to no evidence, but recorded distinctly and never retried into disappearance. |
| `inapplicable`  | Declared condition (binding unresolved, network dependence, load-bearing substitution) prevents evidence. Recorded, provides no verification credit. |
| `not_run`       | Declared in the set but not executed. Recorded. |

Element-level replay coverage is recorded **in the replay report, not in FAIR
states**: `exercised_passing`, `exercised_failing`, `exercised_inapplicable`,
`unexercised`. FAIR's seven-state enumeration remains frozen.

**When an element may be marked `verified`** — all of:

1. The element is `translated`, with generated native source recorded against
   it.
2. A ratified GY-REPLAY/1 run, under a recorded authorisation, exercised the
   element through a resolved binding in at least one `passed` scenario, and
   no `failed` scenario exercised it.
3. Comparison was at value level: no load-bearing field of the element's
   observable behaviour was masked by normalisation substitution.
4. The evidence is persisted with content hashes and is reproducible: a repeat
   run under the same pins is byte-identical modulo declared substitutions.
5. The verdict records its scope: `verified` means *behavioural equivalence
   demonstrated over the recorded scenario set, observation surface and
   normalisations* — never a claim of universal equivalence.

**Honesty rules.**

- Compilation, translation, or a passing subset of scenarios **never** implies
  `verified` — for the exercised element or any other. `exercised_passing`
  elements that fail condition 2 or 3 remain `translated` with the replay
  evidence recorded as a semantics facet.
- Failed scenarios are recorded, never dropped: every scenario in the set
  appears in the report under exactly one verdict, including `error`,
  `inapplicable` and `not_run`. A run that cannot complete still produces a
  report of what did not run.
- Demotion: a later ratified run that fails a scenario exercising a `verified`
  element demotes it to `translated` with reason code
  `REPLAY_EQUIVALENCE_LOST` and the failing run ID. Evidence history is
  append-only; earlier evidence is never rewritten or deleted.
- A whole-migration equivalence claim requires every element `verified` or
  explicitly out of replay scope with a reason code. For any real application
  this is currently unreachable, and no run may phrase its output to suggest
  otherwise.

## GY-FAIR/1 revision path

`verified` becomes reachable only by contract revision, never by tooling
side-effect. The revision path:

1. **Ratify GY-REPLAY/1** — this document, after pilot capture on the
   reference corpus and the authorisation decision for sandboxed foreign
   execution.
2. **Amend GY-FAIR/1** (in-place amendment or GY-FAIR/2, per architecture
   review): name ratified GY-REPLAY/1 as the authorised replay-evidence
   authority; define `verified` as requiring the five conditions above, with
   the run ID, scenario-set hash and comparator-report hash recorded as a
   semantics facet on the element; keep the seven-state enumeration, the
   reason-code register discipline and the provenance rules unchanged.
3. **GY-COVERAGE/1 is unchanged in effect**: assessment still never sets
   `verified`; assessment dispositions and replay verdicts remain independent
   evidence classes that both feed the element's final state.

Until both revisions are ratified, tools must refuse to emit `verified`, and
the refusal is a validation rule, not a convention.

## What replay cannot verify

Some behaviour is outside replay's evidential reach regardless of tooling
quality. Elements whose semantics are dominated by the following stay
honestly `undetermined` (or, unextracted, remain whatever FAIR records); a
scenario touching them is `inapplicable` unless it confines expectations to
what is observable:

| Class                      | Why unverifiable | Proposed reason code |
|----------------------------|------------------|----------------------|
| Timing-dependent behaviour | Semantics defined by timing (debounce windows, animation, scheduling order under time pressure); replay compares state at barriers and deliberately ignores timing | `REPLAY_NOT_OBSERVABLE_TIMING` |
| OS-integration side effects | Dialogs, notifications, tray, shell integration appear on the desktop, outside every declared observation surface; Goya may also lack the capability entirely | `REPLAY_NOT_OBSERVABLE_OS_INTEGRATION` |
| Third-party service interactions | Network is denied by authorisation condition; with stubbing withheld, live-service behaviour cannot be exercised, only intended | `REPLAY_NOT_OBSERVABLE_EXTERNAL_SERVICE` |

Side-effect **intent records** (an issued OS call, an outbound request shape
observed pre-egress) may be collected and compared, and are useful evidence —
but intent-level agreement is weaker than behavioural equivalence and never
constitutes `verified` on its own; an element verified only at intent level
stays `translated` with the evidence recorded.

## Tooling plan

`crates/goya-disparate-replay` owns four responsibilities. No implementation
exists under this draft; these are obligations the future implementation must
meet, not API promises.

- **Scenario runner.** Load, validate and execute a scenario set against both
  pinned builds inside the recorded sandbox; enforce quiescence discipline and
  pin identity; abort the run on any sandbox violation. Run aborts still emit
  a report.
- **Trace collector.** Host per-application observation harnesses; apply the
  normalisation register; emit canonical traces bound to (scenario revision,
  corpus pin, build hash, harness version).
- **Comparator.** Align traces per the rules above; emit the complete diff —
  every record classified, none dropped.
- **Report.** Produce the canonical verdict report and element coverage
  summary that become replay evidence.

Dependency stance mirrors `goya-fair`: no foreign framework runtime code is
vendored into the crate; harness instrumentation for foreign applications is
declared data, versioned in evidence. No LLM inference participates in
running, comparing or reporting.

## Evidence conventions

Replay extends the programme's evidence conventions
(`evidence/disparates/<project>-phase<N>-replay/`; the phase 3 location
`evidence/disparates/reactotron-phase3-replay/` is anticipated but no run
exists). All JSON canonical, as in GY-FAIR/1 serialisation.

Directory layout (reserved):

```text
evidence/disparates/<project>-phase<N>-replay/
  run.json                          run record (schema below)
  scenarios/<name>.gscenario        scenario sources, reviewed
  scenarios/set.json                canonical compiled set with hashes
  binding/<impl>.binding.json       per-implementation binding maps
  traces/<scenario>.<impl>.trace.json
  comparator/<scenario>.diff.json
  report.json                       verdicts and element coverage
```

`run.json` schema shape (extends the extraction `run.json` convention; field
presence enforced once the contract is ratified):

```json
{
  "run": "<project>-phase<N>-replay",
  "date": "YYYY-MM-DD",
  "authorisation": {
    "record": "<id of the authorisation decision>",
    "sandbox": "<declared sandbox and enforcement mechanism>",
    "network": "denied (enforced)",
    "pins": {
      "foreign": { "commit": "...", "build_recipe_sha256": "..." },
      "goya":    { "commit": "...", "build_recipe_sha256": "..." }
    }
  },
  "scenario_set": { "format": "GY-REPLAY/1", "revision": 1, "count": 0, "sha256": "..." },
  "observations": {
    "<impl>": {
      "harness": "<identity and version>",
      "observation_surface_version": 1,
      "quiescence_oracle": "<declared predicate>",
      "instrumentation": "<declared, or none>",
      "normalisation_register_version": 1
    }
  },
  "output": {
    "verdicts_by_scenario": [ { "scenario": "...", "verdict": "..." } ],
    "element_coverage": { "exercised_passing": 0, "exercised_failing": 0,
                          "exercised_inapplicable": 0, "unexercised": 0 }
  },
  "determinism_evidence": {
    "method": "repeat run under identical pins",
    "result": "traces byte-identical modulo declared substitutions"
  },
  "honesty_notes": [ "non-empty; states what this run does and does not demonstrate" ]
}
```

The `authorisation` record is mandatory and non-null: it is the structural
marker that execution was permitted for this run. Evidence produced without
it is invalid by construction. This mirrors GY-DISP-002.json's honesty
discipline: `honesty_notes` is mandatory, non-empty, and must state what the
run does not demonstrate — for a capture-only phase 3 run, that it asserts
nothing about Goya behaviour.

## Invariants

- No replay activity occurs without a recorded authorisation; the sandbox,
  network denial and pin identity are enforced per run and cited in evidence.
- Extraction remains static; nothing in replay reinterprets GY-FAIR/1's
  no-execution rule for extractors.
- Scenario validation is strict: unknown triggers, inputs, bindings or
  normalisations are errors, never ignored.
- Traces, diffs and reports are canonical and deterministic: identical pins,
  scenario revision and harness versions yield byte-identical output modulo
  declared substitutions.
- Every record in a comparison appears in the report under exactly one
  classification; every scenario in a set appears with exactly one verdict.
- No tool emits `verified` before the GY-FAIR/1 revision and a ratified,
  authorised GY-REPLAY/1 run both exist; the refusal is enforced by
  validation.
- Evidence is append-only: later runs add; they never rewrite, reclassify or
  delete earlier evidence.

## Stability

Draft. Everything in this document — grammar, registers, verdict rules,
evidence schema — is proposal until ratified. The trigger vocabulary, side
effect kinds and normalisation register grow by contract revision; existing
entries are never renamed or renumbered. Ratification requires: the
authorisation decision for sandboxed foreign execution, pilot scenario
capture on the reference corpus, and the GY-FAIR/1 revision naming this
contract as evidence authority. Until then `verified` stays unreachable, and
that is the honest state of the programme.
