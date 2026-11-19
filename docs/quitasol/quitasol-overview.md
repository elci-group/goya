# Quitasol — Overview

Status: **proposed, not ratified**. Quitasol is proposed as Goya's
Fire TV compiler target. This document records the adopted architectural
decisions and the boundaries of what is asserted. No implementation, backend,
packaging or Alexa conformance is claimed.

## 1. Position

| Target | Ecosystem | Interaction priority |
| --- | --- | --- |
| Aragon | Android | Touch-first |
| Nevada | Android TV | Remote-first (phase 0 exists: `crates/nevada`) |
| Quitasol | Fire TV | Voice-optimised |

Quitasol shares the Nevada television infrastructure (spatial navigation,
TV layout, hardware capability resolution — `nevada.focus/0.0.1`,
`nevada.profiles/0.0.1`, `nevada.dcr/0.0.1`) and adds a semantic
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
   Draft implemented (phase 0).
2. **Utterance Generator** — emit interaction definitions against the
   pinned `quitasol.utterance.v0` grammar. Validated with typed slots and
   semantic categories (Q4); generation of platform interaction definitions
   (Alexa skill-model fragments) is Q5 (see [phase4](phase4.md)).
3. **OEM Capability Resolver** — resolve device, OS and remote capabilities.
   Draft implemented over `nevada::profiles` + `nevada::dcr` (Q1); real
   device censuses are BLOCKED (see [phase1](phase1.md)).

The **Remote Fallback Engine** is also draft implemented (Q1): remote
fallback targets must be D-pad-reachable (`nevada::focus`), `QS-E2002`.
The **Fire OS integration plan** (Q2,
`QUITASOL-FIREOS-PLAN/1-draft`) emits the documented Media Session
transport mappings and permission entries for transport-declaring intents,
recording everything else `Unresolved` (see [phase2](phase2.md)).
The **MediaSession runtime glue** (Q3, `QUITASOL-MEDIASESSION-RT/1-draft`)
lowers the plan to deterministic Kotlin callbacks over an app-supplied
intent bridge; callback conflicts are compile-time refusals (`QS-E2005`),
and the glue has never been compiled (see [phase3](phase3.md)).
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
- [Phase 0 record](phase0.md) — intent primitive
- [Phase 1 record](phase1.md) — capability resolution over Nevada
- [Phase 2 record](phase2.md) — Fire OS integration emission
- [Phase 3 record](phase3.md) — MediaSession runtime glue
- [Phase 4 record](phase4.md) — typed slots, semantic categories
- [Q0 capability audit](q0-capability-audit.md) — what Amazon documents
- [Gestures grounding](../gestures-grounding.md) — contrast for the
  physical/verbal split
- [Gestures decision register](../gestures-decisions.md) — the G0
  ratification precedent Quitasol proposals follow
