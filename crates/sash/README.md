# Sash

Sash is Goya's Windows compiler. It admits a GY-LANG/1 scene and produces a native `App.exe` for one of three Windows architectures.

Aragon remains the Android compiler. Neptune remains the iOS compiler. Sylyca is the former name of this Windows backend, not a second toolchain.

## What links today

Profiles `sash.win32.static-window/0.0.3`, `sash.win32.interactive/0.0.1`, and `sash.win32.interactive/0.0.2` select among `i686-pc-windows-msvc`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc`. i686 is PE32. x64 and ARM64 are PE32+. None of the three has been launched on Windows. A successful build is still an unqualified cross-link.

Labels use the system message font for the window DPI. The font is recreated on `WM_DPICHANGED`. Authored text size is still rejected. DPI scaling has not been executed on a Windows machine.

The scene is one ontology whose root `@cell` has an id and a positive extent. Nested `@cell` nodes are admitted, up to 32 cells. Each cell with text becomes a `STATIC` control at the rectangle from `goya_lang::layout`. A cell with neither text nor children is rejected. A cell the solver would move or shrink is rejected. The fixture `fixtures/static-window.gscene` is the smallest admitted input. `fixtures/nested-static.gscene` is the nested proof. Buttons, inputs, interactions, fill, stroke, proportional geometry, and retention are rejected by this profile.

Profile `sash.win32.interactive/0.0.1` is selected by `check` and `build` when the scene contains a button, an input, or an interaction and the edit states no gesture policy. It admits one heading, one canonical integer edit, one button, one result cell whose id is `state.<key>`, and one D1 integer addition. Overflow and a non-canonical edit leave the previous value unchanged. That edit keeps the system clipboard.

Profile `sash.win32.interactive/0.0.2` is selected when that edit has a `gestures` block stating `copy` and `paste` as `allow` or `deny`. `cut` may be stated. An omitted cut leaves the system cut command. A denied cut also drops Shift+Delete. Delete without Shift is not a cut. Select, undo, and redo are not policy operations; authoring them is a syntax error. A denied paste does not change the edit. The static-window profile rejects every gestures block. Enter activates the button. Tab and Shift-Tab move between the edit and the button. Space is delivered to the focused control. The result text is written before the value is stored. The stored value changes only when that text reads back as the new value. A different readback restores the previous result text while the window is live. A second activation while that text is being written is ignored. A release that starts during that write drops the commit. A failed text update leaves the previous value. Close releases the font, semantic ids, and edit subclass, then unregisters the window class. Neither interactive executable has been launched on Windows.

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

Linux can cross-link when the Rust standard library for the selected triple, `cargo-xwin` 0.22, `llvm-rc`, `rust-lld`, and a local `windows-msvc-sysroot` marked `DONE` are already present. Sash does not download a target or that SDK. The link passes `/Brepro`, names the build directory from the generated crate and the target, and gives lld one stable `symbols.o` path. Two links of `fixtures/static-window.gscene` and two links of `fixtures/interactive-counter.gscene` on this host produced the same `App.exe` bytes and the same `App.pdb` bytes for x64, i686, and ARM64. Those pair hashes predate the manifest stamp and still say `processorArchitecture="amd64"` on i686 and ARM64, so the current verifier rejects them. One later static link embeds `x86` at `/tmp/sash-stamp-i686/App.exe` and `arm64` at `/tmp/sash-stamp-arm64/App.exe`. Each of those is a single link, not a second pair. The embedded manifest `processorArchitecture` follows the triple: `x86`, `amd64`, or `arm64`. A mismatch fails promotion. The build report calls the result an unqualified cross-link and leaves R1, R2, and the same-host pair unclaimed, because one link does not compare a second worker. `verify` rejects a report that marks those claims PASS. None of those images was launched.

A Windows launch was not run for the proof recorded in `docs/sash/reconnaissance.md`. Import inspection is not a clean-machine test.

## Tests

```text
cargo test -p sash --offline --lib
```
