# Emperor

Emperor compiles one admitted Goya scene to a native Linux ELF. Compiler version `0.0.1` implements profile `emperor.linux.interactive/0.0.1` for the Sash interactive counter only.

```
cargo run -p emperor --offline -- check crates/sash/fixtures/interactive-counter.gscene
cargo run -p emperor --offline -- build crates/sash/fixtures/interactive-counter.gscene --out /tmp/emperor-counter
```

`build` lowers the scene to `GY-EMP-NIR/1`, writes typed Rust in `embed.rs`, and links `emperor-app` with the workspace toolchain. The host triple is the only triple that links. Other directive triples return `BLOCKED` and do not write an ELF.

`qualify` re-checks one build directory on this host: it repeats the static verification of `verify` (build report, sha256 match, ELF checks), runs the headless oracle, and runs the windowed smoke when a display is reachable. Its report carries `qualification_scope: "local-release"`: it does not claim the multi-machine linux-release matrix, Windows parity, or signing, which stay `BLOCKED` in the final stage. Exit 4 on static failure, 5 on a runtime failure, 0 when the oracle passed even if the smoke stayed blocked.

`benchmark` measures one build directory on this host, following the `scripts/perf-baseline.sh` convention (1 warmup, then 5 timed runs, min/median/mean reported). It measures two latencies: the wall time of `App --oracle` as a subprocess (which includes process startup and is therefore a different, larger number than the in-process oracle timings recorded in `evidence/emperor/performance/`), and the in-process cost of the static ELF verification pass. Machine context (CPU model from `/proc/cpuinfo`, core count, `rustc --version`) is recorded next to the numbers. When a display is reachable, the windowed smoke wall time is measured as a third benchmark; that is launch latency, not frame rate. Its report carries `benchmark_scope: "local-host"`: these numbers describe this machine and no cross-machine comparison is claimed. Frame-rate benchmarking stays `BLOCKED` — it needs a display and a frame-presented counter, neither of which exists in compiler 0.0.1 — as does the multi-machine matrix, in the final stage. Exit 0 when every attempted measurement succeeded, 4 on the static failures `verify` reports, 5 when `App` fails to run, 2 on usage problems.

A Wayland smoke runs when `WAYLAND_DISPLAY` is set. Without a display, stage `runtime-conformance` stays `BLOCKED` and the build exits 3 after the oracle passes. Headless output is not a window launch. `--window kumimanu` links the X11 host instead and smokes it when `DISPLAY` is set; the default remains Wayland. The X11 host owns `CLIPBOARD` for copy and cut and converts it for paste, under the same capability gates as Wayland. Text over 256 KiB moves by INCR in 64 KiB chunks, capped at 1 MiB in either direction. `cargo test -p goya-kumimanu -- --ignored` checks this against GTK 3 on a private selection when `DISPLAY` and PyGObject are present. `PRIMARY` is not used. Windows execution stays blocked. Vulkan, packaging formats other than `linux-archive`, sandbox enforcement, and benchmark comparisons are not claimed.

The direct SBOM lists package names only. It is not a vulnerability review. Signing is not performed.
