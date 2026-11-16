# Initial web admission matrix

Disposition is for production web builds under the Wave 0 policy, not a claim that parsing or implementation exists. Every reachable construct not listed is REJECTED. No fallback is available.

| Feature | Disposition | Evidence needed before admission |
|---|---|---|
| Strictly parsed scene, IDs and hierarchy | REJECTED | Complete field coverage and origin handling; verified Frampton browser mapping. Virtual ontology root follows canonical ID rules. |
| Static text, colours, typography and geometry | REJECTED | Faithful browser mapping, unit rules, escaping, geometry and visual tests. |
| Button and input | REJECTED | Native or equivalent accessible semantics, keyboard/focus/composition tests and verified event mapping. |
| Authored D1 interaction and button binding | REJECTED | Canonical action identity, exact typed-state differential and rollback evidence in browser. |
| Initial state, input binding and visible state binding | REJECTED | Approved shared source semantics or explicitly approved caller contract; preserve single-authority requirement. |
| D2, effects, network, storage, persistence, navigation, workers and dynamic imports | REJECTED | Explicit capability, lifecycle and security contract and qualified implementation for each feature. |
| Unknown syntax/property, duplicate property, invalid interactions, unrepresented templates | REJECTED | Canonical strict error and source origin; no permissive parsing. |
| Unknown target/profile/contract version or unavailable Frampton | REJECTED | Explicit actionable diagnostic; no emitted application. |

Candidate static and interactive features are sequencing priorities, not supported features. No element is silently omitted, approximated or mapped to an unapproved fallback.
