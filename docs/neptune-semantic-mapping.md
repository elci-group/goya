# Neptune native profile: proposed semantic mapping 0.0.1

Status: design under development; no mapping is admitted or implemented. This document complements `reconnaissance.md` and does not amend GY-LANG/1 or GY-IR/1.

## Admission and frontend boundary

The input is one UTF-8 `.gscene` document parsed by `goya_lang::parse`. A successful parse alone is insufficient for emission: the target must validate all scene fields and interactions, and reject anything whose meaning it cannot preserve. In particular, a parser default cannot establish that a field was absent. Source ranges and a representation retaining authored properties must be added to the canonical frontend before a validator can reliably reject every unsupported declaration. Resource imports, TOV resolution, and platform configuration need explicit contracts before they can be admitted.

Semantic IDs must include a canonical project-relative module identity and an unambiguous declared node identity. Duplicate names are currently allowed by scene lowering; reject ambiguous identifiers in this profile until a stable identity contract exists. Never use absolute paths or traversal-assigned `NodeId` values as persistent semantic identities. Carry byte ranges from the lexer through AST, shared lowering, and target lowering; map each generated Swift range back to all contributing semantic IDs and source ranges.

## Candidate mappings

| Source meaning | Target representation and observable events | Failure, cancellation, and oracle |
|---|---|---|
| Static text in a cell | Native SwiftUI text, with the authored string as content and accessible name. Preserve child order. Dynamic Type is permitted only when the source layout contract admits expansion. | Reject styles and geometry without an established equivalent. No asynchronous work to cancel. Compare content and order with the canonical scene executor and inspect VoiceOver. |
| Button activation | Native button dispatches one typed action to the authoritative state owner; evaluating the view never dispatches. The accessible name comes from an explicitly admitted label. | Reject unbound or ambiguous actions; do not produce an inert button. Compare ordered action/state traces with the reference executor, including repeated view evaluation. |
| Single-line input | Native text input projects one explicitly declared text state cell. Editing updates that state through typed actions; focus and accessibility are native. | Current `@input` only supplies placeholder and host-side state, so binding requires a shared-language contract. Exercise composition, focus, keyboard dismissal, and restoration on device. |
| D1 assignments | A single state owner evaluates the admitted typed program against a snapshot and publishes the complete result atomically. Integer values are signed 64-bit; addition is checked. | On overflow or type failure preserve the prior state and expose a typed error according to the Goya executor contract. No silent defaults. Compare normalized event and state traces against `CompiledInteraction::execute` with independent fixtures. |
| Navigation | A typed route in application state drives native presentation. Back gestures dispatch the corresponding route transition through the same owner. | Currently no canonical source contract. Reject navigation until that contract and back-gesture semantics exist. Verify forward/back traces and restoration. |
| List | Ordered source collection with stable declared item identity drives native list rows. | Currently no canonical list or identity contract. Reject until it exists; test insertion, deletion, duplicate IDs, and reordering. |
| Persistence | An explicitly declared adapter persists one admitted state value and restores it before the dependent view is shown. Persist after committed changes; termination need not deliver a final callback. | Currently no canonical persistence declaration. Define encoding, missing/corrupt value behavior and write failures before admission; test forced termination and relaunch. |

## Target policy and evidence

The first target is native SwiftUI on an explicitly configured minimum iOS version. The deployment version and Apple toolchain must be recorded after an available worker is inspected; no version is chosen by this document. Native color and text policies need explicit source semantics before light/dark and Dynamic Type can be claimed. All unsupported properties, interactions, capabilities, and platform requirements are fatal diagnostics with a source location and reason.

An admission fixture must contain source, declared inputs, an independently specified expected state/event trace, and either expected diagnostics or native behavior. A passing source-only check reports its own validation level. Apple compilation, simulator execution, and physical-device behavior require their own recorded evidence. Deterministic generated source requires byte comparison across three clean runs with different enumeration order; this does not establish binary or signed artifact reproducibility.

## Implementation prerequisites

1. Extend the canonical frontend with bounded parsing, authored-property preservation, byte ranges, and stable identity; retain compatibility with existing callers.
2. Specify and implement shared language constructs for state binding, navigation, list identities, accessibility, persistence, and restoration. Add a reference execution path independent of the Swift lowering.
3. Add exhaustive validation and a typed target IR. Implement structured Swift emission only for mappings with a defined oracle.
4. Add a versioned compiler CLI and reports, then a pinned Mac worker and physical-device evidence. Until these exist, all corresponding release gates are BLOCKED.
