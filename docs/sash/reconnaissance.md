# Sash reconnaissance

Directive: SASH-ENT-001. Date: 14 November 2026. Status: inspection plus one static-window link. This file does not admit a Windows release.

## Repository

| Fact | Observation |
| --- | --- |
| Root | `/home/sal/goya` |
| Remote | `https://github.com/elci-group/goya.git` |
| Workspace crate version | `0.1.8` in the workspace `Cargo.toml` |
| Toolchain file | Rust `1.95.0` |
| `AGENTS.md` | None in the tree |
| Quinta | No Quinta crate, package, or authoring API in this repository |
| Head at the successful link | `63edebdc5153dcc826ddf9fa962c9b83ed14ebbd` |
| Dirty at that link | `crates/sash/src/orchestrate.rs` only, for the `llvm-rc /?` identity used by that link |

kaptaind has been committing on this tree. A later commit does not change the artifact hashes below. The proof binary is not stored in git. It was written to `/tmp/sash-static-proof`, which is a host temporary directory.

`docs/reconnaissance.md` is the Neptune N0 note. This file is the Sash note.

## Authority

Sash admits scenes through `goya_lang::handoff::admit`. That handoff's contract is `GSSH/1`. The source language is GY-LANG/1 `.gscene`, parsed strictly.

`goya-ir` remains `GY-IR/1`, a legacy flow IR. It is not the window authority. Viewport lowering and `NodeTable` do not change `goya_ir::IR_CONTRACT`. Sash does not emit GY-IR/1.

The Wayland host uses `MP_PER_PX = 250` (`docs/contracts.md`, GY-WAYLAND/1). Sash does not treat that as a Windows product ratification. The static-window profile uses the same ratio only as its own mapping: 250 millipoints equal 1 device-independent pixel at 96 DPI. Extents that are not divisible by 250 are rejected. The profile does not invent a viewport when extent is missing.

## Name

Sylyca was a name in the Gestures documents. This tree has no Sylyca crate, binary, package, or configuration. Those Gestures sentences now say Sash. There is no compatibility alias. Application IDs, shell identity, and retention keys are not renamed because the compiler name changed. Aragon remains the Android compiler. Neptune remains the iOS source preview and does not link an admitted iOS application.

## What the host can prove

The smallest admitted fixture is `crates/sash/fixtures/static-window.gscene`: ontology `hello`, one cell `window.root`, text `Hello`, extent `(80, 40, 0)`. That is 320 by 160 device-independent pixels under this profile.

Command:

```text
sash build crates/sash/fixtures/static-window.gscene --profile portable-native --release --out /tmp/sash-static-proof-003
```

Result on this Linux host:

| Check | Result |
| --- | --- |
| File | `/tmp/sash-static-proof-003/App.exe`, 231424 bytes |
| `file` | PE32+ executable (GUI) x86-64, for MS Windows, 10 sections |
| SHA-256 | `d5fd86a3ccb7ec57e1f180319aebba51bd2f61f5ba1e60c0b7ff58e8fbe05028` |
| Private PDB | `/tmp/sash-static-proof-003/App.pdb`, 4431872 bytes, SHA-256 `97588c9679a812a8ad1960f19ae0de5d314d463aab7cdfd0989c09ac9ebbf164` |
| Scene SHA-256 | `617250a533dd349d6a6186d18dbb320cc1fe6a50f6d245381b9f80d004822962` |
| Machine / magic | AMD64 `0x8664`, PE32+ `0x020b` |
| Subsystem | `2`, `IMAGE_SUBSYSTEM_WINDOWS_GUI` |
| Entry point | RVA 129056, not zero |
| DLL characteristics | `0xc160`: high-entropy ASLR, ASLR, DEP/NX, control-flow guard, plus terminal-server awareness |
| Imports | `KERNEL32.dll`, `USER32.dll`, `GDI32.dll`, `ntdll.dll`, `api-ms-win-core-synch-l1-2-0.dll` |
| Delay imports | none |
| CRT redistributables | none in the import table. Static CRT was requested. This is not a zero-import claim. |
| Manifest | `asInvoker`, `uiAccess="false"`, `PerMonitorV2`. No `supportedOS` GUID. |
| `sash verify` | exit 3. `pe-structure` PASS. `runtime_qualification` BLOCKED. `release_admission` BLOCKED. |
| Launch | Not run. This host did not execute the image on Windows. Wine was not used. |

`sash build` exits 0 only after the linker returns and the PE policy passes. That exit means LINKED. It does not mean the program was launched. `sash check` admits the scene and does not link. `sash verify` does not sign, install, or launch.

The import list is the dependency evidence this host can produce. A machine with developer tools installed would not prove a clean-machine closure, and this link was not performed on Windows at all.

