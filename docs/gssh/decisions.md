# GSSH decision register (Aragon Phase 1)

Status: **proposed, implemented, not ratified**, 17 November 2026. The
decisions below are implemented as the Candidate contract
[`GSSH/2`](GSSH-2.md). Architecture approval (`docs/contracts.md`) is still
required before any target relies on GSSH/2 as frozen. They resolve the six
areas of the [GSSH gap ledger](../aragon/gssh-gap-ledger.md). A decision
marked **Open** is not resolved, and scenes that would need it are rejected.

## D0 — Versioning and profile gating

**Decision.** Keep `GSSH/1` (`handoff::admit`, `parse_strict`) exactly as the
GY-LANG/1 strict profile and make it reject every new declaration with
`GY-E1006`. Add `GSSH/2` as a separate entry point (`admit_v2`).

**Alternatives.** (a) Extend GSSH/1 in place: Sash, Emperor, Neptune and
goya-parity already call `admit`, and would silently ignore state and
bindings. (b) A serialized interchange file: no consumer needs one yet, and a
closed decoder can be added later without changing the in-process value.

**Consequences.** No existing consumer changed behaviour (their test suites
pass unchanged). Moving a target to GSSH/2 is an explicit, reviewable step.
The value stays in-process; serialization remains unselected.

## D1 — Adaptive layout

**Decision.** GSSH/2 admits exactly GY-LANG/1 geometry: absolute and
proportional `position`/`extent` in points, clamped by `goya_lang::layout`,
which is the reference evaluator. No anchors, margins, padding, min/max,
intrinsic sizing, responsive rules, safe-area insets or font scaling are
expressible. A target maps one point to one device-independent unit (dp on
Android, DIP on Windows) and rejects any scene its native layout cannot
reproduce, as Sash already does for cells the solver would move or shrink.

**Open.** Responsive layout needs a new profile. Font scaling stays open for
every target.

**Aragon (Phase 3, target decision, not a GSSH change).** One point is one
dp; millipoint rects come from a Kotlin mirror of `goya_lang::layout` that is
differential-tested on the JVM across parent sizes; each pixel edge is
rounded half up on its own, and clipping follows the reference (children
clamped into parents except under `scroll`). Text is not font-scaled. See
`docs/aragon/android-scene.md`.

## D2 — Actions

**Decision.** The GSSH/2 action model is the existing bounded interaction
set: D1 straight-line typed assignments and D2 static-journal requests, each
triggered by `button.<name>.activate`. GSSH/2 specifies the activation
protocol around them ([GSSH-2 §Execution](GSSH-2.md#execution-semantics-a-target-must-implement)):
snapshot the bound fields, convert, then dispatch atomically, with serialized
activations and a reported, state-preserving failure.

**Alternatives.** A visual action graph, named handler functions or
continuations. None has a reference evaluator; adding one is a new profile.

**Open.** D2 requires capability and lifecycle adapters per target; until a
target has them it must reject D2 (Aragon already does).

## D3 — Field bindings

**Decision.** `input { key: type from "<@input id>"; }` binds an interaction
input to the current text of one `@input` element. Every input must be bound
(no host-supplied inputs in GSSH/2). Conversion is fixed by type and has a
reference implementation, `handoff::convert_field`: `int` is the
round-tripping canonical `i64` decimal (identical to Sash's
`parse_canonical_i64`), `text` is verbatim UTF-8 up to the D1 64 KiB limit,
and `bool` cannot be bound. No locale parsing, trimming, normalization or
clamping. A failed conversion aborts the activation without a state change.

**Alternatives.** Implicit binding by position (Sash's current interactive
profile binds its one edit to its one input). That does not scale beyond one
field and cannot be validated. Sash keeps its implicit rule under GSSH/1.

**Open — `@input` `text:` meaning.** GY-LANG/1 lowering and the GY-IR runtime
treat an `@input`'s `text:` as a *placeholder*. Sash and Emperor treat it as
the field's *initial content*, and their admitted counter depends on that.
GSSH/2 does not change either path. Before a GSSH/2 target renders a bound
field, the architecture owner must choose one meaning for GY-LANG/1. The
proposed direction is initial content, because the shipped native targets
already use it; the placeholder reading would then need its own
`placeholder:` property under a new profile.

**Open — display bindings.** No construct shows a state value in an element.
Sash infers it from an element ID `state.<key>`. GSSH/2 does not admit that
convention; it needs an explicit, typed output binding in a later profile.

## D4 — Persistence

**Decision.** Every state key that an interaction uses is declared once in
`@state` with a type, a mandatory initial value and a mandatory lifetime,
`session` or `durable`. Unused declarations are rejected. `durable` values are
restored before the first frame, all-or-nothing. A missing record yields the
authored initial value; an undecodable or retyped record fails restore with a
reported error and is not overwritten. The storage namespace is (ontology
name, state key).

**Alternatives.** Durable by default, or defaults inferred from element text.
Both invent meaning the author did not write.

**Open.** (a) Migrations: GY-HOTSWAP/1 already defines ordered rename, remove
and add-default steps over a versioned `goya_state::Map`. A durable-state
version declaration that feeds that chain is the intended route; until then a
retyped durable key fails restore. (b) Sensitive values: GSSH/2 has no
`secure` lifetime, so protected data cannot be declared. A later secure
lifetime must require secure storage, with no plaintext fallback. (c) The
crash ordering between a visible revision and its durable write is a target
lifecycle obligation (ARG-G07), recorded per target.

## D5 — Provenance

**Decision.** Identity is authored: element and interaction IDs, element ID
plus canonical property name (`element#property`), state key, and interaction
ID plus input key. `handoff::Origin` names a construct by that identity, and
`origin()` resolves it to a byte range in one revision. Every parse
diagnostic carries an exact UTF-8 byte range: from the start of the construct
being parsed through the offending token, or the offending character for
lexical errors. Lowered GY-IR nodes already carry `semantic_id` and
`action_id`, so node → source composes through `origin()`.

**Alternatives.** Child-index paths (they shift on edit) or content hashes
(they change on any edit). Both remain lookup aids only.

**Open.** Quinta editor IDs and a Swift source map keyed by these origins
are target work. Aragon's Kotlin map (`ARAGON-SOURCE-MAP/1`, Phase 3) is
implemented in `crates/goya-sdk/src/kotlin.rs`. The unused `KotlinMapping`
in `crates/neptune/src/source_map.rs` is not that map.

## D6 — Strict rejection

**Decision.** Nothing reaches a target unrepresented. Unknown properties,
templates, duplicates, invalid interactions and missing IDs stay rejected.
Additionally:

* `placeholder:` and `max-length:` stay **compatibility-parser only**; strict
  profiles reject them (`GY-E1001`). They were added outside the contract
  process with no target consuming them.
* The `focus`, `blur`, `select` and `submit` gesture keys (and the
  `GesturePolicy` fields behind them), added by the removed scaffolding with
  no consumer, are removed. They are now syntax errors in every parser mode.
* GSSH/2 declarations are rejected by GSSH/1 (`GY-E1006`).

The corpus (`crates/goya-lang/tests/corpus/`) holds each rejection with its
exact expected span. The mutation tests drop, duplicate and swap every token
and delete every character of each accepted fixture, under both profiles,
asserting determinism, in-bounds char-aligned ranges, known codes, and (for
admitted mutants) the GSSH/2 coverage invariants.
