# Neptune N0 repository reconnaissance

**Status:** observed repository facts and proposed admission boundary, recorded 12 November 2026. This is not an implementation or benchmark-completion claim.

## Checkout and applicable instructions

- Goya checkout: `/home/sal/goya`
- Inspected commit: `65520f25b3493af7b683be4d618eb8a69af5279a`
- Working tree was clean at inspection start.
- No `AGENTS.md` is present in the checkout. (The applicable local `bank` skill was followed to create this report.)
- Workspace: root `Cargo.toml`, Rust 2021, workspace package version `0.1.8`.
- Host inspection did not find `xcodebuild` or `swift`; Apple compilation, simulator execution, signing, and physical-device validation cannot be performed in this environment.

## Canonical frontend, IR, and integration paths

| Concern | Observed implementation | Contract and implications |
|---|---|---|
| `.gscene` lexer | `crates/goya-lang/src/lexer.rs` | `Token` has kind, text, and line only. No byte/character source range; quoted strings are not escape-decoded. |
| `.gscene` parser and AST | `crates/goya-lang/src/parser.rs`, `crates/goya-lang/src/scene.rs` | `goya_lang::parse` owns the `@ontology`, `@cell`, `@button`, and `@input` grammar. Parse errors carry a line only. AST has geometry, text/style, nested cells, and authored D1/D2 interaction declarations. |
| Goya language facade | `crates/goya-lang/src/lib.rs` | Publishes parser, layout, lowering, and interaction compilation. Contract `GY-LANG/1`. |
| Shared typed IR | `crates/goya-ir/src/lib.rs` | `goya-ir` is the sole shared IR crate in this workspace. It defines `Module`, `Node`, `NodeId`, `ActionId`, `Capability`, and a separate legacy line-oriented parser under contract `GY-IR/1`. Nodes have no source spans, stable semantic IDs, rich types, or state ownership metadata. |
| Scene-to-IR lowering | `crates/goya-lang/src/lowering.rs` | Lowers the canonical parsed scene into `goya_ir::Module` plus a deterministic `NodeTable`; it assigns sequential IDs and derives a fixed-viewport layout. Code comments call this GY-IR/2, but `docs/contracts.md` explicitly says this does not amend the GY-IR/1 identifier. This is an extension/path, not a second IR authority. |
| Project facade | `crates/goya-sdk/src/lib.rs` | `SceneProject` parses, lowers and exposes scene/layout and compiled interactions. It is the smallest existing API boundary for a target backend, but currently exposes neither source maps nor a canonical manifest/capability validation service. |
| State/runtime | `crates/goya-state`, `crates/goya-runtime`, `crates/goya-executable`, and `crates/goya-lang/src/transition.rs` | Existing state stores and bounded compiled D1/D2 transition machinery exist. This is not evidence of arbitrary typed local state or Swift state restoration semantics. D2 effect requests require an explicit journal/host path. |
| TOV adapter | `crates/goya-tov/src/lib.rs` | `GY-TOV/1` is an argument-safe process adapter to an external `tov` CLI. It is not a Goya project configuration/resolution model and should not be mistaken for a canonical Neptune manifest. |
| Existing authoring/preview | `crates/goya-studio/src/main.rs`, `crates/goya-sdk/src/lib.rs` | `goya-studio` supports local inspect/check/edit/HTML preview/serve. It has no iOS target or shared compiler-service protocol. |
| Existing tests | `crates/goya-lang/src/*` unit tests; `crates/goya-sdk/tests/contract_conformance.rs`; crate-local tests | Parser, lowering, interaction, SDK, and preview contracts are exercised. No iOS compiler, Swift snapshots, Apple validation harness, Mac worker adapter, or physical iPhone evidence was found. |

The workspace has no `goya-native` or `goya-lang` duplicate implementation beyond the paths above. Keep `goya-lang` as the `.gscene` frontend and `goya-ir` as the shared typed node IR. Neptune should consume these APIs and add a target-only lowering IR; it must not duplicate parsing or create a second source language.

## Existing scene subset and proposed first safe admission

### Observed source constructs

The parser recognizes ontology/cell/button/input blocks; absolute and proportional geometry; fill/stroke/text styling; text anchoring and wrapping; scrolling; and authored D1/D2 interactions. The interaction AST supports typed `Int`, `Text`, and `Bool` keys, literals, state/input/effect reads, checked integer addition, and straight-line assignments. `goya-lang::lower` maps visual nodes to columns, text, buttons, inputs, and spacers and assigns button actions deterministically in traversal order.

