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
| `goya-zcode` | GY-ZCODE/1 | Native ZCode session monitor (`zcode-monitor` bin, send-capable); see docs/goya-zcode.md |

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
- **goya-zcode (waves 1–10):** Ten swarm waves grew the ZCode session monitor from a read-only transcript mirror into a send-capable session client: pilot monitor (1); editable composer with KeySpecial input (2); `session/send` behind `--allow-send` plus scrollable history (3); palette theming, Ctrl+C/V clipboard ring, and markdown blocks (4); IME text input and tool-call rows (5); multi-session switching (6); per-session transcript retention and a11y input vocabulary (7); live a11y snapshot (`--a11y-dump`) and Wayland data-device clipboard (8); per-node text colors and a system-clipboard-backed ring (9); tool-row progress stats, protocol robustness tests, and docs consolidation (10). Today the monitor streams a markdown/tool-row transcript, offers an IME-capable composer, system clipboard, per-session history, theming, an a11y snapshot, and multi-session switching. Deferred by design: proportional text, richer tool-call blocks (needs a UI-model decision), the GTK AT-SPI bridge, and operator verification of real screen-reader/clipboard interop. Evidence under `evidence/GY-ZCODE-001.json`…`GY-ZCODE-010.json`.
- Completion is not claimed for the full roadmap. See [docs/contracts.md](docs/contracts.md#deferred-work-dag-explicit-wave-assignments) and each wave's implementation notes for remaining evidence and release gates.
# goya

## Native frontend development (experimental)

The native successor is `goya-zcode-native` (`zcode-native`), documented in
[docs/goya-zcode-native.md](docs/goya-zcode-native.md). The original monitor
and its local work are retained. The successor consumes v4 durable
projections and command outcomes; it is not a full ZCode frontend replacement.

| Crate | Development capability | Qualification limits |
| --- | --- | --- |
| `goya-zcode-native` | v4 client, resize-aware scene and native IME handling | Missing full shell, interaction controls and existing-Host attachment |
| `goya-webview` | Owned sandboxed Chromium pixels and input in a native Goya viewport | Engine packaging, accessibility and full browser chrome remain open |
| `goya-document` | Sandboxed Linux PDF raster worker and native presentation | PDF raster only; Office/media, text selection and a11y remain open |
| `goya-plugin-host` | Per-instance origin, rooted static assets and capability validation | No live Host RPC bridge or plugin lifecycle UI |

From the workspace root:

```sh
cargo run -p goya-zcode-native --bin zcode-native
cargo run -p goya-webview --bin goya-browser -- http://127.0.0.1:18189/webview-fixture.html
cargo run -p goya-document -- --file fixtures/documents/one-page.pdf --live --duration 3
cargo run -p goya-plugin-host --example native-plugin
```

Serve the synthetic browser fixture with
`python3 -m http.server 18189 --bind 127.0.0.1 --directory apps` before using
that browser example. Real-browser/PDF qualification tests are explicitly
ignored by the default suite and must be executed separately. Their success
is not evidence of full parity, Windows/macOS support, or screen-reader use.
