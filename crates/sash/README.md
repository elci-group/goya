# Sash

Sash is Goya's Windows compiler. It admits bounded Goya scenes and produces a native `App.exe` for one of three Windows architectures.

Aragon remains the Android compiler. Neptune remains the iOS compiler. Sylyca is the former name of this Windows backend, not a second toolchain.

## What links today

Profiles `sash.win32.static-window/0.0.3`, `sash.win32.interactive/0.0.1`, and `sash.win32.interactive/0.0.2` select among `i686-pc-windows-msvc`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc`. i686 is PE32. x64 and ARM64 are PE32+. None of the three has been launched on Windows. A successful build is still an unqualified cross-link.

Labels use the system message font for the window DPI. The font is recreated on `WM_DPICHANGED`. Authored text size is still rejected. DPI scaling has not been executed on a Windows machine.

By default, static scenes use GSSH/1. The scene is one ontology whose root `@cell` has an id and a positive absolute extent. Nested `@cell` nodes are admitted, up to 32 cells. Each cell with text becomes a `STATIC` control at the rectangle from `goya_lang::layout`. A cell with neither text nor children is rejected. A cell the solver would move or shrink is rejected. The fixture `fixtures/static-window.gscene` is the smallest admitted input. `fixtures/nested-static.gscene` is the nested proof.

### Proportional static scenes

`--viewport-dip WIDTHxHEIGHT` selects GSSH/3 static admission for `inspect`, `check`, `expectations`, and `build`. Each edge must be an integer from 1 through 4096. The root must resolve to the full viewport at the origin. One DIP is 250 millipoints. `goya_lang::layout` resolves nested `position_p` and `extent_p` against the parent, including the language's clamping rules. Resolved coordinates and sizes must land exactly on the DIP grid, with positive sizes and zero authored depth. The compiler flattens text cells in tree order into the existing Win32 static-window adapter. The client size is fixed at build time and labels clip on resize.

The proportional slice accepts plain static cells and text. Fill, stroke, authored text size, anchors, wrapping, scrolling, gestures, controls, state, and interactions are rejected. The manifest uses the existing presentation profile for its API floor. Scene-command JSON identifies the frontend as `GSSH/3` and records the viewport.

```text
sash expectations crates/sash/fixtures/proportional-static.gscene --viewport-dip 640x480 --format json
sash build crates/sash/fixtures/proportional-static.gscene --viewport-dip 640x480 --release --out /path/to/new-dir
```

### Accessible identity (`@iden`)

A proportional static scene that declares `@iden` (GSSH/4, decision D14) is admitted as GSSH/4; `expectations` reports `frontend_contract: "GSSH/4"` and a `uia` object per labelled control. The label becomes the control's accessible name and the hint its description and help, set through Win32 dynamic annotation (`IAccPropServices::SetHwndPropStr` with `PROPID_ACC_NAME`, `PROPID_ACC_DESCRIPTION`, `PROPID_ACC_HELP`), which UI Automation exposes as Name, FullDescription and HelpText. COM is bound at run time with `LoadLibraryW`/`GetProcAddress`, so the import closure is unchanged; a failure to annotate refuses to show the window. Annotations are cleared on `WM_DESTROY`. Scenes without `@iden` generate byte-identical programs. An `@iden` on a cell without text is refused (no control carries it), and the interactive profiles, which are GSSH/1, refuse it. `fixtures/iden-static.gscene` is the proof input; its program type-checks for all three architectures on both floors, but has not been linked here or launched on Windows.

Profile `sash.win32.interactive/0.0.1` is selected by `check` and `build` when the scene contains a button, an input, or an interaction and the edit states no gesture policy. It admits one heading, one canonical integer edit, one button, one result cell whose id is `state.<key>`, and one D1 integer addition. Overflow and a non-canonical edit leave the previous value unchanged. That edit keeps the system clipboard.

Profile `sash.win32.interactive/0.0.2` is selected when that edit has a `gestures` block stating `copy` and `paste` as `allow` or `deny`. `cut` may be stated. An omitted cut leaves the system cut command. A denied cut also drops Shift+Delete. Delete without Shift is not a cut. Select, undo, and redo are not policy operations; authoring them is a syntax error. A denied paste does not change the edit. The static-window profile rejects every gestures block. Enter activates the button. Tab and Shift-Tab move between the edit and the button. Space is delivered to the focused control. The result text is written before the value is stored. The stored value changes only when that text reads back as the new value. A different readback restores the previous result text while the window is live. A second activation while that text is being written is ignored. A release that starts during that write drops the commit. A failed text update leaves the previous value. Close releases the font, semantic ids, and edit subclass, then unregisters the window class. Neither interactive executable has been launched on Windows.

### Interactive counter on GSSH/4-live

With `--viewport-dip`, a scene with controls is admitted as GSSH/4-live (decision D15) by the same interactive profiles (`0.0.1`/`0.0.2`, and their legacy floors). The initial value comes from `@state` (session only; durable is refused until a Windows store exists), the edit is bound with `input { amount: int from "<edit id>"; }`, and the result cell shows the value with `text-from: state.<key>;` and no text. Every `@input` needs `@iden`; any control may have one, and it becomes the UI Automation Name and HelpText through dynamic annotation. The result is a polite live region: after each stored change its name becomes the D9 announcement (`count increased by 1, from 0 to 1`) and `EVENT_OBJECT_LIVEREGIONCHANGED` is raised. `fixtures/interactive-live.gscene` (320x160 dip) links reproducibly for x64, x86 and ARM64 on both floors and passes `sash verify`; it has not been launched. GSSH/1 counters generate byte-identical programs.

### Windows host

Sash compiles on Windows. `check`, `inspect`, `expectations` and `verify` work there; `build` refuses, because the cross-link uses Linux tools (rust-lld through a Python shim, the pinned xwin sysroot).

## API floors

`--api-floor win10` (default) emits the per-monitor-v2 adapter of the first
proof. `--api-floor win8` emits the system-DPI-aware legacy adapter: it drops
every Windows 10 1703+ per-monitor API from the import table, reads the
process DPI once from `GetDeviceCaps`, and names a `.Legacy` assembly so
`sash verify` reports the floor the image was built for. The scene admits
identically on both floors; only the Win32 adapter differs.

The win8 floor is the lowest this toolchain can produce: the stable
`pc-windows-msvc` standard library imports `api-ms-win-core-synch-l1-2-0.dll`
(`WaitOnAddress`, Windows 8+). Windows 7, Vista, and XP need the tier-3
`*-win7-windows-msvc` standard library that only a nightly `-Z build-std`
toolchain can build, so those floors are refused with that reason. `win8`
refuses ARM64, because ARM64 Windows did not exist before Windows 10.

No image of any floor has been launched on any Windows version. The floor is
an import-table and manifest declaration, not a support claim.

The generated program is a Win32 window. It is not an Electron, WebView, or WinUI application.

## Commands

```text
sash inspect project.gscene
sash check project.gscene --target i686-pc-windows-msvc
sash check project.gscene --target x86_64-pc-windows-msvc
sash build project.gscene --target aarch64-pc-windows-msvc --profile portable-native --release --out /path/to/new-dir
sash verify /path/to/new-dir/App.exe --evidence /path/to/new-dir/evidence
```

`check` does not create an executable. `build` exits 0 only after the linker produces an image that passes the PE policy. That success still reports runtime qualification and release admission as BLOCKED. `verify` exits 3 when the PE checks pass and the image has not been launched on Windows. It reports the triple of the PE machine and the presentation profile named by the manifest assembly identity. The static-window assembly version `0.0.1.0` is profile `sash.win32.static-window/0.0.3`. An explicit `--target` that is not that machine fails verify. `verify` does not sign, install, or start the program.

`--out` must be a new directory. A failed build does not leave an `App.exe` there.

## This host

Linux can cross-link when the Rust standard library for the selected triple, `cargo-xwin` 0.22, `llvm-rc`, `rust-lld`, and a local `windows-msvc-sysroot` marked `DONE` are already present. The import libraries for that triple must be in the sysroot. An x64 directory does not satisfy i686 or ARM64. Sash does not download a target or that SDK. The link passes `/Brepro`, names the build directory from the generated crate and the target, and gives lld one stable `symbols.o` path. Two links of `fixtures/static-window.gscene` and two links of `fixtures/interactive-counter.gscene` on this host produced the same `App.exe` bytes and the same `App.pdb` bytes for x64, i686, and ARM64. Those pair hashes predate the manifest stamp and still say `processorArchitecture="amd64"` on i686 and ARM64, so the current verifier rejects them. One later static link embeds `x86` at `/tmp/sash-stamp-i686/App.exe` and `arm64` at `/tmp/sash-stamp-arm64/App.exe`. One later interactive link embeds `x86` at `/tmp/sash-stamp-ix86/App.exe` and `arm64` at `/tmp/sash-stamp-iarm64/App.exe`. Each of those is a single link, not a second pair. The embedded manifest `processorArchitecture` follows the triple: `x86`, `amd64`, or `arm64`. A mismatch fails promotion. The build report calls the result an unqualified cross-link and leaves R1, R2, and the same-host pair unclaimed, because one link does not compare a second worker. `verify` rejects a report that marks those claims PASS. None of those images was launched.

A Windows launch was not run for the proof recorded in `docs/sash/reconnaissance.md`. Import inspection is not a clean-machine test.

## Tests

```text
cargo test -p sash --offline --lib
```

The linked-artifact reproduction gate links both fixtures for every admitted
triple and floor and asserts the recorded SHA-256 pairs. It performs ten
cross-links, so it is ignored by default:

```text
cargo test -p sash --offline --test link_reproduction -- --ignored --nocapture
```

A deliberate toolchain bump or emitter change fails this gate by design.
Update the recorded hashes only through a new programme record, never by
loosening the assertion.
