# Emperor

Emperor compiles one admitted Goya scene to a native Linux ELF. Compiler version `0.0.1` implements profile `emperor.linux.interactive/0.0.1` for the Sash interactive counter only.

```
cargo run -p emperor --offline -- check crates/sash/fixtures/interactive-counter.gscene
cargo run -p emperor --offline -- build crates/sash/fixtures/interactive-counter.gscene --out /tmp/emperor-counter
```

`build` lowers the scene to `GY-EMP-NIR/1`, writes typed Rust in `embed.rs`, and links `emperor-app` with the workspace toolchain. The host triple is the only triple that links. Other directive triples return `BLOCKED` and do not write an ELF.

`qualify` re-checks one build directory on this host: it repeats the static verification of `verify` (build report, sha256 match, ELF checks), runs the headless oracle, and runs the windowed smoke when a display is reachable. Its report carries `qualification_scope: "local-release"`: it does not claim the multi-machine linux-release matrix, Windows parity, or signing, which stay `BLOCKED` in the final stage. Exit 4 on static failure, 5 on a runtime failure, 0 when the oracle passed even if the smoke stayed blocked.

A Wayland smoke runs when `WAYLAND_DISPLAY` is set. Without a display, stage `runtime-conformance` stays `BLOCKED` and the build exits 3 after the oracle passes. Headless output is not a window launch. `--window kumimanu` links the X11 host instead and smokes it when `DISPLAY` is set; the default remains Wayland. The X11 host owns `CLIPBOARD` for copy and cut and converts it for paste, under the same capability gates as Wayland. Text over 256 KiB moves by INCR in 64 KiB chunks, capped at 1 MiB in either direction. `cargo test -p goya-kumimanu -- --ignored` checks this against GTK 3 on a private selection when `DISPLAY` and PyGObject are present. `PRIMARY` is not used. Windows execution stays blocked. Vulkan, packaging formats other than `linux-archive`, sandbox enforcement, and benchmark comparisons are not claimed.

The direct SBOM lists package names only. It is not a vulnerability review. Signing is not performed.
