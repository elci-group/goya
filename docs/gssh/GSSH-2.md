# GSSH/2 — Goya shared semantic handoff

Contract identifier: `GSSH/2`. Status: **Approved** by the project owner,
21 November 2026 (Candidate from 17 November). Targets may cite it as a
frozen contract (`docs/contracts.md`); changes need a new profile or a
recorded decision. GSSH/3 below is still a Candidate. Decisions and their rationale are in
[`decisions.md`](decisions.md). Implementation: `crates/goya-lang`
(`parse_strict_gssh2`, `handoff::admit_v2`); conformance corpus:
`crates/goya-lang/tests/strict_corpus.rs` and `tests/corpus/`.

GSSH is how a target compiler (Aragon, Sash, Emperor, Neptune, Maja) receives
a `.gscene`. It is an in-process Rust value, not a file format. It is not a
second language: the source of meaning is GY-LANG/1 plus the declarations
below, and the handoff carries the parsed scene, its source entries and its
declarations from one parse of one source revision.

## Profiles

| Profile | Entry point | Admits |
| --- | --- | --- |
| `GSSH/1` | `handoff::admit`, `parse_strict` | The GY-LANG/1 strict profile. Any GSSH/2 declaration is rejected with `GY-E1006`. |
| `GSSH/2` | `handoff::admit_v2`, `parse_strict_gssh2` | Everything GSSH/1 admits and checks, plus `@state` declarations and `from` input bindings, with the coverage rules below. |
| `GSSH/3` | `handoff::admit_v3`, `parse_strict_gssh3` | GSSH/2 with proportional lengths only: absolute `position`, `extent`, `text-size` and `stroke-width` are rejected (`GY-E1009`); `text-size-p` and `stroke-width-p` are admitted. See [GSSH/3](#gssh3--proportional-lengths) and decision D7. |

GSSH/1 exists so that consumers written before GSSH/2 never receive state or
bindings that they would silently ignore. A consumer moves to GSSH/2 by
calling `admit_v2` and implementing, or explicitly rejecting, every GSSH/2
declaration. Calling `admit_v2` and ignoring a declaration is a contract
violation.

Both profiles retain everything strict GY-LANG/1 already guarantees: unknown
properties (`GY-E1001`), templates (`GY-E1002`), duplicate canonical
properties (`GY-E1003`), invalid interactions (`GY-E1004`) and missing or
duplicate authored IDs (`GY-E1005`) are rejected.

## Grammar additions (GSSH/2)

```text
@state {
  <key>: int  = <canonical decimal> session|durable;
  <key>: text = "<string>"          session|durable;
  <key>: bool = true|false           session|durable;
}
```

* At most one `@state` block, directly in the ontology body. Nested or
  repeated blocks are syntax errors (`GY-E1000`). Keys are unique.
* The initial value is mandatory and must match the type. Integers use the
  canonical decimal form (`0`, `-12`; not `+1`, `01`, `-0`, `1_000`).
* The lifetime is mandatory. There is no default.

```text
input { <key>: int|text from "<@input element id>"; }
```

* `from` binds an interaction input to the current text of one authored
  `@input` element.

The compatibility parser (`parse`, `parse_with_source_map`) accepts the same
syntax and records it in `ParsedScene::declarations`; it does not validate
coverage.

## Admission rules (GSSH/2)

Checked after the GSSH/1 rules, in this order:

1. **Bindings** (`GY-E1007`, range: the binding): the target ID names an
   element; that element is an `@input`; the input type is not `bool`; a
   field is bound at most once per interaction.
2. **Input coverage** (`GY-E1007`, range: the input key): every input of every
   interaction has exactly one binding. Host-supplied inputs are not part of
   GSSH/2.
3. **State coverage** (`GY-E1008`): when any interaction reads or writes
   state, `@state` declares every such key (range: the `read`/`write` key) with
   the same type (range: the declaration); every declared key is used by an
   interaction (range: the declaration); an empty `@state` block is rejected.
   A scene with no interactions needs no `@state`.

## Diagnostics

Every rejection is a `StrictParseError { code, name, range }`. `range` is a
half-open UTF-8 byte range in the exact source:

* `GY-E1000` (syntax) spans from the start of the construct being parsed (a
  property, declaration, state entry, input key or gesture key) through the
  offending token. A lexical error spans the offending character, or an
  unterminated string through end of input. End-of-input errors end at the
  source length. Line-only ranges are no longer produced.
* `GY-E1001`–`GY-E1003` span the property or declaration entry.
* `GY-E1006`–`GY-E1008` span the construct named above.

`ParseError` from the compatibility parser carries the same `range`.

## Handoff value

`SemanticHandoffV2` (from `admit_v2`) exposes:

| Accessor | Meaning |
| --- | --- |
| `contract()` | `"GSSH/2"` |
| `parsed()` | `ParsedScene`: scene, sorted source entries, SHA-256 of the source, declarations |
| `state()` | `StateDecl { key, value_type, initial, lifetime, range }` in source order |
| `initial_state()` | Authored initial snapshot, key → value |
| `durable_keys()` | Keys a storage adapter must persist |
| `bindings()`, `bindings_for(name)` | `InputBinding { interaction, input, value_type, field_id, range }` |
| `origin(&Origin)`, `origins()` | Source range of a construct named by identity |

## Identity and provenance

Identity is authored, never positional. Child-index `scene_path`s and source
ranges change with edits and are lookup aids only.

| Origin | Key | Unique because |
| --- | --- | --- |
| `Element(id)` | authored `id` | `GY-E1005` |
| `Property { element, property }` | element ID + canonical name (`-` spelled `_`); `property_id()` renders `element#property` | `GY-E1003` |
| `Interaction(id)` | authored interaction `id` | `GY-E1005` |
| `State(key)` | `@state` key | duplicate keys are syntax errors |
| `Binding { interaction, input }` | interaction ID + input key | input keys are unique per interaction |

Lowered GY-IR nodes already carry `semantic_id` and `action_id`
(`NodeRecord`); composing them with `origin()` gives node → source range. The
corpus test `origins_survive_edits_that_shift_positions_and_paths` checks
that inserting an element moves ranges and shifts child paths while every
leaf origin still names the same source text.

## Execution semantics a target must implement

These are normative for any target that admits GSSH/2. The reference
evaluators are the canonical D1 compiler/executor and
`handoff::convert_field`.

1. **Initial state.** At first start, state is exactly `initial_state()`.
   Nothing is defaulted, inferred or read from element text.
2. **Activation.** Activating a button that triggers a D1 interaction takes
   one snapshot of the current text of every field bound by that interaction,
   converts each with `convert_field`, and only if all succeed dispatches the
   D1 transition with those inputs. A conversion failure or any D1 failure
   leaves state and revision unchanged and is reported to the user with the
   field (for conversion) or the D1 error code. Activations are serialized;
   an activation that arrives while one is being applied is not interleaved.
3. **Conversion** (`convert_field`): `int` accepts exactly the text that
   parses as `i64` and prints back identically (the rule Sash's interactive
   edit already uses); `text` is verbatim UTF-8 up to 64 KiB with no
   normalization or trimming; `bool` is never bound.
4. **Lifetimes.** `session` values are never written to storage. `durable`
   values are restored before the first frame. Restore is all-or-nothing: a
   missing record restores the authored initial value; a present record that
   fails to decode or has a different type fails restore with a reported
   error, and the stored record is not overwritten until the user acts.
   Storage namespace is (ontology name, state key). Commit ordering between
   the visible revision and the durable write is a target lifecycle obligation
   (Aragon gate ARG-G07); GSSH/2 does not define migrations (see decision D4).
   The record format and the concurrent-writer rule are fixed by
   [decision D8](decisions.md#d8--durable-record-format-and-concurrent-writers).
5. **Field content.** An `@input`'s `text:` is the field's initial content
   at every start, never a placeholder (decision D3). It is not state: it is
   not restored, and edits are lost when the application restarts. A target
   that cannot show it exactly (for example, line breaks in a single-line
   field) refuses the scene.

## GSSH/3 — proportional lengths

Candidate, 18 November 2026 ([decision D7](decisions.md#d7--proportional-lengths-gssh3)).
Everything in this document applies, plus:

```text
text-size-p: <number>;     // em size, fraction of the element's height, >= 0
stroke-width-p: <number>;  // fraction of the element's shorter side, [0, 0.5]
```

* After the GSSH/2 rules, the first absolute length in source order
  (`position`, `extent`, `text-size`, `stroke-width`, in any spelling) is
  rejected with `GY-E1009`, with the range of that property.
* GSSH/1 and GSSH/2 reject `text-size-p` and `stroke-width-p` with
  `GY-E1006`.
* `position_p` and `extent_p` fractions are clamped to [0, 1] of the parent,
  except directly inside a `scroll` element, where only negative values are
  clamped (to 0). This rule is part of `goya_lang::layout` for every profile.
* `goya_lang::layout` resolves `DrawNode::text_size_mp` and
  `DrawNode::stroke_width_mp`.
* `SemanticHandoffV2::contract()` returns `"GSSH/3"` for a handoff admitted
  by `admit_v3`.

## GSSH/3-live — display bindings

Candidate, 21 November 2026; its construct is approved
([decision D9](decisions.md#d9--display-bindings-text-from-gssh3-live)).
Entry points `parse_strict_gssh3_live` and `handoff::admit_v3_live`;
`contract()` returns `"GSSH/3-live"`. Everything in GSSH/3 applies, plus:

```text
text-from: state.<key>;   // show a declared key's value after the element's text
```

Every earlier profile refuses `text-from` with `GY-E1012`; an undeclared key
is `GY-E1013`. Values are shown as canonical decimal (`int`), verbatim
(`text`) or `true`/`false` (`bool`), always the committed state. The other
constructs of the Arachnid live proposal are not part of this branch.

Normative for every target that admits GSSH/3-live:

* The element's authored text and value wrap with `white-space: pre-wrap`
  semantics (spaces and line breaks kept, lines broken at the element's
  edge, overflow clipped).
* Each committed transition that changes shown values is announced once,
  politely, to assistive technology, describing what changed and how (not
  how it looks), exactly as `goya_lang::display_binding::announcement`
  words it. Start, restore and failed transitions are not announced.

## Not in GSSH/2

Explicitly outside this contract; scenes cannot express them, and targets
must not invent them: adaptive layout beyond GY-LANG/1 absolute and
proportional geometry; display bindings from state to element text;
host-supplied inputs; `bool` field bindings; D2 effects at targets that lack
capability adapters; sensitive or secure storage; durable-state migrations;
`placeholder:` and `max-length:` (compatibility parser only).
