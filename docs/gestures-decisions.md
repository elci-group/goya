# Gestures G0 decision register

Status: **proposed, not ratified**. These decisions do not admit a schema,
target, control or release. See [grounding](gestures-grounding.md). G0 requires
named owners and ratification before these decisions can be accepted.

| Decision | Alternatives and proposed direction | Rationale, fixtures and compatibility |
| --- | --- | --- |
| Authoring and handoff | Extend `goya-lang::Scene` and a versioned GSSH contract; alternatives are a target dialect or legacy `goya-ir::Module`. Exact handoff location remains open. | Scene preserves authored structure; flow IR loses it. F13/F15 must compare source and Quinta and preserve origins. Legacy parsing remains unchanged. |
| Syntax | Add a closed opt-in declaration to existing `.gscene` grammar; exact tokens remain open. Alternative: a separate manifest. | One frontend avoids competing interpretations. F04/F11/F15 cover invalid properties. Old sources retain their prior profile. |
| Defaults | Adopt the directive's `gestures.text/1` table, versioned; alternative: blanket deny. | Matches the product baseline while making migration opt-in. F04/F11 cover target and read-only/protected states. Default changes need a new profile or retained old profile. |
| Scope policy | Separate ordered defaults from non-widenable hard masks; element `inherit` resolves to the nearest default. Alternative: one overloaded deny. | Distinguishes overridable defaults from hard denial. F11 tests nesting, shadowing and monotonicity, including related origins. |
| Unicode | Propose scalar offsets for shared selections, half-open UTF-8 bytes for source origins, checked UTF-16 conversion for adapters. Pin a grapheme algorithm and version before admission. | Avoid conflating source bytes and native indices. F08 covers emoji, combining marks, flags and RTL; byte-indexed legacy runtime needs an explicit bridge. |
| Editor owner | One owner per editor and lifecycle epoch; native controls may own the immediate buffer behind a checked commit boundary. Alternative: duplicate authoritative buffers. | Avoid stale selection and conflicting history. F02/F05/F06/F09/F14 cover validation, races and destruction. GSSH binding semantics remain prerequisite. |
| Windows toolkit | **Open**: inspect Sylyca and native runners; WinUI is a candidate. | F01/F03/F10 require a real native runner; no Windows profile is admitted. |
| Platform matrices | **Open**: pin OS, SDK, toolkit, compiler, device, IME and assistive-tool endpoints after worker inspection. | Minimum/newest supported versions and actual devices are evidence, not inferred from source generation. |
| Limits and performance | Start with the directive's proposed 1 MiB text limits, 100 history entries, 4 MiB retained history and 10,000 policy steps; do not admit until measured and budget semantics are defined. | F07/F09/F16 require bounded allocation, eviction, evaluator budget and locked corpus; larger profiles need separate admission. |
| Custom rendering | Exclude custom-rendered editors from the first native-control profile; qualify separately when a real input/selection/accessibility bridge exists. | Existing runtime/Android IME bridging does not prove native route enforcement. F03/F08/F10 still apply. Existing behaviour is not silently changed. |
| Deployment | Keep old profiles and pin schema, adapter and capability locks; withdraw a broken admission rather than silently changing permissions. | F13/F14/F16 bind source, artifacts and environment; migration reports changed defaults and handler conflicts. |
| Quinta ownership | **Open**: inspect the authoring interface and assign an owner; use the same frontend and validators as CLI. | F13 demands canonical parity and revision-aware diagnostics. No inspector is claimed. |

These proposals do not resolve the shared binding contract, native route
enforcement, named owners, toolchain locks or device evidence. Unresolved
choices remain BLOCKED.
