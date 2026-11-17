# Aragon v0.0.1 readiness

Status: **BLOCKED**. The canonical Goya frontend now has strict admission and
source-origin records, and Neptune has a verified source map for its narrow
Swift text preview. Goya's SDK also emits experimental Kotlin previews for a
single static text cell and a root button, plus a standalone scalar D1 runtime
and deterministic starter project. These are implementation capabilities,
not Aragon release evidence. Phase 2 locked one Android toolchain and builds
both generated projects into reproducible unsigned release APKs on this host
(`android-toolchain-lock.md`); no APK has been installed or run, and no AAB,
signing, benchmark or device qualification is asserted. The opt-in
Kotlin/JVM D1 conformance runner is `scripts/check-android-d1-kotlin.sh`; it
does not qualify Android platform behavior.

## Phase 0 cleanup (17 November 2026)

Scaffolding committed in `99a2872` was removed because it was not evidence for
any gate: the `emit_android_root_input_preview`, `emit_android_scrollable_preview`
and `emit_android_d1_runtime_with_form` emitters (the first line-scanned source
instead of using `parse_strict`, the second ignored its source, and the third
emitted Kotlin referencing undeclared members), `runtime/form_engine.rs`, the
never-populated GY-LANG "Phase 2" input/accessibility types and `SceneCell`
fields, `tests/integration/` and `crates/goya-sdk/tests/android-differential/`
(neither was compiled), and their string-only tests. Uncommitted edits to the
removed files are kept in the `git stash` entry named "aragon phase 0". Root-level
`IMPLEMENTATION_*` and `PHASE*` reports are marked superseded.

With a local JDK and Kotlin compiler, the strict checks found and fixed two
defects: generated D1 dispatch for an input-free action emitted an untyped
`setOf()` (now `setOf<String>()`, likewise the snapshot `mapOf`), and the
`Button` test stub was final. Both of these now pass on this host:

```sh
bash scripts/check-android-d1-kotlin.sh
bash scripts/check-android-project-kotlin.sh --require-kotlin
```

They ran with Temurin JDK 21.0.13+9 and kotlinc-jvm 2.0.21, installed under
`~/.local/opt` from archives verified against their published SHA-256 digests
(`d7f5d5cf…66d1c` and `0352c0a4…bcf2a`) and selected with `JAVA_HOME`/`PATH`
for the command. This is a Kotlin/JVM result against Android API stubs, not a
locked Android toolchain or SDK build.

Resolved in Phase 1 (`docs/gssh/decisions.md`, D6): `placeholder:` and
`max-length:` are compatibility-parser only and rejected by strict admission;
the `focus`/`blur`/`select`/`submit` gesture keys are removed. The unused
`KotlinMapping` in `crates/neptune/src/source_map.rs` remains and is not the
Android source map.

| Gate | Status | Evidence or missing work |
| --- | --- | --- |
| ARG-G01 Frontend | BLOCKED | Phase 1 (17 Nov 2026) implemented the shared handoff as Candidate contract `GSSH/2` (`docs/gssh/GSSH-2.md`, decisions in `docs/gssh/decisions.md`): `handoff::admit_v2` adds declared state with initial values and lifetimes, input-to-`@input` bindings with mandatory coverage and the reference conversion `convert_field`, and identity-keyed origins for elements, properties (`element#property`), interactions, state keys and bindings. All parse and strict diagnostics now carry exact UTF-8 byte ranges. `crates/goya-lang/tests/strict_corpus.rs` holds the negative corpus (28 fixtures with exact expected spans) and token/byte mutation sweeps under both profiles. Still missing for this gate: architecture approval of GSSH/2; a decision on `@input` `text:` (placeholder versus initial content); a Kotlin emitter and Android source map consuming GSSH/2 (Phase 3); and a release audit. |
| ARG-G02 Semantics | BLOCKED | `goya_sdk::emit_android_static_text_preview` emits Kotlin for one strictly admitted static text cell. `emit_android_root_button_preview` emits a one-button preview only after canonical GY-IR lowering and D1 compilation confirm the authored callback ID and trigger ActionId match; it rejects actions with inputs. `emit_android_d1_runtime` emits a standalone scalar D1 runtime with atomic state transitions; the strict Kotlin/JVM qualification runner covers a bounded conformance harness. These source/runtime checks do not establish Android platform equivalence. There is no Android differential/mutation corpus. See `android-root-button.md` and `android-d1-runtime.md`. |
| ARG-G03 Determinism | BLOCKED | Phase 2 (17 Nov 2026): the toolchain is locked (`android-toolchain-lock.md`: Temurin 21.0.13+9, Gradle 8.9 by SHA-256, AGP 8.7.3, Kotlin 2.0.21, platform 35 r2, build-tools 34.0.0, and SHA-256 dependency verification for all 246 build components). `scripts/check-android-apk-build.sh` builds the static-text and root-button projects twice each, offline, with strict verification, from fresh copies at different paths; all four unsigned release APKs matched their twins and the recorded hashes (`cd3e6f15…f9b06b`, `47af8d6c…ab89e`). Still missing: a second host or OS, the release R1 comparison, and signed/bundle outputs. Dependency checksums were trusted on first use, not signature-verified. |
| ARG-G04 Android compile | BLOCKED | `aragon-android-project` emits the locked Gradle wrapper and verification metadata (`android-project.md`), and both generated qualification projects build with the Android SDK into unsigned release APKs with the locked toolchain (`scripts/check-android-apk-build.sh`, 17 Nov 2026). This covers only the two narrow emitted subsets (one static text cell; one root button with D1). Still missing: the wider language surface (Phase 3), lint and a signed release build. |
| ARG-G05 Emulator | BLOCKED | No emulator execution or recorded artifact. |
| ARG-G06 Physical device | BLOCKED | No device attached at inspection; no execution. |
| ARG-G07 Lifecycle | BLOCKED | No recreation, process-death or migration tests. |
| ARG-G08 Input/accessibility | BLOCKED | No Android IME or TalkBack evidence. |
| ARG-G09 Native | BLOCKED | Profile and package contents not established; absence of native libraries has not been verified. |
| ARG-G10 Packaging | BLOCKED | No release APK, bundle-derived APK set, signature or update test. |
| ARG-G11 Security | BLOCKED | No application manifest, isolated worker or dependency evidence. |
| ARG-G12 Performance | BLOCKED | No reviewed budgets, benchmark plan or measurements. |
| ARG-G13 Documentation | BLOCKED | CLI and generated-project usage and supported-feature limits are documented (`android-compiler.md`, `android-project.md`); deployment and qualification documentation still require later gates. |

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
