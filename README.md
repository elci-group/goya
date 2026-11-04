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

The core IR, state, layout, and runtime crates have no external dependencies;
platform adapters and extensions declare their own dependencies.

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

- **Implemented milestones:** GY-001 through GY-005; GY-CHR-001; GY-WAYLAND-001; GY-FUSION-001; GY-DDH-001; GY-TYP-002; GY-LANG-001; efficiency hierarchy; calculator exemplar; Wave 2 spatial audio, bridge, anticipation, determinism, payment, artifact and watermark work. Evidence files are under `evidence/`.
- **Platform follow-through implemented:** keyboard input (evdev US subset), window resize, `pw-cat` audio output, and transactional `.gscene` hot-swap. See `evidence/GY-WAVE2-INPUT-AUDIO-HOTSWAP.json`.
- **Wave 2 implementation:** Tov CLI adapter, hot-swap interface reconciliation, and canonical D0 EDC bytecode/reference VM are present. D1/D2 executable effects remain deferred until their replay and effect contracts exist.
- **Wave 3 implementation:** RUGID, 3form, Mesut, and Theosis adapters; C2PA tool integration; host-supplied HRTF/multichannel output; opt-in RPI Level 2; and VEE submission API client are present. Goya bytecode execution through VEE is not implemented because VEE has no Goya worker/compiler, and no live VEE server was available for end-to-end verification. HRTF quality still needs measured profiles.
- **Wave 4 implementation:** GTK/AT-SPI bridge, SDK, initial Studio CLI, reproducible local packaging, and deterministic watermark robustness benchmark are present. Orca speech/WCAG conformance, a full visual IDE and distribution release remain open. The watermark corpus uses synthetic images and pixel-domain codec surrogates; it does not establish general codec or photographic robustness.
- Completion is not claimed for the full roadmap. See [docs/contracts.md](docs/contracts.md#deferred-work-dag-explicit-wave-assignments) and each wave's implementation notes for remaining evidence and release gates.
# goya
