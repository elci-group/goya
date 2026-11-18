# Quitasol — Intent primitive design

Status: **proposed, not ratified**. This document admits no schema, target,
compilation backend or release. It adopts an `intent` primitive as the
authoring-level unit of voice interaction for Quitasol, and records the
decision against the alternative of extending the Gestures architecture. No
implementation, Alexa conformance or runtime behaviour is asserted.

## 1. Decision

| Decision | Adopted direction | Rejected alternative |
| --- | --- | --- |
| Authoring primitive | A first-class `intent` declaration, orthogonal to `gesture`. | Extending `gesture` with a `voice { }` block. |
| Rationale | Voice intents share no structure with gestures: no spatial grounding, no focus state, no continuous input stream. Coupling them would bind a verbal abstraction to a physical one. An `intent` maps 1:1 to Alexa intent models, Siri Shortcut intents and Assistant App Actions without carrying gesture semantics. | Gestures exist to route *physical* interaction through spatial and focus state (see [gestures-grounding](../gestures-grounding.md)). A `voice { }` block on a gesture would inherit assumptions it cannot honour. |
| Platform independence | The primitive's semantics are defined against Goya, not Amazon. Quitasol lowers intents to Fire OS and Vega OS mechanisms; other conversational targets (Siri, Google Assistant) are future lowerings of the same primitive. | An Alexa-shaped authoring surface. |
| Fallback | Every `intent` declaration must state a non-voice equivalent (`remote`, `keyboard` or a reference to an existing gesture). An intent with no resolvable fallback is rejected at admission. | Optional fallback. Voice is an additive interaction mode; functionality must survive its absence. |

## 2. Proposed syntax (illustrative, not grammar)

```
action searchCatalogue(query: String) {
    catalogue.search(query)
}

intent SearchCatalogue {
    action: searchCatalogue

    voice {
        utterance: "Find {query}"
        utterance: "Search for {query}"
        utterance: "Show me {query}"

        confirmation: false
    }

    fallback {
        remote: searchPanel
        keyboard: searchField
    }
}
```

Differences from the earlier gesture-shaped proposal:

- `gesture SearchCatalogue` → `intent SearchCatalogue`; no `intent:` field,
  since the declaration itself is the intent.
- No spatial/focus properties appear or are permitted.
- The `fallback` block is required, not illustrative.

## 3. Relationship to Gestures

The intent primitive is defined as a **contrast** to, not an extension of, the
Gestures architecture:

| Aspect | Gesture | Intent |
| --- | --- | --- |
| Input channel | Physical (pointer, remote, keyboard) | Verbal, lowered to platform speech interfaces |
| Grounding | Spatial — resolves against focus, hit targets, layout | Semantic — resolves against application state and declared utterances |
| Continuity | Supports continuous input (drag, scroll streams) | Discrete; utterances are events, not streams |
| State dependence | Editor/focus lifecycle epochs | Application-level state and context resolution |
| Shared infrastructure | — | Both consume the same action binding and capability resolution layers |

Intents may reference gestures as fallbacks (`remote: searchPanel` above may
name a gesture), but an intent never *is* a gesture and cannot gain gesture
properties.

## 4. Lowering map (illustrative)

Quitasol's Semantic Intent Compiler lowers a declared intent according to the
[platform integration hierarchy](quitasol-overview.md). The primitive is not
defined in terms of any one of these; availability is per-OS and per-device.

| Lowering | Mechanism | Scope |
| --- | --- | --- |
| Native platform | System-provided voice interfaces, app launching, supported navigation | Highest tier; no certification of app-specific intents |
| Media | Video Skills Kit / Media Session integration | Playback, transport, catalogue discovery |
| Custom Skill | App-specific intent registration | Subject to Amazon invocation rules and certification |
| Deterministic fallback | The required `fallback` block, compiled to remote-driven interaction | Always emitted; required by §1 |

An intent may lower to *at most one* of the first three tiers plus the
fallback; conflicting lowerings are a compile-time diagnostic, not a runtime
choice.

## 5. Consequential actions

The `confirmation` field is a declaration, not a guarantee. The Confirmation
Engine treats any intent marked `confirmation: true` (or classified as
consequential — purchases, sharing, destructive state changes) as requiring
explicit authorisation at runtime. In phase 0 the field is parsed and
recorded on `IntentDecl` (default `false`) with no runtime; the Confirmation
Engine does not exist.

## 6. Open decisions (BLOCKED)

These remain unresolved and block admission:

**Resolved for phase 0 (IR representation).** Phase 0 keeps intents as a
side-collection on the goya-lang `Scene` (GY-INTENT/1-draft) and emits a
standalone draft manifest via `quitasol-semantic` (QUITASOL-INTENT-IR/1-draft,
profile `quitasol.intent.v0`); `goya-ir` is untouched (GY-IR/1 frozen). A
permanent IR home for intents remains open and needs a contract revision.

- **Utterance grammar.** Placeholder syntax (`{query}`) needs a pinned,
  versioned grammar with slot typing and escaping rules before any generator
  is admitted.
- **Language policy.** Utterance locales, pluralisation and localisation
  requirements.
- **Owners.** Named owners and ratification, per the Gestures G0 precedent.
- **Fixture corpus.** F-series fixtures covering invalid fallbacks, conflicting
  lowerings, slot type mismatches and unresolvable platform tiers.
- **Privacy.** What utterance and slot data may leave the device; per the
  Privacy Enforcement Engine, nothing is asserted here.

## 6a. Phase 0 status

Phase 0 delivers an experimental surface, not a ratified profile:

- **Parser arm.** `@intent` is parsed by the goya-lang compatibility parser
  only; the scene carries `IntentDecl`. All strict GSSH/1/2/3 profiles reject
  `@intent` with `GY-E1010`.
- **Grammar v0 validator.** `goya_lang::intents` validates intents against
  the pinned grammar `quitasol.utterance.v0` (`GY-E1011` diagnostics).
- **Semantic manifest.** `quitasol-semantic::compile` emits a deterministic
  versioned intent manifest (contract QUITASOL-INTENT-IR/1-draft, profile
  `quitasol.intent.v0`, per-intent lowering tier `unresolved`).
- **Tests.** goya-lang intent corpus with mutation sweep; strict-reject
  fixtures under all three GSSH profiles.

Still BLOCKED: platform lowerings, confirmation runtime, owners/ratification,
locale policy, and fixtures beyond the phase-0 corpus. See
[phase0.md](phase0.md) and [../contracts.md](../contracts.md)
(GY-INTENT/1-draft).

## 7. Non-goals

- No claim that arbitrary Goya actions can be exposed as native Alexa
  commands; only documented Fire TV mechanisms are lowering targets.
- No runtime, packaging or Appstore conformance is asserted.
- No change to existing gesture behaviour, parsing or admission.
