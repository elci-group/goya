# Quitasol — Overview

Status: **proposed, not ratified**. Quitasol is proposed as Goya's
Fire TV compiler target. This document records the adopted architectural
decisions and the boundaries of what is asserted. No implementation, backend,
packaging or Alexa conformance is claimed.

## 1. Position

| Target | Ecosystem | Interaction priority |
| --- | --- | --- |
| Aragon | Android | Touch-first |
| Nevada | Android TV | Remote-first (proposed; not implemented) |
| Quitasol | Fire TV | Voice-optimised |

Quitasol shares proposed Nevada television infrastructure (spatial
navigation, TV layout, hardware capability resolution) and adds a semantic
interaction layer: translation of application functionality into
voice-addressable actions, with conventional remote navigation preserved.

## 2. Adopted decisions

| Decision | Direction |
| --- | --- |
| Interaction primitive | First-class `intent` declaration. See [intents](intents.md). Extending `gesture` was considered and rejected. |
| Backends | Two independent backends behind a shared Alexa Integration Layer: Fire OS (Android-derived, APK) and Vega OS (Vega SDK, Vega application package). This insulates Goya from Amazon's OS transition. |
| Integration hierarchy | Native platform integration → media (VSK / Media Session) → custom Alexa Skills → contextual interaction → deterministic fallback. Lower tiers do not imply higher tiers. |
| Fallback policy | Voice is additive. Every voice-addressable action has an equivalent remote-driven action. |
| Roadmap discipline | Appstore *policy* audit (certification constraints, VSK requirements) belongs in Q0 with the capability audit, not Q8, so certification constraints are known before surface is built. |

## 3. Subsystems

Eleven subsystems are described in the source proposal. The load-bearing
initial three, in dependency order:

1. **Semantic Intent Compiler** — classify voice-addressable actions.
2. **Utterance Generator** — emit interaction definitions against a pinned
   grammar (see [intents](intents.md) §6, open).
3. **OEM Capability Resolver** — resolve device, OS and remote capabilities.

The Dialogue Manager, Confirmation Engine and remaining subsystems are not
exercisable before native media integration (Q3+) and are treated as
contingent, not committed.

## 4. Boundaries

- Alexa integration availability, certification and invocation rules are
  external constraints; each mechanism must be evaluated separately per OS
  and device profile.
- No runtime conformance, packaging or Appstore submission is asserted.
- Goya gesture behaviour and admission are unchanged.

## 5. Related documents

- [Intent primitive design](intents.md)
- [Gestures grounding](../gestures-grounding.md) — contrast for the
  physical/verbal split
- [Gestures decision register](../gestures-decisions.md) — the G0
  ratification precedent Quitasol proposals follow
