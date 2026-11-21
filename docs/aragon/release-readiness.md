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
| ARG-G01 Frontend | BLOCKED | GSSH/2 is approved and D3 defines `@input` `text:` as initial content, now emitted by Aragon with the 64 KiB text limit (`android-scene.md`). `compile_android_scene` consumes the proportional-length GSSH/3 profile and emits or refuses admitted constructs at their exact source ranges. `ARAGON-SOURCE-MAP/1` maps generated D1 action branches to their interaction declarations; individual statements in those branches do not yet have separate mappings. Still missing: adoption of GSSH/3-live `text-from` bindings (currently refused), statement-level D1 runtime origins, and the release audit. |
| ARG-G02 Semantics | BLOCKED | Phase 3 (17 Nov 2026): the scene compiler (`android-scene.md`) emits nested cells, absolute and proportional geometry, wrap, scroll, fill, buttons with D1, bound inputs and session state. Its pure-Kotlin layout spec, field conversion and the generated D1 runtime are compared on the JVM against `goya_lang::layout`, `Frame::scroll_limits`, `handoff::convert_field` and the canonical D1 executor (`scripts/check-android-scene-kotlin.sh`: 61 scenes × 7 viewport sizes, 331 conversion inputs, 40 random D1 programs × 12 dispatches). Three planted bugs were each caught. Still missing: pixel, touch-scroll and platform behaviour on Android (no emulator or device), display bindings, durable state and D2. |
| ARG-G03 Determinism | BLOCKED | Phase 2 (17 Nov 2026): the toolchain is locked (`android-toolchain-lock.md`: Temurin 21.0.13+9, Gradle 8.9 by SHA-256, AGP 8.7.3, Kotlin 2.0.21, platform 35 r2, build-tools 34.0.0, and SHA-256 dependency verification for all 246 build components). `scripts/check-android-apk-build.sh` builds the static-text, root-button and (since Phase 3) compiled-scene projects twice each, offline, with strict verification, from fresh copies at different paths; all six unsigned release APKs matched their twins and the recorded hashes (`cd3e6f15…f9b06b`, `47af8d6c…ab89e`, `c792e345…73979`). Still missing: a second host or OS, the release R1 comparison, and signed/bundle outputs. Dependency checksums were trusted on first use, not signature-verified. |
| ARG-G04 Android compile | BLOCKED | `aragon-android-project` emits the locked Gradle wrapper and verification metadata (`android-project.md`). The static-text, root-button and compiled-scene (`--scene`) qualification projects build with the Android SDK into unsigned release APKs with the locked toolchain (`scripts/check-android-apk-build.sh`). The scene fixture covers nested layout, wrap, scroll, two bound fields and a D1 action. Still missing: Android lint review and a signed release build. |
| ARG-G05 Emulator | BLOCKED | Phase 4 (18 Nov 2026): `scripts/check-android-device.py` installed the locked-toolchain release APKs (test-signed) on Android 15 (API 35) and Android 6.0 (API 23, the `minSdk`) emulators and passed 72 of 72 checks on each (`android-device-qualification.md`, logs in `evidence/`). Every element's bounds in the platform accessibility tree equal the Rust reference at four display configurations and both orientations, and at each scroll limit; field conversion and D1 dispatch behave as GSSH/2 requires, using overflow as the oracle. Still missing: a release-signed artifact, intermediate API levels and non-AOSP images, and pixel-level rendering. |
| ARG-G06 Physical device | BLOCKED | No physical device attached; the device script is ready to run on one (`android-device-qualification.md`). |
| ARG-G07 Lifecycle | BLOCKED | Phase 4: on both emulators, session state survives activity recreation (rotation) and resets to its initial value after process death; field text is restored in both cases. Still missing: durable state (still refused), crash ordering of durable writes, scroll offsets across recreation (reset to 0), and a physical device. |
| ARG-G08 Input/accessibility | BLOCKED | Phase 4: the soft keyboard opens on fields; buttons expose their labels; scroll viewports expose the `ScrollView` role and scroll actions (added in this phase). Gaps: fields have no accessible name (GSSH/2 has no input label; tied to decision D3); text is not font-scaled; TalkBack was not available on AOSP images; the keyboard's composition path was not exercised. |
| ARG-G09 Native | BLOCKED | Profile and package contents not established; absence of native libraries has not been verified. |
| ARG-G10 Packaging | BLOCKED | No release APK, bundle-derived APK set, signature or update test. |
| ARG-G11 Security | BLOCKED | Generated preview, scene and Nevada manifests disable app-data backup and cleartext traffic (`android-project.md`). Still missing: a reviewed application security policy, isolated worker analysis, and complete dependency provenance/signature evidence. |
| ARG-G12 Performance | BLOCKED | No reviewed budgets, benchmark plan or measurements. |
| ARG-G13 Documentation | BLOCKED | CLI, generated-project, scene-compiler and device-qualification usage, supported constructs and refusals are documented (`android-compiler.md`, `android-project.md`, `android-scene.md`, `android-device-qualification.md`, `android-toolchain-lock.md`); deployment documentation still requires later gates. |

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
