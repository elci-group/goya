# GSSH decision register (Aragon Phase 1)

Status: proposed and implemented 17 November 2026; **ratified by the project
owner on 21 November 2026** together with the contract
[`GSSH/2`](GSSH-2.md), which is now Approved. D3's `text:` question was
settled the same day (initial content); D3's display bindings remain
**Open**. D7 (GSSH/3) is decided but its profile is still a Candidate. They resolve the six
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

**Resolved — `@input` `text:` meaning** (project owner, 21 November 2026).
`text:` on an `@input` is the field's **initial content**, as Sash and
Emperor already render it (their admitted counter depends on it). A
placeholder needs its own `placeholder:` property under a later profile.
Implemented by Maja `maja.web.local.v2`. **Known divergence:** the older
GY-LANG/1 → GY-IR lowering (`goya_lang::lowering`, into GY-IR/1's
`Node::Input { placeholder }`, whose shape is frozen) and
`goya_sdk::Project::input_nodes` still present `text:` as a placeholder.
They are not GSSH/2 consumers, but they must move to initial content, or
stop reading `text:` on inputs, in their next contract revision. Aragon
still refuses `text:` on `@input` and may now implement it.

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

## D7 — Proportional lengths (GSSH/3)

**Decision** (project owner, 18 November 2026). Every length in the Goya
ecosystem is proportional, never absolute. This is introduced as the
Candidate profile **GSSH/3** (`handoff::admit_v3`, `parse_strict_gssh3`) and
the ecosystem migrates to it one target at a time.

* GSSH/3 is GSSH/2 with every absolute length rejected with `GY-E1009` at the
  property's exact range: `position`, `extent`, `text-size` and
  `stroke-width`. The error names the replacement.
* Positions and sizes are `position_p` and `extent_p`: fractions of the
  parent's resolved rect (the root's parent is the viewport).
* `text-size-p` is the text's em size as a fraction of the element's own
  resolved height. It must be at least 0 and may exceed 1, in which case the
  text is taller than its element and is clipped like any overflowing text.
* `stroke-width-p` is a fraction of the shorter side of the element's
  resolved rect, clamped to [0, 0.5]. The stroke is drawn inside the
  element's bounds.
* **Overflow rule.** Fractions are clamped to [0, 1] of the parent, except
  directly inside a `scroll` element, where they are only clamped to be at
  least 0. Without this, content could never be larger than its scroll
  viewport once absolute lengths are gone. The rule lives in
  `goya_lang::layout`, which is shared by every profile; no scene in the
  repository used a fraction outside [0, 1] when it was introduced.
* The reference layout resolves both lengths into `DrawNode::text_size_mp`
  and `DrawNode::stroke_width_mp`, in millipoints. They come from the
  proportional form when present, otherwise from absolute points.
* GSSH/1 and GSSH/2 reject `text-size-p` and `stroke-width-p` (`GY-E1006`),
  so a consumer on an older profile never receives a length it would ignore.
* `wrap` is a count of characters, not a length, and is unchanged.

**Alternatives.** (a) Remove absolute lengths from GY-LANG/1 at once: this
breaks 33 of the 64 scene files and Sash, which supports only absolute
geometry today. (b) Apply the rule to Aragon only: the rule is for the whole
ecosystem. (c) Proportional text size relative to the viewport or the
parent: element height was chosen because it keeps text and its box
scaling together.

**Migration.** `goya_lang::proportional::to_proportional` (command:
`goya-proportional <scene> <width-pt> <height-pt>`) rewrites absolute
lengths as fractions that reproduce the reference layout exactly at one
reference viewport, editing only the rewritten property spans. Every scene
file in the repository converts with an identical reference layout at
800×600 and 392×753 points (`crates/goya-lang/tests/proportional_migration.rs`).
The only exceptions, reported rather than guessed, are text sizes on elements
with zero size, which no fraction can reproduce. The per-target status is in
[`proportional-migration.md`](proportional-migration.md).

**Open.** Renderers other than Aragon do not yet read the resolved lengths
(the Wayland renderer still uses its own scale for absolute `text-size`).
Sash must gain proportional geometry before it can move to GSSH/3.

## D8 — Durable record format and concurrent writers

**Decision** (project owner, 21 November 2026; refines D4). Every target
stores a scene's durable state as one record per ontology. The storage
medium is the target's own; the record is shared:

* **Key** `goya.state.v1/<ontology name>`. A key's namespace (D4) is the
  entry inside the record.
