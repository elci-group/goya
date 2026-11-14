# Aragon v0.0.1 readiness

Status: **BLOCKED**. The canonical Goya frontend now has strict admission and
source-origin records, and Neptune has a verified source map for its narrow
Swift text preview. Goya's SDK also emits an experimental Kotlin `TextView`
preview for a single, unstyled static text cell, rejecting features outside
that subset. These are implementation capabilities, not Aragon release
evidence. No Android build, APK, AAB, benchmark, device qualification or
complete Android toolchain lock is asserted.

| Gate | Status | Evidence or missing work |
| --- | --- | --- |
| ARG-G01 Frontend | BLOCKED | `goya_lang::parse_strict` admits only represented semantics: it rejects unknown properties, templates, duplicate canonical properties, missing/duplicate authored IDs, and invalid interactions with structured codes and UTF-8 byte ranges (`crates/goya-lang/src/parser.rs`). `parse_with_source_map` returns sorted declaration/property entries with exact-source SHA-256, byte ranges, within-revision scene paths, and authored semantic IDs where available. This is not yet an Aragon frontend contract or a Kotlin emitter/map. Parse failures from the compatibility parser still carry a line only; `parse_strict` converts these to the corresponding whole-line byte range. Property entries have no independent authored property ID, and scene paths can shift across edits. Negative coverage exists as unit tests in `crates/goya-lang/src/lib.rs`, but no Aragon differential/mutation corpus or release audit is recorded. |
| ARG-G02 Semantics | BLOCKED | `goya_sdk::emit_android_static_text_preview` emits Kotlin for one strictly admitted static text cell, caps its authored UTF-8 text at 16 KiB, and rejects unsupported scene features. Unit coverage checks the byte boundary and source mapping. Its output has not established layout or text equivalence; there is no Android differential/mutation corpus. |
| ARG-G03 Determinism | BLOCKED | No generated outputs or R1 comparison. |
| ARG-G04 Android compile | BLOCKED | No generated app or verified pinned Android build environment. |
| ARG-G05 Emulator | BLOCKED | No emulator execution or recorded artifact. |
| ARG-G06 Physical device | BLOCKED | No device attached at inspection; no execution. |
| ARG-G07 Lifecycle | BLOCKED | No recreation, process-death or migration tests. |
| ARG-G08 Input/accessibility | BLOCKED | No Android IME or TalkBack evidence. |
| ARG-G09 Native | BLOCKED | Profile and package contents not established; absence of native libraries has not been verified. |
| ARG-G10 Packaging | BLOCKED | No release APK, bundle-derived APK set, signature or update test. |
| ARG-G11 Security | BLOCKED | No application manifest, isolated worker or dependency evidence. |
| ARG-G12 Performance | BLOCKED | No reviewed budgets, benchmark plan or measurements. |
| ARG-G13 Documentation | BLOCKED | Usage, deployment and supported-feature documentation require implementation and qualification. |

The initial inspection and outstanding semantic decisions are recorded in
`reconnaissance.md` and `semantic-path.md`. A missing evaluator or unavailable
device must remain BLOCKED; it is not a passing or inapplicable result.

## Source evidence and boundaries

`crates/goya-lang/src/parser.rs` implements the strict admission checks and
source-entry metadata described above. The parser retains UTF-8 byte spans for
recognized declarations and properties, but does not attach source origins to
the lowered IR/node table. The existing map implementation is in
`crates/neptune/src/source_map.rs`: it accepts schema version 2 maps for
generated Swift, validates artifact/source hashes and UTF-8 boundaries, and
reports matching authored IDs. `crates/neptune/src/lib.rs` only generates this
map for its one-cell static text preview. That preview explicitly does not
admit an iOS build or verify Swift syntax. Neither implementation provides the
typed Kotlin emission, Android source map, broader source coverage, or
qualification required by ARG-G01 and later gates. The SDK's static `TextView`
preview can also return a source digest, authored ID, and UTF-8 byte ranges
linking its generated Kotlin literal to the source text property. This is a
single-property in-process map, not a general Android source-map format. No
Android compile or execution evidence is recorded.

`scripts/check-android-preview-kotlin.sh` generates a preview and compiles it
offline with cached Kotlin 2.0.21 and minimal Android API stubs. This checks
Kotlin syntax and types against those stubs, not an Android SDK build or
platform behavior; ARG-G04 remains blocked.
