# Chromatics: Phase 0 reconciliation and core engine

Status: **Phase 0 reconciliation done; Phase 1 core built**, 26 November
2026, branch `nebula/v0.1`, uncommitted. There is no `.gscene` syntax, no
Arachnid integration and no device run. No release gate is claimed (§6).
Continued by `CHROMATICS-1.md`, which adds the `.gscene` authoring surface
this file's §5 recommendations (3a, 4, 5) proposed.

The owner pasted directive GOYA-INT-CHR-001 r1.0 ("Chromatics — Adaptive
Semantic Input Allocation Architecture"). This file has four parts:
* it compares the directive with the tree;
* it records corrections;
* it describes what was built;
* it lists the decisions only the owner can make.

## 1. What the directive depends on, and what exists

| Directive dependency | What the tree has | Gap |
| --- | --- | --- |
| Colour keys on webOS (§13) | Arachnid `remote.rs` gives 403–406 `Handling::Colour`. The Slice 0 probe **observed** them on the NEON (webOS 22): keydown/keyup reach a third-party app, and held keys repeat (`docs/arachnid/launcher.md` §4, branch `arachnid/v0.3`). Back belongs to the platform. | Only one TV and one stock remote have been observed. |
| Static allocation | `remote-colour: red\|green\|yellow\|blue` on `@button` (live profiles only). For each presence configuration, at most one present button may claim a colour (`GY-AR-650`). With no claimant, the key goes to the platform. | This is the directive's **Manual** strategy, precomputed per presence configuration. Nothing ranks, adapts or stabilises. |
| Reference launcher (§22) | `apps/launcher.gscene` on `arachnid/v0.3`, uncommitted: Red Applications, Green Household, Yellow Broadcast, Blue Activity. This is the directive's Home row. Profiles, per-profile favourites, `@apps` launch and four Household device slots (`d1`–`d4` host state from bloom-tv). | The bindings are fixed. Favourites are precomputed per profile, not a collection. |
| Gestures as the intent vocabulary (§18) | In this tree, **Gestures is GOYA-GEST-TD-001: text-editing policy** (copy/cut/paste decisions, `GesturePolicy`), at G0 and unratified (`docs/gestures-decisions.md`). | Gestures has no action vocabulary. The closest constructs are `@interaction` (bounded D1/D2 transitions) and the experimental `@intent` (Quitasol phase 0). An `@intent` names an `@interaction` and carries a confirmation flag, remote/keyboard fallbacks and a category. |
| Inquisition (§25) | **It does not exist** (also recorded in `docs/delectable/DELECTABLE-0.md`). | The runtime invariants are in this crate's verification layer. Static checks would be goya-lang/Arachnid diagnostics. |
| Goya Resolver (§14) | GY-RESOLVER/0.1 (`b408ee19`) is headless D1 only, with no remote input. Viuda is on `viuda/v0.1`. | The resolver has nothing for Chromatics yet. The shared fixtures (§4) are its future contract. |
| Dynamic collections (§12.2) | `.gscene` has no collection or list construct. `@template` is refused (`GY-E1002`). Lists are written out as repeated cells and presence variants. | `source: shows` over a runtime collection has nothing to bind to. **This is the largest gap for §12.** |
| GVI (§17) | `@graphic` + `@draw` with dimensions, as in the launcher logo's `channel` dimension. Arachnid ships `gvi.js`. | A marker inventory of colour × visibility × focus is a small finite model. It fits as is. |
| Runtime on the TV | Arachnid emits JavaScript for Chrome 87. Rust → wasm is not possible, because std needs reference-types and Chrome 87 lacks them (ARACHNID-2). | The engine on webOS must be **generated JavaScript**, checked against the same fixtures. The precedent is `gvi_js.rs`, conformed in Node against the Rust reference. |
| Names | `goya-chroma` (GY-CHROMA/1, OKLCH themes) and `goya-cue` (colour cueing) are colour-rendering crates. | They are unrelated. The new crate is `goya-chromatics`, contract `GY-CHROMATICS/0.1`. |

## 2. Corrections to the directive

* **§18 "Gestures provides the semantic action vocabulary."** Not in this
  tree (see §1). A Chromatic candidate's intent should name an
  `@interaction`, which is already the admitted, registered unit of
  behaviour, or an `@apps` launch request. The experimental `@intent`
  already binds that way.
* **§25 "Inquisition shall enforce."** Inquisition must be built first, or
  its rules go into existing diagnostics. The second option is the one
  that fits the tree.
* **§23 idle CPU ≤ 0.1 percentage points.** A stock webOS app cannot read
  system CPU (Slice 0: `memorymanager` denied, no `resourcemonitor`). Only
  the engine's own work can be measured on the TV, not the whole system's
  idle load.
* **§26.4 hardware matrix.** The household has one webOS TV (NEON
  XR55N21WEB24, webOS 22) and one stock remote, with no Magic Remote. At
  most, CHR-G08 can report "one device".
* **§11 "before its representation is presented."** A web runtime has no
  presentation acknowledgement. The nearest equivalent is to commit after
  the DOM update and the next `requestAnimationFrame`. The core models this
  as *staged* then *committed* (§3). The target decides what "presented"
  means, and documents it.
* **§9 utility formula.** It is implemented in integer milli-inputs, not
  reals. This is so that Rust and JavaScript compute the same order.
  Fixture values must stay within ±2^53 for the same reason.
* **§19 profile privacy.** As `launcher.md` §2 says, anyone with Developer
  Mode can read app storage. Profile isolation is a correctness property
  here, not a security one.
* **§24 interaction savings.** There is no need to estimate the
  conventional cost by hand. Arachnid knows the focus model of every
  presence configuration at build time, so `cost` (arrow presses + OK to
  reach a candidate) could be computed from it. This is a proposal (§5).

## 3. What was built: `crates/goya-chromatics`

It is a dependency-free library with no clock, no I/O and no target code.
Every number is an integer.

| Directive component (§4) | Code |
| --- | --- |
| Context Manager | `scope.rs`: nested scopes, broad to narrow. A modal may open anywhere. The context key is the `/`-joined path, including the profile. |
| Capability Registry | `model.rs` `Capability`: slots that are Available, Reserved or Unsupported, plus native-equivalence keys. `engine.rs` refuses malformed tables. |
| Candidate Generator | The caller supplies candidates. With no collections in `.gscene` yet, generation is the target's job (§5, D3). |
| Eligibility Evaluator | `eligibility.rs`. It refuses candidates that are unavailable, unauthorised or expired; risky ones with no confirmation workflow; unregistered intents; empty labels; duplicate ids (all copies are refused); duplicate entities; and native equivalents. Every refusal has a reason code. |
| Allocation Engine | `allocate.rs`. Strategies: Manual, Preferential, Ranked, Temporal, Frequent, Positional and Utility. Operational, Severity, Predictive and Hybrid are Utility with the matching terms. A fallback never repeats an entity. Ties break by candidate id. It is a stable top-k, O(n log n). |
| Stability Controller | `allocate.rs`, Chromatic Inertia. Immediate triggers (context, policy, capability) re-resolve but keep slots. Background triggers apply hysteresis (`min_improvement`), `min_residency` and `max_deferral`, and defer while the user is navigating. Invalidation removes bindings at once and leaves the others in place. A freed key is not refilled during navigation. |
| Binding Registry, Interaction Dispatcher | `registry.rs`: staged and committed generations. A press records the meaning its key had at press time. Release executes that meaning or refuses it, and is never redirected. Refusals: transition, revoked, scope-inactive, not-authorised, not-held. Auto-repeat is swallowed, and each press executes at most once. Permission is checked again at execution time. |
| Verification Layer | `generation.rs` `verify`. It re-checks every §25 runtime invariant without trusting the allocator. `Engine::allocate` records nothing when a check fails, so the previous generation stands. |
| Presentation Engine, Preference Store, Telemetry | Not in the core. `Allocation` carries the diagnostics of §27: rejections with reasons, scores, selection and placement provenance, and stability notes. |

**Conformance fixtures** (`fixtures/chromatics/*.chr`, format in its
README) are 12 scripts. They include the directive's own examples: §8.1's
"two favourites, then ranked", §10.4's "same four favourites swap places and
keep their keys", and §6's modal with a single confirm action. They are
plain text, so a JavaScript or Kotlin runtime can run the same files.

Two items on §14's list are **not covered**. Scopes nest, but a scope
cannot yet declare that it inherits its parent's candidates. Accessibility
is covered only as far as requiring a text label.

**Tests:**
* `cargo test -p goya-chromatics`: 13 tests.
* The conformance test runs every fixture. Mutating an expectation makes it
  fail, which was checked on four fixtures.
* Properties (§26.2): 1,500 random cases × 12 steps each. They check
  capacity, available slots only, slot and entity uniqueness, no invalid
  or native-covered binding, independence from candidate order, and no
  change when the input is unchanged.
* Randomised dispatch (§26.3): 400 runs × 200 steps of reallocation,
  staging, revocation, suspension and press/release. It checks that no
  key is redirected, that no revoked or unauthorised binding executes, and
  that nothing executes through a transition.
* Adversarial input: 10,000 candidates that share one id, entity and pin.
* `cargo clippy --all-targets -D warnings` is clean.
* uni on the crate: 93/100, with `--fail-under 80` passing. The traci notes
  that remain are `expect` calls in property tests.

**Host timings** (`cargo run --release -p goya-chromatics --example
chromatics_bench`, Intel Core 5 120U). This is the core's internal
processing only. It includes verification. It is not a webOS measurement.

| Case | p50 | p95 | p99 | §23 target (p95) |
| --- | ---: | ---: | ---: | ---: |
| 100 candidates, utility, background | 68 µs | 84 µs | 86 µs | ≤ 2 ms |
| 1,000 candidates, utility, background | 0.88 ms | 0.95 ms | 0.97 ms | ≤ 10 ms |
| Press → release dispatch | 0.21 µs | 0.23 µs | 0.24 µs | ≤ 1 ms |

The 1,000-candidate case has 10× headroom on this host. A TV's CPU running
generated JavaScript has to be measured on the TV.

## 4. Release gates (§29)

Nothing is production-ready. "Core" means the logic is covered by fixtures
and properties, with no target behind it.

| Gate | State |
| --- | --- |
| G01 Deterministic allocation | Core |
| G02 Context-sensitive reassignment | Core |
| G03 Native input preservation | Core (exclusion, reserved slots). The Arachnid side already leaves unclaimed keys to the platform. |
| G04 Profile isolation | Core (context keys, `Engine::forget`) |
| G05 Presentation/dispatch coherence | Core (staged/committed). No presentation exists. |
| G06 Zero stale execution in fixtures | Core |
| G07 Accessibility and navigation parity | Not started. Needs a target. The core only requires text labels. |
| G08 Physical webOS verification | Not started |
| G09 Reproducible benchmarks | Host only |
| G10 Compiler/resolver conformance | Not started. Only one implementation exists. |
| G11 Security and permissions | Core (risk policy, execution-time check, registered intents only) |
| G12 Zero available inputs | Core |

## 5. Owner decisions needed before Phase 3

1. **Where it lives.** The core crate is on `nebula/v0.1` (shared
   infrastructure, with the resolver). The launcher is on `arachnid/v0.3`.
   Either merge the crate across, or carry Chromatics on `arachnid/v0.3`.
   It has no dependencies, so either way is cheap.
2. **Runtime on webOS.** The recommendation is two lowerings from one model:
   * when every candidate is static, Arachnid precomputes allocations at
     build time, as it already does for presence and `remote-colour`;
   * when a candidate field depends on state, Arachnid emits a generated
     JavaScript port of this crate and checks it in Node against
     `fixtures/chromatics`, as with `gvi_js.rs`.
3. **What a "source" is (§12.2).** `.gscene` has no collections. The options:
   * (a) Candidates are declared in the scene, each naming a button or
     `@interaction`. Rank, favourite and availability are bound from state
     with the existing `…-from` pattern. Bounded, and fits now.
   * (b) A new bounded collection construct, which is a language change.
   * (c) Ranked candidate ids arrive over `@link` from a peer, as Household
     devices do.
   The recommendation is (a) first and (c) for household data.
4. **Intent vocabulary.** The recommendation is that a candidate's intent
   names an `@interaction` or an `@apps` request. It would not depend on
   Gestures, which is about text editing.
5. **Inquisition.** The recommendation is to add the §12.3 static checks as
   goya-lang/Arachnid diagnostics (new `GY-E…`/`GY-AR-6xx` codes) rather
   than wait for a new tool.
6. **`remote-colour`.** Keep it as the pinned or Manual form of a candidate,
   which is backwards compatible, or retire it once `@chromatics` exists.
7. **Conventional cost.** Should `cost` come from Arachnid's focus model
   (§2) rather than from authored numbers?