* **Text** the JSON object
  `{"format":"goya.state.v1","values":{"<key>":["<type>",<value>]}}` with
  exactly those two members. `int` values are canonical decimal strings
  (JSON numbers cannot hold every i64), `text` values are strings, `bool`
  values are JSON booleans. Only durable keys are written; the revision is
  not.
* **Restore** follows D4. A missing entry yields that key's authored value.
  Every other defect fails the whole restore: an unknown envelope or type
  tag, a value that does not decode under the GSSH/2 conversion rules
  (canonical i64, valid UTF-8 text up to 64 KiB), a type different from the
  declaration, or a restored snapshot above D1's 1 MiB.
* **Write.** A transition that writes a durable key stores the whole durable
  snapshot after the transition in one atomic write, before the new revision
  becomes visible. If the write fails the transition fails and nothing
  changes. Entries of a decodable record that the scene does not declare
  durable are written back unchanged.
* **Concurrent writers.** Each running instance remembers the record text it
  last read or wrote. If the stored text differs at the next durable write
  (another window or process wrote or cleared it), the transition fails with
  a conflict, nothing is written, and the user is told to reload; reloading
  restores the other writer's state. Last-writer-wins is rejected because it
  silently discards another window's committed state.

**Alternatives.** (a) One storage item per key: a transition writing several
durable keys could be torn by a crash. (b) A generation counter in the
record: same protection as comparing text, but changes the format. (c)
Adopting another window's writes live: a state change with no user
interaction, which GSSH/2 has no construct for.

**Implemented** by Maja `maja.web.local.v2` (`localStorage`;
`docs/maja/interactive-profile.md`). Aragon still refuses `durable` (gate
ARG-G07) and must adopt this record when it adds a storage adapter.

## D9 — Display bindings: `text-from` (GSSH/3 live)

**Status: approved** by the project owner, 21 November 2026, including the
wrapping and announcement rules below. Resolves D3's open display-binding
question for GSSH/3-live; GSSH/2 and GSSH/3 are unchanged. The profile
itself stays a Candidate only because its base, GSSH/3, is one.

**Decision.** Adopt the Arachnid proposal's construct
(`docs/arachnid/ARACHNID-1.md` on branch `arachnid/v0.1`), with its syntax,
names and codes, rather than a second design. The project owner chose this
on 21 November 2026. This branch carries only `text-from`:

```text
text-from: state.<key>;
```

* Admitted only by `parse_strict_gssh3_live` / `handoff::admit_v3_live`
  (contract `GSSH/3-live`): GSSH/3 plus live bindings. GSSH/1, /2 and /3
  refuse the property with `GY-E1012` at its range.
* The key must be declared in `@state` (`GY-E1013` otherwise). A key that
  only a display binding uses counts as used (`GY-E1008` is not raised).
* The element shows its authored `text:` (if any) followed by the key's
  current value: `int` as its canonical decimal, `text` verbatim, `bool` as
  `true` or `false`. This matches Arachnid's `display()`.
* The value shown is the committed state: after restore and before the
  scene is first shown; after each committed transition, before the next
  frame; unchanged by a failed or refused transition.