### Admission boundary

No complete feature in the requested v0.0.1 app should be called iOS-admitted yet: the existing source has no declared navigation model, native list semantics, persistence adapter contract, accessibility-name property, or lifecycle restoration contract. The required two-screen app cannot currently be expressed as a validated, typed source fixture with all required semantics. Do not infer these semantics from names, geometry, or host behavior.

The first compiler experiment can safely admit only static `@cell` composition, text, and explicitly labeled buttons/inputs once source mapping and accessibility contracts are added. D1 state transitions are a candidate for the next bounded step because they are already compiled by Goya, but persistence and navigation remain unsupported until their semantics are specified. Inputs currently have placeholder text and host-side focus/content state; that is not a complete shared state binding. Fixed-point layout output must not be treated as responsive native layout.

Every construct outside the explicit admission table must fail closed with a source-located diagnostic. In particular, D2 effects, scrolling, arbitrary custom rendering, raw GY-IR modules, and unsupported source properties must not silently disappear in successful builds.

## Compatible interfaces and missing components

### Reusable now

- `goya_lang::parse`, `Scene`, and `goya_lang::lower` for canonical syntax and shared IR lowering.
- `goya_lang::compile_d1_interactions` for the existing bounded interaction subset, subject to a mapping spec and differential oracle.
- `goya_sdk::SceneProject` as a stable-ish application facade for source loading and interaction discovery.
- `goya_executable` D1/D2 execution contracts for bounded semantics where explicitly admitted.
- Existing determinism helpers and test conventions, after their contracts are inspected when implementation begins.

### Required before release implementation

- Source byte ranges on lexer tokens and parser/AST nodes, plus stable semantic identities independent of traversal accidents and machine paths.
- A versioned Goya shared IR/source-map contract. Resolve the GY-IR/1 versus GY-IR/2 documentation distinction before claiming one versioned cross-backend IR; retain `goya-ir` authority.
- A typed iOS target IR and capability registry with exhaustive lowering checks.
- A documented source subset that expresses the mandatory screens, list, navigation, typed state, persistence, accessibility labels, light/dark behavior, and restoration.
- Mapping specifications and a reference executor/oracle for admitted behavior.
- Typed Swift syntax emission or an equivalent structured emitter, stable formatting, deterministic resource hashing, and D1 fresh-directory/order-independence checks.
- Manifest/config ownership decision: no canonical TOV config resolver was found; do not assume TOV is the Goya project manifest.
- Mac worker protocol, toolchain lock, isolated build jobs, Apple compile/simulator validation, authorized development signing, and physical iPhone fixture/evidence.
- Build reports, capability inventory, source maps, privacy/distribution input schemas, SBOM, and release gate evaluator integrations.

## Competing implementations and decisions

1. `goya-ir` has a legacy textual parser and typed `Module`; `goya-lang` has the actual `.gscene` parser and deterministic lowering into the same `Module` type. For Neptune, the `.gscene` frontend is authoritative for `.gscene` input; the legacy parser must not be used as a parallel frontend.
2. GY-IR/1 is the declared contract identifier in `goya-ir`, while the scene lowering comments/docs refer to a GY-IR/2 table extension. Keep using the existing Rust types, but version and document the actual consumed data before a public compiler protocol depends on it.
3. Existing layout resolves a viewport to millipoints; SwiftUI needs adaptive layout semantics. A future target pass must preserve source relationships/constraints or explicitly reject geometry-dependent constructs; it cannot claim general responsive behavior from resolved coordinates.
4. Existing button actions are sequential `ActionId`s and scene node keys are deterministic names with suffixes for duplicates. Neptune provenance IDs should derive from canonical source identity and declared names, and must account for duplicate names explicitly.

## Planned versus observed

Observed: the Rust frontend, shared node IR, scene lowering, D1/D2 compiler, state/runtime crates, SDK facade, studio preview, and local tests listed above exist at the inspected commit.

Planned only: Neptune, native SwiftUI/UIKit emission, iOS target validation, C ABI runtime slices, Apple worker builds/signing, Quinta integration, iPhone execution, and all release/benchmark evidence described in the directive. None is asserted complete by this report.
