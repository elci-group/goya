# Goya–Aragon shared handoff gap ledger (H0)

Inspection revision: `7958bfaa8c1655cc232b5fd8f5434eb1131abf57` (12 Nov 2026).
This is source inspection only; no new tests were run and no GSSH contract is
implemented. No applicable `AGENTS.md` was found by searching the checkout's
parent tree. Workspace license is MIT OR Apache-2.0 (`LICENSE-MIT`,
`LICENSE-APACHE`).

## Representation decision

The canonical authoring authority is GY-LANG/1's parsed `Scene`, because it
retains the authored cell tree, absolute/proportional geometry, styles and
bounded interaction declarations. Do not use GY-IR/1 `Module` as the complete
handoff: its flow nodes cannot represent those properties. Do not use
`Lowered` as the complete handoff: it combines that limited module with
viewport-resolved geometry. Preserve canonical Scene semantics and augment
them through an explicitly versioned shared semantic contract. The current
`ParsedScene` is the strongest existing source-preserving input; evolve it via
the normal Goya contract process rather than adding an Aragon parser or a
parallel authoritative model.

`parse_with_source_map` was added after the older Aragon reconnaissance
revision. Its `SourceEntry` records declaration/property ranges as half-open
UTF-8 byte offsets and scene child-index paths. These paths are explicitly not
persistent IDs. The parser still discards some recognized syntax and does
not provide typed property identity/provenance for every semantic field.

## Six decision mapping

| Area | Existing representation | Exact gap / observed behavior | Handoff work |
| --- | --- | --- | --- |
| Adaptive layout | `SceneCell` has `position`, `extent`, proportional variants, fill/stroke, text size/anchor, wrap, scroll and child order. `layout` resolves a viewport to millipoints. | No anchors, margins/padding, min/max, intrinsic sizing, responsive rules, explicit insets, font scaling or overflow choices beyond scroll. Defaults/clamping live in the evaluator. `NodeRecord.explicit_rect_mp` stores only a resolved rectangle when both authored position and extent exist. | Define a versioned admitted adaptive constraint subset and reference evaluator; preserve authored rules and reject unimplemented modes. |
| Actions/functions | `InteractionDecl` has D1/D2 class, button-name trigger, typed read/write/input keys, straight-line assignments; D2 requests are static journal entries. Compiler validates and wraps bytecode execution. | No visual action graph, custom Goya function body/reference, named handler inputs/outputs, explicit error/cancellation continuations, invocation policy or arbitrary typed outputs. D1/D2 are bounded assignment models only. | Version a typed ordered action representation and reference evaluator; retain current interaction behavior as a legacy subset conversion only where exact. |
| Field bindings | Input cell carries `text` as placeholder. Interaction separately declares named typed inputs. | No link from input element/value to an interaction input; placeholder is not value. No conversion, normalization, validation, coherent event snapshot, composition policy or binding origin. | Add explicit event/source/property/destination/type/conversion/validation bindings and validate exactly-once input coverage. |
| Persistence | `goya_state::Store` supports in-memory revisioned transactions. Interaction schemas declare typed keys. | No per-value lifetime, initial value, durable key, codec, storage class, sensitivity, backup, commit acknowledgement or migrations. Store uses in-memory `HashMap`; persistence is not provided by this interface. | Add per-value declarations and adapter contract; secure storage must be required for protected values, with no plaintext downgrade. |
| Provenance | Lexer tokens include UTF-8 byte starts/ends; `ParsedScene.entries` records declaration/property ranges and paths. | Property entry range is currently token-to-value range but lacks source ID/digest, stable authored IDs, Quinta IDs/property paths, semantic-node mapping, transformation ancestry and generated-source map. Child paths shift after edits. Parse errors report line only. | Extend source records with immutable source identity/digest, semantic property IDs and editor origins; compose origin sets through lowering and Kotlin emission. |
| Strict rejection | Unknown declaration words and malformed known values fail. Unsupported syntactic forms generally fail. | In `parse_property`, unknown property names call `skip_value()` and are accepted. `@template` is recognized and skipped. Thus typo/unsupported behavior can disappear before target admission. | Add a closed strict profile/version that rejects unknown properties and unsupported recognized features at exact spans; preserve old compatibility parsing separately. |

## Source map and loss points

| Path | Relevant API / observed limitation |
| --- | --- |
| `crates/goya-lang/src/lexer.rs` | `Token` carries kind, text, line and UTF-8 `byte_start`/`byte_end`. No source digest or source ID. |
| `crates/goya-lang/src/parser.rs` | `parse_with_source_map`, `parse_strict`, `ParsedScene`, `SourceEntry`, `SourceRange`; entries sorted by source offsets. Parsed document and entries carry SHA-256 of exact UTF-8 input. Property ranges and owning authored IDs are retained; `scene_path` is only a structural lookup aid. Strict mode rejects unknown properties, skipped templates, duplicate properties, invalid D1/D2 declarations and missing/duplicate element IDs. Syntax errors still map only to the parser's reported line range. `parse` returns only `Scene`, discarding entries. |
| `crates/goya-lang/src/scene.rs` | `SceneCell.authored_id` preserves explicit `id: "..."`; strict admission requires unique non-empty IDs for authored elements. Interaction identities and Quinta IDs/property paths are not defined. |
| `crates/goya-lang/src/lowering.rs` | `lower` resolves geometry for supplied width/height; `NodeTable` maps deduplicated names to IR IDs and optional resolved rectangles. Text becomes synthetic IR nodes. Button traversal stops for actions; input children are handled by synthetic columns. No origins travel with nodes. |
| `crates/goya-ir/src/lib.rs` | GY-IR/1 `Node` has Column/Row/Text/Button/Spacer/Input. No authored geometry, style, action body, field binding or source origin. Also exposes an earlier line-oriented parser. |
| `crates/goya-lang/src/transition.rs` | `compile_d1_interactions` / `compile_d2_interactions` return typed execution wrappers; D1 execution checks runtime schema and output types. D2 uses caller-supplied journal and does not perform host effects. Raw bytecode is private. |
| `crates/goya-sdk/src/lib.rs` | `SceneProject` retains source/Scene/lowered representation; `frame()` uses scene layout, `runtime()` consumes lowered flow IR, and `compile_interactions()` compiles D1/D2. `InputNodeInfo` reports placeholder and geometry only. |
| `crates/goya-state/src/lib.rs` | GY-STATE/1 is an in-memory transactional store with revision counter; no persistence codecs or platform adapter contract. |

Existing source tests cover UTF-8 ranges and duplicate-name distinction in
`crates/goya-lang/src/lib.rs`; they do not establish stable IDs or complete
provenance. Existing lowering and interaction tests establish only their
documented subsets, not lossless GSSH serialization.

## Proposed H0 choices requiring ADR/versioning

1. Name and version the shared handoff through the existing Goya contract
   process. Keep `Scene` authoritative at authoring time; handoff is a closed,
   typed, validated representation plus provenance, not a second language.
2. Define stable element/property IDs and source/editor origin references;
   child paths and content hashes are lookup/revision aids, never identity.
3. Define strict parse/admission behavior while preserving a clearly named
   legacy compatibility mode. Unknown source must never reach Aragon as valid.
4. Specify logical units, adaptive layout subset, action protocol, binding
   conversions, persistence guarantees and version compatibility before
   claiming the handoff is target-ready.
5. Treat serialization, if selected, as exhaustive closed decoding with
   bounded resource use and deterministic canonical order. This inspection
   does not select JSON or any file extension.

No Quinta, Neptune, PAMPER or other orchestration API was established by this
inspection. Inspect actual interfaces before integrating them.
