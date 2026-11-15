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

### Experimental static column

`preview-static-column` is a source preview for a tree of static `@cell` nodes: at most 32 cells and 16 levels. Authored text is emitted with `Text(verbatim:)` before that cell's children. Sibling order follows the scene and must match the shared IR text sequence. Every cell needs an authored id. The profile rejects buttons, inputs, interactions, and every visual property other than `id` and `text`.

The generated `VStack` keeps that child order. It is not the layout solver's geometry, and it does not admit Dynamic Type, color, safe area, or VoiceOver behavior. Apple compilation and device execution are not run. The one-cell `preview` command remains the narrower source preview.

### Experimental root-button preview

This candidate accepts a single root `@button` with an authored ID and a nonempty literal `text`, no children or other visual properties, and exactly one bound D1 interaction with no inputs. Initial state is supplied explicitly through the state-model transport and validated against the shared Goya application owner. The shared IR root must be the same button and its action must match the verified interaction. All other nodes, properties, interactions, and effects are rejected.

The candidate uses one native SwiftUI button occupying the available native content area. Its visible text supplies its accessible name. One activation synchronously dispatches the matching typed action on the main actor. On success the new state is published atomically; on failure the previous state remains and a visible error is shown. The view never dispatches during recomputation. SwiftUI supplies native control interaction and text scaling, but safe-area, Dynamic Type, accessibility, and actual layout behavior remain unverified. There is no input, navigation, persistence, or background work in this candidate.

`preview-observable-button` keeps that admission and adds a readout of each admitted state cell, in UTF-8 key order, followed by the revision. Integers and the revision are decimal text. Booleans are `true` or `false`. Text is the stored characters. The readout is bound to the published state, so a failed action leaves the previous text in place. The view reads state while it is evaluated and dispatches only from the button action. `preview-native-button` does not include the readout. Both commands remain source previews until pinned Apple compilation and the simulator and device fixtures pass.

### Experimental observable column

`preview-observable-column` uses mapping `neptune.swiftui.observable-column/0.0.1`. It admits static text cells and exactly one button with an authored id, a nonempty literal label, and one input-free D1 action. The tree holds at most 32 nodes and 16 levels. Authored text is emitted with `Text(verbatim:)` before that cell's children, and sibling order must match the shared IR. The readout follows the column: each admitted state cell in UTF-8 key order, then the revision, with the same encodings as the observable button. A failed action leaves the previous readout. The generated `VStack` records child order. Layout fidelity, accessibility, Apple compilation, and iOS admission stay unverified.

### Experimental observable buttons

`preview-observable-buttons` uses mapping `neptune.swiftui.observable-buttons/0.0.1`. It admits static text and at least two buttons in the same node and depth limits. Each button has an authored id, a nonempty literal label, and one input-free D1 action. The button calls the state-owner method bound to that authored id. Method numbers follow authored action-id order, so they can differ from child order. The shared IR action for each button must match the compiled D1 trigger. The readout follows the column with the same encodings. A failed action leaves the previous readout. The generated `VStack` records child order. Layout fidelity, accessibility, Apple compilation, and iOS admission stay unverified.

### Experimental button trace

`trace-observable-buttons` accepts a scene that `preview-observable-buttons` accepts, plus an ordered list of at most 32 admitted action ids. It runs that list on `goya_sdk::D1Application` and records the initial state and every activation. Each step includes the outcome, the state, the revision, and the generated button method. A failed action records an error class and keeps the previous state and revision. The command checks that the generated source contains one call to each admitted method from a button. It does not execute that source, and it does not measure view re-evaluation. Apple compilation and iOS admission stay unrun.

`trace-observable-column` accepts a scene that `preview-observable-column` accepts, plus the same activation list. The single button's generated method is `dispatch0`. The same host record applies: outcome, error class, state, revision, and the previous state kept on failure. It checks that the generated source contains one button call to that method. It does not execute Swift or measure view re-evaluation. Apple compilation and iOS admission stay unrun.

The observable trace to compare on an Apple worker is the ordered sequence of activations, action outcome (including error class), state values, and revision against `goya_sdk::D1Application` initialized with the same explicit state. Repeated view evaluation must not change that trace. A generated-source snapshot cannot establish semantic or accessibility conformance. Until pinned Apple compilation and the relevant simulator and device fixtures pass, the candidate remains an experimental source preview and is not admitted for an iOS build.

The first target is native SwiftUI on an explicitly configured minimum iOS version. The deployment version and Apple toolchain must be recorded after an available worker is inspected; no version is chosen by this document. Native color and text policies need explicit source semantics before light/dark and Dynamic Type can be claimed. All unsupported properties, interactions, capabilities, and platform requirements are fatal diagnostics with a source location and reason.

An admission fixture must contain source, declared inputs, an independently specified expected state/event trace, and either expected diagnostics or native behavior. A passing source-only check reports its own validation level. Apple compilation, simulator execution, and physical-device behavior require their own recorded evidence. Deterministic generated source requires byte comparison across three clean runs with different enumeration order; this does not establish binary or signed artifact reproducibility.

## Implementation prerequisites

1. Extend the canonical frontend with bounded parsing, authored-property preservation, byte ranges, and stable identity; retain compatibility with existing callers.
2. Specify and implement shared language constructs for state binding, navigation, list identities, accessibility, persistence, and restoration. Add a reference execution path independent of the Swift lowering.
3. Add exhaustive validation and a typed target IR. Implement structured Swift emission only for mappings with a defined oracle.
4. Add a versioned compiler CLI and reports, then a pinned Mac worker and physical-device evidence. Until these exist, all corresponding release gates are BLOCKED.
