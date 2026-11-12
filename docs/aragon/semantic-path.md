# Proposed shared semantic path (A0; not an approved contract)

The source of meaning for `.gscene` is the canonical GY-LANG parser and its
documented semantics. `SceneProject` demonstrates retaining both the parsed
scene and the existing IR projection. Aragon must not reconstruct the source
scene from `goya_ir::Module` or the viewport-specific `NodeTable`: those do
not retain enough information. This is a proposed direction, not a newly
versioned shared contract or proof of a lossless implementation.

Before emission, introduce a reviewed, versioned shared representation or
side tables retaining the complete parsed scene, interactions, structural
identity and byte-accurate source ranges. Preserve relative and absolute
geometry rather than resolving it once against a fixed viewport. Retain
declared ordering, styles, clipping, wrap, input meaning and effects. Define
identity for duplicate names independently of name lookup. Link each
interaction's trigger to the canonical compiler's action identity; account
for its button traversal behavior. Do not claim that retaining a `Scene`
alone is sufficient: skipped syntax and missing spans are already lost.

Extend the canonical frontend through normal shared contract versioning so
that strict consumers receive structured diagnostics for skipped templates,
unknown properties and unsupported syntax, with source ranges. Preserve
existing compatibility behavior where required by the existing contract.
Reject source or intermediate contract versions that are not understood.
Bound source size, nesting and diagnostic volume before accepting untrusted
inputs; validate asset and path boundaries separately.

## Feature mapping and unresolved decisions

| Feature | Existing source meaning | Proposed Android decision / acceptance |
| --- | --- | --- |
| Cells, hierarchy and geometry | Scene preserves absolute and proportional coordinates; layout produces millipoints with parent constraints. | Map one point to one dp only after a reviewed rounding/clipping policy. Differential-test changing parent sizes and overflow against `goya_lang::layout`. |
| Text and wrap | Optional text, size, anchor and grapheme-aware column wrapping. | Specify font scaling and exact wrap policy; compare semantic output and bounded visual results, not pixel identity. |
| Button and D1 | Declared activation has a bounded typed transition; addition is checked i64. | A single Kotlin authority executes atomically, with checked addition and observable error parity against the existing D1 execution wrapper. No action may be omitted. |
| Input | Scene input has a placeholder; D1 inputs are separately typed host values. | Establish and version an explicit field-to-input binding and conversion/error contract. Preserve IME composition, selection and Unicode behavior. |
| State and persistence | State keys and types occur in interaction declarations; host supplies initial snapshots. | Define initial values, durable declarations, restoration and migration in a shared configuration contract before generating them. Do not invent defaults. |
| D2 and services | Explicit effect requests and journal replay. | Reject until capability and lifecycle adapters are admitted; never erase an effect. |
| Scrolling | Scrollable node clips and its descendants may overflow. | Match reference geometry and scroll bounds; test touch, keyboard and accessibility separately. |

Generated Compose must be a pure projection of a single revisioned state
owner; dispatch transitions and persistence outside composition. Define
event ordering, reentrancy, cancellation and failure results before claiming
equivalence. State changes and durable writes need a documented crash and
recovery policy: a UI revision and its durable record are not automatically
one transaction.

## Required next work

1. Agree the shared versioned source/semantic contract and strict parse path;
   implement source-origin retention and fixtures without breaking GY-LANG/1.
2. Specify host-supplied initialization, input bindings and declared durable
   state in Goya's actual configuration mechanism, then validate a single
   sample end to end against the independent D1 and layout reference paths.
3. Build a typed Kotlin emitter with deterministic escaping and source maps;
   reject all unsupported constructs. Add negative and mutation fixtures.
4. Select and lock the complete compatible Android toolchain, run offline
   compilation, then execute the exact recorded APK on emulator and device.
   Release packaging, accessibility and lifecycle gates remain separate.
