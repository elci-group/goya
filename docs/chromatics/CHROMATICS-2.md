# Chromatics: scope inheritance and the accessibility floor (Phase 2 closed)

Status: **the two §14 coverage gaps recorded in CHROMATICS-0 §3 are
closed**, 26 November 2026, branch `Lantern`, uncommitted. There is still
**no Arachnid integration, no presentation engine and no device run**. No
release gate is claimed (§6).

CHROMATICS-0 recorded two items on §14's shared-model list the core did not
cover: *context inheritance* and *accessibility beyond a text label*. This
record closes both, entirely inside the shared core and fixtures — no
owner decision was needed.

## 1. Declared scope inheritance (§6)

A scope may now declare that it *inherits* its parent's effective pool:

* `Scopes::enter_inheriting` declares the policy; `Scopes::enter` keeps the
  previous behaviour (the scope replaces its parent's pool, as Show Details
  replaces Television Shows). Nesting and activation rules are unchanged.
* `compose(own, inherited)` is the whole semantics: the scope's own
  candidates first, the parent's effective pool filling the rest without
  repeating a candidate id. Declarations fold outward in, so an inheriting
  modal sees the whole chain beneath it. Allocation re-sorts the pool, so
  composition order decides nothing except id conflicts, where the inner
  scope wins.
* Verification is untouched: an inherited candidate is an ordinary
  candidate; eligibility, permissions, generations and dispatch apply to it
  identically.

Fixture grammar gains `push NAME [inherits]` / `pop`. Fixture scopes are
views (which nest). `candidate`/`set`/`remove`/`clear` author into the
innermost open scope's own pool; `resolve` allocates over the composed pool
of the active scope; `pop` restores the parent's pool and context, and a
flat `context` command is refused while a scope is open (one source of
truth). New fixture `13-inheritance.chr` covers the §6 chain — Shows →
Details (inherits) → Modal (inherits), each pop re-resolving the parent
with the engine's history intact — and the non-inheriting confirmation
modal that reduces the allocation to one action and leaves the other
inputs unassigned.

The `.gscene` grammar is unchanged: scenes have no view constructs yet, so
inheritance remains engine-and-fixture surface (as recorded in
CHROMATICS-1 §3). The future per-view grammar declares it against this
exact behaviour.

## 2. Accessibility floor (§21)

* `Candidate` and `Binding` now carry optional `symbol` (semantic symbol or
  marker shape) and `description` (accessible description), so the §7
  presentation metadata rides the candidate into the published generation
  and no target has to re-derive it.
* `verify` refuses a generation containing a binding whose label is blank
  (`Violation::Unlabelled`), re-checking the §21 floor without trusting the
  allocator. It also treats a drifted `symbol`/`description` as a
  `BindingMismatch`, like any other candidate field.
* Fixture grammar gains `symbol=` / `describe=` candidate keys and
  `expect-symbol` / `expect-describe`; fixture `14-accessibility.chr`
  covers pass-through, absence, the `empty-label` rejection and a
  dispatchable labelled binding.
* The `.gscene` candidate gains `symbol:` (an identifier) and `describe:`
  (a non-empty string; empty is a parse error). The canonical lowering
  passes both through unchanged.

## 3. What was deliberately not done

* **Presentation (Phase 4).** The core carries metadata; drawing markers,
  shapes and command strips remains the target's job. `symbol` is an open
  identifier on purpose: a closed inventory is presentation-layer policy.
* **Inheritance in `.gscene`.** Waits for view constructs, with the
  Arachnid work (CHROMATICS-0 §5, decisions 1/2).
* **Auditory announcements.** §21 lists them as *supported* presentation
  mechanisms; the core exposes the text the target would announce
  (`label`, `description`) and nothing more.

## 4. Verification

* `cargo test -p goya-chromatics`: 16 tests (13 → 16: unlabelled
  publication refusal, accessibility metadata pass-through, inheritance
  composition and nesting) plus the conformance suite, now 14 fixtures.
* `cargo test -p goya-lang`: 126 unit + 31 integration tests, including
  the corpus (8 tests: accessibility metadata reaches the published
  binding; empty `describe` refused).
* Fixture `13` exercises push/pop with the registry untouched; every
  pre-existing fixture passes unchanged (flat `context` behaviour is
  bit-identical).

**END OF RECORD**