Profile `0.0.2` linked the same single-cell fixture to `/tmp/sash-static-proof-002/App.exe`, SHA-256 `571633c6a0830fef0ec5cd5f24ef35cd90abdb72ad8bf2f2cbdab5b4d31aa90e`. That image uses the stock GUI font and is not this emitter. An earlier single-cell link, before nested cells, was `/tmp/sash-static-proof/App.exe`, SHA-256 `ea8f4c11f2321c5669ad3b93d5cbd47346635d678e82d14c4f9ea200fccc1401`.

The nested fixture `crates/sash/fixtures/nested-static.gscene` was linked to `/tmp/sash-nested-proof-003/App.exe`, 231936 bytes, SHA-256 `d5ba33bfae91c1b0553df9b87d0d4db8f36cc03249ecf56598130d7be1c8d29c`. Its scene SHA-256 is `c663e75fda69a31d19a5c7ab4b51f89d0a6d822333892aa3849fbabcfb5c147d`. `sash verify` exited 3: `pe-structure` PASS, runtime and release BLOCKED. The generated program places `Hello` at `(0, 0, 320, 160)` DIP and `There` at `(40, 20, 80, 40)` DIP, and it creates the system message font with `SystemParametersInfoForDpi`. That placement and that font were not executed on Windows.

Library tests for this profile: `cargo test -p sash --offline --lib` reported 22 passed.

## Toolchain actually used

| Tool | Identity |
| --- | --- |
| rustc | `rustc 1.95.0 (59807616e 2026-04-14)` with the `x86_64-pc-windows-msvc` standard library installed |
| Linker orchestration | `cargo-xwin` 0.22.0, `--cross-compiler clang`, `--offline` |
| Resource compiler | `llvm-rc` LLVM Resource Converter, host Ubuntu clang 18.1.3 |
| Linker binary | `rust-lld`, which cargo-xwin exposes as `lld-link` |
| SDK | Local splat `~/.cache/xwin/windows-msvc-sysroot`, reused because `DONE` is present. Sash does not download an SDK. |

An earlier attempt used cargo-xwin's default `clang-cl` backend. It failed before promotion with `unable to find x86_64 headers for Win11SDK_10.0.26100`. That backend looks for a separate CRT unpack. The unpack under `~/.cache/xwin/xwin` is only a manifest. cargo-xwin 0.22 deletes a sysroot directory that has no `DONE` file and then downloads a replacement. The successful path refuses to start unless `DONE` and the x64 libraries are already present, then selects the clang backend so the existing splat is reused.

The build report labels the claim `unqualified-cross-link` and the host `linux-cross`. R1 and R2 were not run. Signing was not run.

## Profile that was compiled

`sash.win32.static-window/0.0.3` admits one ontology and a tree of `@cell` nodes, at most 32 cells. Labels use `NONCLIENTMETRICSW.lfMessageFont` for the window DPI. The structure size checked into the generated program is 504 bytes. A failed font at creation rejects the window. A later DPI change that cannot create a font leaves the previous font installed. DPI percentages were not measured on a device.

The root needs an id and a positive extent at depth 0. Position on the root may be omitted or be the origin. A nested cell may omit position, which places it at the parent origin, or omit extent, which fills the parent. `goya_lang::layout` is the geometry oracle. If that solver moves or shrinks a cell, admission fails with `SASH-LAYOUT`. A cell with text becomes a native `STATIC` control (`SS_LEFT|SS_NOPREFIX`) at the solved client rectangle. A cell with children and no text contributes layout only. A cell with neither text nor children is rejected. Buttons, inputs, interactions, fill, stroke, text size, anchor, wrap, scroll, proportional geometry, and strong right-to-left text are rejected. The semantic id is a window property, `Goya.SemanticId`, on the frame and on each label. UI Automation `AutomationId` is not mapped. Authored strings are emitted as decimal byte arrays.

The five directive units are not separate compilation units yet. This profile links one window and its static labels into `App.exe`. Cards, taskbar behaviour, startup, actions, and retention are not generated. Resize clips the placed rectangles and does not reflow them. Neither recorded image was launched.

## Not ratified

Exact Windows 11 builds and editions, Windows 10, LTSC, ARM64, Wine, single-exe, the installed-user profile, enterprise packaging, WinUI, notifications, startup registration, named owners, benchmark devices, signing, accessibility with Narrator, and reproducibility are unratified. An unratified item blocks that claim. It does not imply support.

`platform/windows/clipboard.rs` is an experimental `CF_UNICODETEXT` transport. It is not a Gestures adapter and Sash does not call it. `crates/goya-tov` is an argument-safe adapter to the external Tov CLI. It is not a scene manifest.
