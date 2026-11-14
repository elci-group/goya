# ADR 0003 — Toolchain and evidence

Status: accepted for the current Linux cross-link. Runtime qualification stays blocked.

## Decision

Sash may call an existing Rust compiler and PE linker. It does not implement its own code generator.

On this host the link is:

- `rustc` 1.95.0, target `x86_64-pc-windows-msvc`
- `cargo-xwin` 0.22.0 with `--cross-compiler clang` and `--offline`
- the local `windows-msvc-sysroot` only when `~/.cache/xwin/windows-msvc-sysroot/DONE` exists
- `rust-lld` as `lld-link`
- `llvm-rc` for the manifest and version resource

`clang-cl` is not selected. Its CRT unpack is incomplete here, and cargo-xwin 0.22 deletes a sysroot that lacks `DONE` before downloading another. Sash fails with `SASH-TOOLCHAIN` when the marker or the x64 libraries are missing. It does not start that download. A native log that reports MSVC CRT setup failure, a missing SDK header, or a failed SDK download is also `SASH-TOOLCHAIN`. Resource-compiler and linker errors stay `SASH-LINK`. Both codes leave the output directory unpromoted.

Static CRT and control-flow guard are requested in the generated `.cargo/config.toml`. The PE policy checks the bits in the linked image. A requested flag that is absent from the image fails promotion.

The output directory is created only after those checks pass. Failure removes that directory. `verify` never signs, installs, or launches. A report that marks runtime qualification or release admission as PASS is rejected.

Linux output is labelled `unqualified-cross-link`. Wine is not evidence. R1 and R2 are `NOT_RUN` until two comparable builds exist.

## Consequences

Import-table inspection is the dependency evidence of a cross-link. It does not prove a clean Windows machine. `api-ms-win-core-synch-l1-2-0.dll` is disclosed as a system API set. Static CRT is not a claim of zero system imports.

Scene digests in evidence are the hex of the GY-LANG SHA-256, not a second hash of that digest.
