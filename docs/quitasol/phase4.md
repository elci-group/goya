# Quitasol Q4 — Goya-native voice primitives (typed slots, categories)

Status: **proposed, not ratified**. Implementation is experimental; it does
not admit, ratify or freeze any profile. Phase 3 record:
[phase3.md](phase3.md); design: [intents.md](intents.md).

## What Q4 delivers

The authoring-side semantic surface proposed in the Quitasol document §3/§4
— the Semantic Intent Compiler's "identify and classify voice-addressable
actions" duty — as Goya-native, platform-independent primitives:

- **Typed slots** — `slots { query: text; limit: int; }` blocks on
  `@intent` (goya-lang compatibility parser, draft surface). Slot *text*
  syntax is unchanged from the pinned grammar `quitasol.utterance.v0`
  (ident `{name}` placeholders); typing lives in the declaration, so the
  grammar identifier does not change. Slot types use the D1 scalar schema
  (`int`/`text`/`bool`, `InteractionType::keyword` added as the closed
  keyword set).
- **Slot symmetry, fail-closed both directions** — every `{slot}` an
  utterance references must be declared (undeclared reference → `GY-E1011`),
  and every declared slot must be referenced by at least one utterance
  (unused declaration → `GY-E1011`). Enforced in both `validate_intents`
  and `validate_intent_decl`, so it propagates to the fail-closed compiler.
- **Semantic categories** — `category: search|playback|navigation|selection|
  settings|confirm|general;` on `@intent`, the closed classification set
  mirroring the proposal's conventional-to-voice equivalence table. Default
  `general`; unknown keywords are parse errors, never silent defaults.
- **Manifest projection** — manifest entries carry `transport`, `category`
  and typed `slots` (deterministic JSON). Contract/profile identifiers are
  unchanged (`QUITASOL-INTENT-IR/1-draft`, `quitasol.intent.v0`); the
  draft stability clause permits shape growth.

## Deliberately out of scope

- **No platform lowering of slots/categories** — Q5 (custom Skills
  generation where supported) is the consumer: an Alexa interaction-model
  fragment would map declared slots to slot types and categories to intent
  groupings. Nothing here claims that mapping.
- **No locale/pluralisation policy** — still open per [intents.md](intents.md)
  §6; slot typing is orthogonal.
- **No runtime slot filling** — no runtime exists (phase 3 boundary
  unchanged).

## BLOCKED (unchanged)

Runtime slot filling, platform lowering, locale policy, owners/ratification
— see [phase3.md](phase3.md) and the Q0 audit for the standing boundaries.

## Verification

```
cargo test -p goya-lang --offline
cargo test -p quitasol-semantic --offline
```

Evidence record: [programme/quitasol-q4.json](programme/quitasol-q4.json).
