# MAJA-ADR-002: Browser-native execution

Accepted by user directive MAJA-ARCH-002, 2026-11-15. This supersedes ADR-001's requirement that browser builds pass through Frampton. Maja's standalone target generates browser-native assets; Frampton is optional for explicitly remote operations and is not part of local execution. ADR-001's observations about the inspected Frampton implementation remain valid.

Goya's canonical strict frontend and retained source handoff remain the semantic authority. Until shared IR preserves all semantics, Maja retains that handoff and admits only explicitly validated subsets; it must not infer omitted initial values, view bindings, effects or authority. Unknown and unsupported features fail with diagnostics. Local state, browser events and effects must not be silently redirected to a server. Remote authority must be independently enforced by its backend.

The initial standalone profile `maja.web.local.v1` is an [experimental, limited profile](standalone-profile.md). Its exact admitted features and tests are documented as they are implemented. It is not a production qualification. An application that uses only its admitted local features must require no application-logic server; offline startup and service workers are separate capabilities and are not implied.