* **Wrapping, all targets** (owner: "follow Arachnid's style, and enforce
  across all other compilers"). An element with `text-from` lays out its
  authored text followed by the value with CSS `white-space: pre-wrap`
  semantics, as Arachnid renders it: spaces and line breaks are kept, lines
  break at the element's edge, overflow is clipped. `wrap` may be combined
  with it; it applies to the authored text as usual.
* **Announcements, all targets, by default** (owner: "define what changed
  and how it changed contextually, not aesthetically"). After each
  committed transition that changes at least one shown value, the target
  makes one polite announcement to assistive technology, without changing
  what is drawn. It names each changed display by its context (the
  element's authored text without trailing blanks and one trailing colon,
  or the state key when that is empty) and says how it changed: `int`
  "`<context>` increased/decreased by `<d>`, from `<old>` to `<new>`";
  `text` "`<context>` changed from "`<old>`" to "`<new>`""; `bool`
  "`<context>` changed from `<old>` to `<new>`"; several joined with "; "
  in depth-first order. Nothing is announced at start, after restore, or
  for a failed transition, which targets report separately. Appearance is
  never described.
* **Reference.** `goya_lang::display_binding` (`shown_text`, `context`,
  `describe_change`, `announcement`) defines these strings; every target
  must produce exactly them, and should test against it as Maja does.

**Compliance by target** (21 November 2026):

| Target | Admits GSSH/3-live | Wrapping | Announcements |
| --- | --- | --- | --- |
| Maja `maja.web.local.v3` | yes | yes | yes (Node differential vs the reference; Chrome and Firefox) |
| Aragon (live scenes, 25 November 2026) | yes (GSSH/4-live) | yes | yes: `announceForAccessibility`, reference wording; checked on an emulator, not yet heard through TalkBack |
| Arachnid (branch `arachnid/v0.1`, uncommitted) | yes | yes (`pre-wrap`) | **no**: must add them before its live profile merges |
| Aragon (GSSH/4 scenes), Sash, Emperor, Neptune, others | no: their profiles refuse `text-from` (`GY-E1012`) | must on adoption | must on adoption |

Non-strict (compatibility) consumers such as the GY-IR lowering parse
`text-from` and ignore it, as they ignore every GSSH/2 declaration.

**Alternatives.** (a) A separate minimal profile
with `text-state:`): faster, but two designs for one construct. (b) Sash's
`state.<key>` element-ID convention: GSSH/2 already refuses inference from
IDs (D3).

**Open.** (a) The rest of the Arachnid proposal (`image-from`,
`present-when`, `host` state, `@link`, motion) is not on this branch and
stays unknown here (`GY-E1001`). (b) Characters that targets render
differently in authored text (Maja refuses U+0000 and carriage return there)
can reach a shown `text` value at run time. (c) Announcement wording is
English only; very long `text` values are announced whole.

## D10 — Platform ambiguity and explicit exceptions (GSSH/4)

*Numbered D9 when first written on 23 November 2026; renumbered because D9
is display bindings.*

**Decision** (project owner, 23 November 2026). An ontology has implicit
platform scope `any`. Each compiler defines its native implementation of
shared semantics. Authors use `@platform <target>` only for an exception to
that default, scoped to one named feature. The GSSH/4 profile adds seven
directives: `only-use`, `choice-use`, `fallback`, `fallbacks`, `must-use`,
`invade`, and `cede`. Their option counts and compiler obligations are in
[GSSH/4](GSSH-2.md#gssh4--platform-ambiguity-and-explicit-exceptions).

Older profiles reject a platform exception (`GY-E1015`). A GSSH/4 compiler
may ignore rules for other platforms, but must resolve or refuse every rule
for its own platform with a source location. A feature name or option string
alone is not evidence that a compiler implements it. Adopting GSSH/4 does
not admit live bindings; their existing profile gate remains separate.

## D11 — Persistence targets (GSSH/4)

**Decision** (project owner, 24 November 2026).

1. **Approved profiles do not change.** New constructs go into the next
   profile, GSSH/4. GSSH/2, GSSH/3 and GSSH/3-live reject `@persistence`
   with `GY-E1015`, as they reject `@platform`. A draft of 23 November had
   made `@persistence` mandatory in every GSSH/2+ ontology; that would have
   changed the approved GSSH/2 contract and invalidated existing scenes, so
   it is withdrawn.
2. **Default to local; support a defined database.** In GSSH/4 the
   declaration is optional. Without it, or for a platform it does not list,
   durable state stays in the platform's default local store. For the
   browser that is `localStorage` (`browser.local-storage.v1`), the store D8
   was approved on. An author may name a defined database instead; the
   browser's is IndexedDB (`browser.indexeddb.v1`). `cache_targets: null`
   disables persistence and cannot be combined with `durable` state
   (`GY-E1014`). The rules are in
   [Persistence targets](GSSH-2.md#persistence-targets).

Follow-up rulings the same day:

3. **Relying on the default warns.** Leaving the store unnamed is allowed,
   but when the scene has durable state the compiler reports a warning
   naming the default it used. Nothing is saved without durable state, so
   such scenes get no warning.
4. **Every platform gets a database, with its own defaults.** Each
   platform keeps its own default local store and has its own database
   adapter (SQLite as each operating system ships it; IndexedDB in the
   browser). See the table in
   [Persistence targets](GSSH-2.md#persistence-targets). Naming an adapter
   is not proof that a compiler implements it: a target that has not built
   a store refuses it.

**Consequences.** A target refuses an adapter name it does not implement,
at the declaration's range. Both browser stores keep the same D8 record
text. With IndexedDB the D8 comparison and the write share one `readwrite`
transaction, so another window cannot write between them; with
`localStorage` a small window remains between the re-read and the write
(Maja gap M4-02). A store change does not migrate an existing record.

**Open.** (a) Implementations of the non-browser stores: no target except
Maja emits durable state yet (Aragon refuses it, gate ARG-G07). (b) Whether
a later profile should migrate records between stores. (c) Persistence
targets in a live profile: resolved by D12 (GSSH/4-live). (d) Persistence keys for
`android_tv` and `tvos`, which `@platform` names but `@persistence` does
not. (`webos` was added on 25 November 2026 for Arachnid: default
`webos.local-storage.v1`, database `webos.indexeddb.v1`.)

## D12 — Live state with persistence: live, staged and source (GSSH/4-live)

**Decision** (project owner, 24 November 2026). An application may show
live values and choose its store at the same time. Following D11, this is
a new profile, `GSSH/4-live` (GSSH/4 plus GSSH/3-live's `text-from`), not a
change to GSSH/3-live or GSSH/4.

State is kept in triplicate: **live** (shown and read by interactions),
**staged** (one proposed change) and **source** (the saved record restored
at the next launch; the project owner confirmed it is the stored data, not
the `.gscene` file). Changes apply to staged, are validated, are saved to
source, and then become live. The project owner chose **save, then show**:
the screen never shows an unsaved value, accepting a short delay while an
asynchronous store (IndexedDB, SQLite) writes. The full rule is in
[Live, staged and source state](GSSH-2.md#live-staged-and-source-state-gssh4-live).

**Consequences.** Maja's D1 runtime exposes the staged step as `prepare`
(validate) with `commit` (become live) and `abandon` (discard); a second
interaction while a change is staged fails with `REENTRANT_DISPATCH`.
Other targets with live bindings (Arachnid) must follow the same order
when they persist state.

## D13 — Migration between persistence stores (GSSH/4)

**Decision** (project owner, 24 November 2026). When an application's
store changes, its saved data is **copied over automatically by default**.
An author may give an explicit different ruling with a strict schema:
`migration: copy|move|none;` for every platform, or a map of platforms to
those words. Rules and errors are in
[Migration between stores](GSSH-2.md#migration-between-stores).

**Consequences.** Migration happens at restore, only when the scene's store
is empty, and only from a store of the same platform; the record is
decoded with the D8 rules before it is copied and is copied unchanged
(entries the scene does not declare stay). The copy writes only if the
scene's store is still empty, so two windows migrating at once cannot
overwrite each other. Maja implements it between `localStorage` and
IndexedDB and marks the page `data-goya-migrated`. Targets without a
second store have nothing to migrate.

## D14 — Accessible identity: `@iden` (GSSH/4)

**Decision** (project owner, 24 November 2026). Add the `@iden` primitive
and enforce it on every text box (`@input`) at compile time, for all
platforms, with general headroom for further accessibility features and
for each platform's native accessibility features. Rules:
[Accessible identity](GSSH-2.md#accessible-identity-iden).

**Design.** The requirement is part of GSSH/4 strict admission
(`GY-E1017`), so every compiler that admits GSSH/4 enforces it without
its own check. Headroom: `goya_lang::Iden` is non-exhaustive, the
accessible name and description are separate fields, and keys later
profiles may define are already reserved and refused precisely. Each
target maps the fields to its native API (table in the contract) or
refuses `@iden` at its range; none may drop it.

**Status by target (24 November 2026).** Browser: implemented and verified
in Chrome 157 (accessibility tree name) and Firefox 150 (DOM). Android:
generated code calls the platform API; not yet run on a device. Android
TV: label carried in the plan. iOS (Neptune): SwiftUI `accessibilityLabel`
and `accessibilityHint` in every element-emitting preview, source-mapped;
Swift not compiled on this host. Windows (Sash): dynamic annotation in the
static-window profile and in the GSSH/4-live counter (whose result is also a
D9 live region); images link reproducibly for x64, x86 and ARM64 on both API
floors and pass `sash verify`, but none was launched. Sash's GSSH/1 counter
cannot carry `@iden` (GSSH/1 refuses it). This closes Maja gap M3-01.

## D15 — Build on the latest profile

**Decision** (project owner, 24 November 2026). GSSH/3 is not taken to
approval; work continues on the latest profile, GSSH/4 (with GSSH/4-live
for live bindings). Maja's `maja.web.local.v2` and `v3` admit GSSH/4 and
GSSH/4-live directly. Sash admits GSSH/4 (static) and GSSH/4-live (the
counter) for `--viewport-dip` scenes and keeps GSSH/1 for its absolute-length
scenes. Neptune and Emperor still try earlier profiles first; Neptune maps
`@iden` where it admits it.
