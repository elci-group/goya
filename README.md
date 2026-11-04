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
- **Wave 2 implementation:** Tov CLI adapter, hot-swap interface reconciliation, canonical D0 bytecode/reference VM, bounded D1/D2 state-transition VMs with canonical decoding, and journal-backed D2 replay are present. Authored Goya source compiles into the documented single-button D1 and static-journal D2 subsets; typed source-compiled requests passed an opt-in local VEE submit/status/result smoke. Broader language features remain open.
- **Wave 3 implementation:** RUGID, 3form, Mesut, and Theosis adapters; C2PA signing and explicit trust-aware verification; measured-profile HRTF loading/multichannel output; opt-in RPI Level 2; and bounded D0/D1/D2 VEE Python workers are present. All three passed local live submit/status/result smokes on 2026-11-04. D0 returned the expected reference-VM JSON in its stdout artifact, prefixed by sandbox diagnostics. Landlock was unavailable (`Operation not permitted`), so the live smokes used VEE's rlimit sandbox layer only; this does not establish production sandbox coverage. The included measured HRTF fixture covers 64 directions; more subjects and the full dataset remain caller-supplied.
- **Wave 4 implementation:** GTK/AT-SPI live snapshot updates, SDK, initial Studio CLI plus loopback browser preview, reproducible local packaging and offline installer, and a deterministic watermark benchmark over synthetic or caller-selected local images with optional ffmpeg codec round trips are present. Orca navigation/WCAG conformance, a full visual IDE, and published distribution remain open. Each report describes only its listed corpus, normalization, transforms, and local ffmpeg build; it does not establish general robustness.
- Completion is not claimed for the full roadmap. See [docs/contracts.md](docs/contracts.md#deferred-work-dag-explicit-wave-assignments) and each wave's implementation notes for remaining evidence and release gates.
# goya
