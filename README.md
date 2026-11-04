# Goya — Native Desktop Application Platform

Implementation of directive GY-ENT-001 (Rev 1.0, 2026-11-04). Rust + Tov;
initial target Linux/Wayland. Wave 1 (native foundation) is complete and
evidenced; see `evidence/GY-WAVE1-001.json` and `docs/contracts.md`.

## Workspace

| Crate | Contract | Role |
|---|---|---|
| `goya-ir` | GY-IR/1 | Typed IR, declarative parser, capability schema |
| `goya-state` | GY-STATE/1 | Revisioned single-writer state transactions |
| `goya-layout` | GY-LAYOUT/1 | Deterministic pure-function layout (millipoint integers) |
| `goya-runtime` | GY-RUNTIME/1 | Event dispatch: input → state → layout → render list |
| `goya-chroma` | GY-CHROMA/1 | Relational OKLCH theming, grey-line polarity, contrast solver |
| `goya-provenance` | GY-PROVENANCE/1 | Bounded, redacted Deep Copy diagnostic lineage |
| `goya-clipboard` | GY-CLIPBOARD/1 | Capability-gated verbatim/deep clipboard |
| `goya-sonic` + `goya-spatial-audio` + `goya-audio-dsp` + `goya-audio-linux` | GY-SONIC/1… | ARSEM: geometry→pan/gain, DSP, device lifecycle |
| `goya-bridge` + `goya-bridge-physics` | GY-BRIDGE/1 | NSB: retargetable spring state bridges |
| `goya-rpi` + `goya-speculation` | GY-RPI/1, GY-SPECULATION/1 | Anticipatory expression under budget |
| `goya-determinism` + `goya-executable` | GY-DETERMINISM/1 | EDC classification & artifact registry |
| `goya-x402` + `goya-economics` + `goya-settlement` | GY-X402/1… | Payment challenges, allowances, reconciliation |
| `goya-artifact` + `goya-watermark` | GY-ARTIFACT/1 | Byte/perceptual/provenance identity, DCT marks |
| `goya-wayland` | GY-WAYLAND/1 | Live Wayland window, pointer → runtime, shm presentation |
| `goya-hello` | — | GY-005 proof: headless PPM + `--live` window; `buttons` bin = ARSEM acceptance |

Zero external dependencies — the foundation is fully self-contained.

## Build and verify

```sh
cargo build --workspace
cargo test --workspace --lib --tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo run --bin goya-hello   # renders target/goya-hello-frame-*.ppm
```

Known host issue: rustdoc cannot load its LLVM shared library here, so
doctests are excluded (`--lib --tests`); they assert nothing.

## Status

- ✅ GY-001 reconnaissance · GY-002 contracts · GY-003/004 foundation · GY-005 proof
- ⬜ GY-WAYLAND-001: live Wayland surface (contract consumer of GY-RUNTIME/1)
- ✅ GY-CHR-001: chromatic relativism, Deep Copy provenance, dual-mode clipboard
- ✅ Wave 2 (10-agent swarm): ARSEM spatial audio, NSB bridges, RPI + speculation, EDC determinism, native x402, artifact identity & DCT watermarking — 15 new crates, contracts GY-SONIC/1 … GY-WATERMARK/1
- ✅ GY-WAYLAND-001: live Wayland surface (release 0.1 gate met); ARSEM 9/9 acceptance; parser under 20k-input mutation fuzz
- ✅ GY-FUSION-001: fused milestone live — NSB retargetable bridges + ARSEM presentation-tracked pan + chromatic polarity + RPI/speculation gating in one interactive object (10/10 checks)
- ✅ **GY-DDH-001 Deterministic Dehumanisation**: semantic UI traversal — text-only agent computes 87×42=3654 through the calculator's DIG with **2125× fewer bytes than a screenshot**; leases, authority gates, exposure policies (`cargo run --bin calculator -- --ddh`)
- ✅ **Audit remediation**: triple-buffered presentation (never write unreleased slots), incremental damage-scoped raster, GitHub Actions CI
- ✅ **GY-TYP-002 Formatting Similarity Compression** (`goya-fsc`): shared immutable formatting profiles with inheritance-aware identity, typed in-object deltas with stable anchors, run-based resolution, directive-table invalidation scopes, 100k-char acceptance test
- ✅ **Efficiency hierarchy (GY-LCPE/RARS/CSR/HCAR-001)**: change envelopes + causal invalidation + quantised damage (LCPE), deterministic projection fidelity with hysteresis (RARS), spatial partition scheduling with work stealing (CSR), heterogeneous core classification (HCAR) — wired into the calculator: presses emit quantised local damage; glyph fidelity resolved from projected area
- ✅ **Exemplar app: `apps/calculator.gscene`** — a full calculator authored in Goya scene syntax (`cargo run --bin calculator -- --live --sound`); headless acceptance: 7+3=10, 10*5=50, 50/0=Error, decimals, sign
- ✅ Flicker fix: double buffering (buffer pair + Release sync), opaque region, dirty-flag redraw (idle 7200 commits → 1)
- ✅ Wave 2 input/audio/hot-swap: keyboard typing (evdev US subset), window resize, **real audible PipeWire audio** (pw-cat adapter), transactional `.gscene` hot-swap (`--watch`; broken edits rejected, state retained)
- ✅ GY-LANG-001: **Goya language revised to RUGID-compatible scene syntax** — `@ontology`/`@cell`/`@button` with absolute + proportional geometry; build a GUI from a `.gscene` file: `cargo run --bin scene --file app.gscene --live`
- ⬜ Wave 2a: PipeWire backend; Wave 2b: hot-swap + Tov; Wave 3: RUGID/3form/Mesut/Theosis, C2PA, HRTF; Wave 4: a11y conformance, packaging
- ⬜ Wave 2: Tov adapters, hot-swap, interface reconciliation
- ⬜ Wave 3: RUGID, 3form, Mesut, Theosis, Vico Vee adapters
# goya
