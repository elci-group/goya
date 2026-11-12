# Aragon reconnaissance (A0, preliminary)

Inspected checkout: `eed4ede9ec853b1108c017a09f6c7cfd132aac1b`,
12 November 2026. This record is a scoped source inspection, not a test audit
or an approval of a toolchain. Refresh it if the implementation base changes.
No applicable AGENTS.md was found in the checkout or its parent.

## Existing interfaces

| Source | Observed contract and consequence |
| --- | --- |
| `Cargo.toml`, `rust-toolchain.toml` | Workspace Rust 1.95.0; workspace license MIT OR Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`. New dependencies need their own license review. |
| `crates/goya-lang/src/{lexer,parser,scene}.rs` | GY-LANG/1 parses `.gscene` to `Scene`, including authored geometry and bounded interactions. Tokens have lines only, and the scene AST has no source spans. Unknown visual properties and templates are intentionally skipped; a target cannot detect all omissions from the AST. |
| `crates/goya-lang/src/{lowering,transition}.rs` | `lower` produces a GY-IR/1 module plus a viewport-resolved node table; D1/D2 interaction compilation is separate. Its documentation calls the combined table GY-IR/2, but it does not change `goya_ir::IR_CONTRACT`. It is not a lossless authored-scene interchange. |
| `crates/goya-ir/src/lib.rs` | GY-IR/1 is a flow-oriented node model and has its own earlier line-oriented parser. It cannot encode the full scene geometry, visual attributes or authored transitions. |
| `crates/goya-sdk/src/lib.rs` | `SceneProject` retains source, scene and lowered projection; `frame()` uses scene layout, while `runtime()` uses flow layout. `compile_interactions()` validates D1 and D2. |
| `crates/goya-state/src/lib.rs`, `docs/contracts.md` | GY-STATE/1 offers revisioned transactions; GY-LANG/1 describes bounded checked arithmetic and D1/D2. A host supplies initial state and inputs. Durable state declaration and text-field-to-input mapping are not established by these interfaces. |
| `crates/goya-tov/src/lib.rs` | GY-TOV/1 wraps external `tov check` and `tov build` processes. It does not establish the proposed Android configuration schema or a general `.gscene` import resolver. Inspect the actual configuration contract before designing an adapter. |
| `platform/android/gradle/` | Existing IME library, not an application compiler. Wrapper is Gradle 8.9 with a distribution checksum; AGP 8.7.3, Kotlin 2.0.21, compile SDK 35 and min SDK 23 are library pins, not an application release lock. |

README describes Linux/Wayland as the initial Goya target; this inspection
does not qualify macOS or Windows. Android source emission and build-host
support remain unverified. No Quinta or Neptune integration API was established
by this inspection.

## Local infrastructure observation

On this Linux x86-64 host Java reports OpenJDK 21.0.11. The system Gradle is
4.4.1; the existing Android wrapper declares 8.9. An Android SDK directory is
present at `/usr/lib/android-sdk`, but no `platforms` directory was found
there; no emulator or bundletool command was found on PATH. These observations
do not rule out other configured SDK locations. `adb devices -l`, run with
access to the host ADB service, reported no connected devices. The sandboxed
attempt could not bind the ADB listener. No Android build or device test has
been performed. A compatible, checksum-locked Compose/AGP/JDK/SDK stack and
offline dependency closure remain to be selected and verified as a unit.
