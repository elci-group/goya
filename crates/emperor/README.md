# Emperor

Emperor compiles one admitted Goya scene to a native Linux ELF. Compiler version `0.0.1` implements profile `emperor.linux.interactive/0.0.1` for the Sash interactive counter only.

```
cargo run -p emperor --offline -- check crates/sash/fixtures/interactive-counter.gscene
cargo run -p emperor --offline -- build crates/sash/fixtures/interactive-counter.gscene --out /tmp/emperor-counter
```

`build` lowers the scene to `GY-EMP-NIR/1`, writes typed Rust in `embed.rs`, and links `emperor-app` with the workspace toolchain. The host triple is the only triple that links. Other directive triples return `BLOCKED` and do not write an ELF.

A Wayland smoke runs when `WAYLAND_DISPLAY` is set. Without a display, stage `runtime-conformance` stays `BLOCKED` and the build exits 3 after the oracle passes. Headless output is not a window launch. Windows execution stays blocked. Vulkan, packaging formats other than `linux-archive`, sandbox enforcement, and benchmark comparisons are not claimed.

The direct SBOM lists package names only. It is not a vulnerability review. Signing is not performed.
