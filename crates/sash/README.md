# Sash

Sash is Goya's Windows compiler. It admits a GY-LANG/1 scene and, for the one linked profile, produces a native `App.exe`.

Aragon remains the Android compiler. Neptune remains the iOS compiler. Sylyca is the former name of this Windows backend, not a second toolchain.

## What links today

Profile `sash.win32.static-window/0.0.1` on target `x86_64-pc-windows-msvc`.

The scene must contain one ontology and exactly one `@cell` with an id, text, and a positive extent. The fixture `fixtures/static-window.gscene` is the smallest admitted input. Buttons, inputs, nested cells, interactions, and retention are rejected.

The generated program is a Win32 window with one `STATIC` label. It is not an Electron, WebView, or WinUI application.

## Commands

```text
sash inspect project.gscene
sash check project.gscene --target x86_64-pc-windows-msvc
sash build project.gscene --profile portable-native --release --out /path/to/new-dir
sash verify /path/to/new-dir/App.exe --evidence /path/to/new-dir/evidence
```

`check` does not create an executable. `build` exits 0 only after the linker produces an image that passes the PE policy. That success still reports runtime qualification and release admission as BLOCKED. `verify` exits 3 when the PE checks pass and the image has not been launched on Windows. It does not sign, install, or start the program.

`--out` must be a new directory. A failed build does not leave an `App.exe` there.

## This host

Linux can cross-link when the Rust Windows standard library, `cargo-xwin` 0.22, `llvm-rc`, `rust-lld`, and a local `windows-msvc-sysroot` marked `DONE` are already present. Sash does not download that SDK. The build report calls the result an unqualified cross-link.

A Windows launch was not run for the proof recorded in `docs/sash/reconnaissance.md`. Import inspection is not a clean-machine test.

## Tests

```text
cargo test -p sash --offline --lib
```
