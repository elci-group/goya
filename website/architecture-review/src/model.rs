//! Architecture model served to the review page.
//!
//! Crate edges match workspace manifests. Wave notes follow `docs/contracts.md`
//! and the root README. This is a review map, not a second contract.

pub struct Layer {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

pub struct Crate {
    pub id: &'static str,
    pub layer: &'static str,
    pub contract: &'static str,
    pub status: &'static str,
    pub role: &'static str,
}

pub struct Wave {
    pub id: &'static str,
    pub wave: &'static str,
    pub name: &'static str,
    pub status: &'static str,
    pub note: &'static str,
}

pub struct Finding {
    pub kind: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

pub struct Boundary {
    pub id: &'static str,
    pub name: &'static str,
    pub via: &'static str,
    pub note: &'static str,
}

pub struct Unit {
    pub name: &'static str,
    pub crate_name: &'static str,
    pub detail: &'static str,
}

pub struct PathView {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub units: &'static [Unit],
}

pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub crate_name: &'static str,
    pub detail: &'static str,
    pub state: &'static str,
}

pub struct Trace {
    pub id: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    pub revision: &'static str,
    pub steps: &'static [Step],
}

pub fn layers() -> &'static [Layer] {
    &[
        Layer { id: "shells", title: "Shells", summary: "Exemplar hosts and experimental native frontends." },
        Layer { id: "host", title: "Host adapters", summary: "Wayland, windowing, audio, and the AT-SPI bridge." },
        Layer { id: "core", title: "Frame core", summary: "IR, single-writer state, pure layout, synchronous runtime, OKLCH theme." },
        Layer { id: "scene", title: "Scene and SDK", summary: "GY-LANG/1 authoring, the Rust facade, Studio, Tov, and companion adapters." },
        Layer { id: "execute", title: "Checked execution", summary: "EDC classification, D0/D1/D2 VMs, VEE workers, and hot-swap migration." },
        Layer { id: "efficiency", title: "Efficiency", summary: "Change potential, core split, compression, and deterministic dehumanisation." },
        Layer { id: "identity", title: "Identity", summary: "Provenance, clipboard, artifact identity, and watermark measurement." },
        Layer { id: "economy", title: "Settlement", summary: "x402 challenges, allowances, and reconciliation." },
        Layer { id: "anticipate", title: "Motion and anticipation", summary: "Springs, probability fields, and opt-in level-2 prediction." },
        Layer { id: "compilers", title: "Platform compilers", summary: "Sash for one Win32 window. Neptune for Swift source previews. Dockyard for one static HTTP executable and an OCI layout." },
    ]
}

pub fn crates() -> &'static [Crate] {
    &[
        Crate { id: "goya-hello", layer: "shells", contract: "GY-APP-001", status: "implemented", role: "Exemplar hosts. Each apps/*.gscene declares layout. The matching bin owns state and runs a headless self-test unless --live." },
        Crate { id: "goya-zcode", layer: "shells", contract: "GY-ZCODE/1", status: "implemented", role: "Send-capable ZCode session monitor: composer, clipboard, markdown rows, themes, and an accessibility snapshot. Proportional text and richer tool blocks remain deferred." },
        Crate { id: "goya-zcode-native", layer: "shells", contract: "native", status: "experimental", role: "v4 durable-projection client with resize-aware scene and native IME. A full shell, interaction controls, and existing-host attachment are not in this crate." },
        Crate { id: "goya-webview", layer: "shells", contract: "webview", status: "experimental", role: "Owned sandboxed Chromium viewport composited into a Goya Wayland surface. Navigation is credential-free http and https. Engine packaging, accessibility, and full chrome stay open." },
        Crate { id: "goya-document", layer: "shells", contract: "document", status: "experimental", role: "Sandboxed Linux PDF raster worker and native presentation. Office, media, text selection, and accessibility are outside this crate." },
        Crate { id: "goya-plugin-host", layer: "shells", contract: "plugin-host", status: "experimental", role: "Per-instance origin, rooted static assets, and capability validation. No live host RPC bridge or plugin lifecycle UI." },
        Crate { id: "goya-wayland", layer: "host", contract: "GY-WAYLAND/1", status: "implemented", role: "Live Wayland window. Pointer and keyboard enter the runtime. Frames are ARGB8888 shared memory. GoyaWindow::pump drains queued seat events, including data_offer." },
        Crate { id: "goya-window-management", layer: "host", contract: "window actions", status: "implemented", role: "Platform-neutral close, maximize, restore, minimize, and new-tab actions. Floating and tiling bins sit on this contract." },
        Crate { id: "goya-a11y", layer: "host", contract: "AT-SPI", status: "partial", role: "GTK/AT-SPI live snapshot bridge. Orca navigation, announcements, and WCAG conformance remain open." },
        Crate { id: "goya-sonic", layer: "host", contract: "GY-SONIC/1", status: "implemented", role: "Sonic event policy for the audio path." },
        Crate { id: "goya-spatial-audio", layer: "host", contract: "GY-SPATIAL-AUDIO/1", status: "partial", role: "Geometry to pan and gain, plus measured HRTF. The included fixture is 64 directions of one Viking subject." },
        Crate { id: "goya-audio-dsp", layer: "host", contract: "GY-AUDIO-DSP/1", status: "implemented", role: "Mixer, limiter, and synthesis used before device output." },
        Crate { id: "goya-audio-linux", layer: "host", contract: "GY-AUDIO-LINUX/1", status: "implemented", role: "Linux audio adapter. Output goes through pw-cat on the PipeWire path." },
        Crate { id: "goya-ir", layer: "core", contract: "GY-IR/1", status: "implemented", role: "Typed module: Column, Row, Text, Button, Spacer, and Input. Ids are unique. Parsing is total. No external crates." },
        Crate { id: "goya-state", layer: "core", contract: "GY-STATE/1", status: "implemented", role: "Single-writer transactions. begin, set or one stage_snapshot, then commit. Drop rolls back. Revision bumps only on success. No external crates." },
        Crate { id: "goya-layout", layer: "core", contract: "GY-LAYOUT/1", status: "implemented", role: "Pure layout of a module and constraints. Geometry is millipoints, 1 point = 1000 mp. A row spacer is a horizontal gap. No clock, locale, or RNG." },
        Crate { id: "goya-runtime", layer: "core", contract: "GY-RUNTIME/1", status: "implemented", role: "Synchronous handle: hit test, text, scroll, and actions. Layout runs again only after the state revision advances. Depends on unicode-segmentation, speculation, and rpi2." },
        Crate { id: "goya-chroma", layer: "core", contract: "GY-CHROMA/1", status: "implemented", role: "OKLCH themes. Polarity reflects lightness through GREY_LINE 0.6. Unreachable WCAG contrast returns ContrastUnreachable. No external crates." },
        Crate { id: "goya-lang", layer: "scene", contract: "GY-LANG/1", status: "implemented", role: "Scene lexer, parser, and lowering. Lowered scenes are complete trees. Branches, loops, comparisons, and authored D0 are a separate language contract." },
        Crate { id: "goya-sdk", layer: "scene", contract: "SDK", status: "partial", role: "Facade for parse, lower, geometry, and the synchronous runtime. D1Application commits a validated state owner. The host still owns the window." },
        Crate { id: "goya-studio", layer: "scene", contract: "Studio", status: "partial", role: "CLI authoring plus a loopback browser preview on 127.0.0.1. Geometry edits rewrite position and extent in place. A full visual IDE is open." },
        Crate { id: "goya-tov", layer: "scene", contract: "Tov", status: "implemented", role: "Argument-safe adapter to the Tov compiler CLI. It does not invoke a shell." },
        Crate { id: "goya-ecosystem", layer: "scene", contract: "companions", status: "implemented", role: "Capability-gated adapters for RUGID, 3form, Mesut, and Theosis. Missing grants and tool failures are errors." },
        Crate { id: "goya-determinism", layer: "execute", contract: "GY-DETERMINISM/1", status: "implemented", role: "Classifies a process as D0 pure, D1 state-deterministic, D2 effect-deterministic, or N. N is never compiled. D0 with effects is an unresolved claim." },
        Crate { id: "goya-executable", layer: "execute", contract: "GY-EXECUTABLE/1", status: "implemented", role: "D0 reference VM, bounded D1 transition VM, and D2 journal VM. Hashes are SHA-256. The VMs do not touch the live state store or host I/O." },
        Crate { id: "goya-vee", layer: "execute", contract: "VEE", status: "implemented", role: "Builds a capability-empty Python worker after the Rust VM accepts the program, then POSTs it to /vee/submit and reads /vee/status." },
        Crate { id: "goya-hotswap", layer: "execute", contract: "GY-HOTSWAP/1", status: "implemented", role: "Ordered one-version state migration. The candidate activates only after stage_snapshot and commit both succeed." },
        Crate { id: "goya-change", layer: "efficiency", contract: "GY-CHANGE/1", status: "implemented", role: "Localised change-potential records used by rendering and accessibility updates." },
        Crate { id: "goya-rars", layer: "efficiency", contract: "GY-RARS/1", status: "implemented", role: "Resolution-adaptive resource scaling." },
        Crate { id: "goya-csr", layer: "efficiency", contract: "GY-CSR/1", status: "implemented", role: "Core-split rendering scheduler." },
        Crate { id: "goya-csr-cpu", layer: "efficiency", contract: "GY-CSR/1", status: "implemented", role: "CPU-side core split that consumes the scheduler, runtime, change records, and layout." },
        Crate { id: "goya-hcar", layer: "efficiency", contract: "GY-HCAR/1", status: "implemented", role: "Heterogeneous core classification for the scheduler." },
        Crate { id: "goya-fsc", layer: "efficiency", contract: "GY-FSC/1", status: "implemented", role: "Formatting-similarity compression over change records." },
        Crate { id: "goya-dgpm", layer: "efficiency", contract: "GY-DGPM/1", status: "implemented", role: "Memoises deterministic pure kernels. Identical input returns the cached result bit-exactly. The cache is an evictable LRU and reuse is cost-gated." },
        Crate { id: "goya-ddh", layer: "efficiency", contract: "GY-DDH/1", status: "implemented", role: "Deterministic dehumanisation of authored scene text before it reaches a host." },
        Crate { id: "goya-provenance", layer: "identity", contract: "GY-PROVENANCE/1", status: "partial", role: "Bounded redacted diagnostic lineage, plus C2PA signing and trust-aware verification. Signer identity and key custody remain release-owner decisions." },
        Crate { id: "goya-clipboard", layer: "identity", contract: "GY-CLIPBOARD/1", status: "implemented", role: "Capability-gated verbatim and deep clipboard. Deep copies carry provenance." },
        Crate { id: "goya-artifact", layer: "identity", contract: "GY-ARTIFACT/1", status: "implemented", role: "Byte, perceptual, and provenance identity for artifacts." },
        Crate { id: "goya-watermark", layer: "identity", contract: "GY-WATERMARK/1", status: "partial", role: "DCT marks. Robustness claims cover the measured corpus and codec settings only." },
        Crate { id: "goya-bench", layer: "identity", contract: "watermark bench", status: "partial", role: "Deterministic watermark benchmark over synthetic or caller-selected images, with optional ffmpeg round trips." },
        Crate { id: "goya-x402", layer: "economy", contract: "GY-X402/1", status: "implemented", role: "Payment challenge vocabulary." },
        Crate { id: "goya-economics", layer: "economy", contract: "GY-ECONOMICS/1", status: "implemented", role: "Allowances attached to payment challenges." },
        Crate { id: "goya-settlement", layer: "economy", contract: "GY-SETTLEMENT/1", status: "implemented", role: "Reconciliation of x402 challenges." },
        Crate { id: "goya-rpi", layer: "anticipate", contract: "GY-RPI/1", status: "implemented", role: "Probability field for anticipatory expression under a budget." },
        Crate { id: "goya-rpi2", layer: "anticipate", contract: "GY-RPI/2", status: "implemented", role: "Opt-in level-2 learned anticipation. Consent is explicit." },
        Crate { id: "goya-speculation", layer: "anticipate", contract: "GY-SPECULATION/1", status: "implemented", role: "Budgeted speculation. The runtime can consult a level-2 predictor." },
        Crate { id: "goya-bridge-physics", layer: "anticipate", contract: "GY-BRIDGE-PHYSICS/1", status: "implemented", role: "Retargetable spring integration for native state bridges." },
        Crate { id: "goya-bridge", layer: "anticipate", contract: "GY-BRIDGE/1", status: "implemented", role: "Spring state bridge. Theme colour comes from chroma." },
        Crate { id: "sash", layer: "compilers", contract: "Sash", status: "experimental", role: "Admits one static Win32 window and can emit App.exe. Runtime qualification and release admission stay BLOCKED. Linux cross-link is unqualified." },
        Crate { id: "neptune", layer: "compilers", contract: "Neptune", status: "experimental", role: "Swift source previews and a stdin JSON service. Apple compilation, simulator, and device admission stay BLOCKED." },
        Crate { id: "dockyard", layer: "compilers", contract: "dockyard.linux.http/0.0.1", status: "experimental", role: "Admits one GET route and emits a crt-static Linux executable plus an OCI image layout. The executable listens on 127.0.0.1. build also writes an SPDX 2.3 document and an unsigned provenance predicate beside the report; neither is in the image layer. verify leaves image start BLOCKED, rejects an evidence mismatch, rejects any file outside that closed set, serves the admitted GET for HTTP/1.0 and HTTP/1.1, rejects HEAD, HTTP/1.2, an absolute-form target, and every other request line it sends, and records a LISTEN socket on 127.0.0.1 for the admitted port and no IPv6 listener for that port. The response header block is exactly the status line, content-type, content-length, and connection close. On one connection verify writes the admitted GET twice and requires one response, then serves the next connection. An unterminated header or a line-feed-only header produces no response bytes. A header split across two writes returns one response. A lowercase method, a case-flipped path, and an empty query are rejected. Connection keep-alive still returns one response. HTTP/1.10 and a doubled space are rejected. Expect 100-continue does not receive 100 Continue. A lowercase version and a dot segment are rejected. A 2048-byte header returns one response. A tab after GET and a versionless request line are rejected. A header terminator after 2048 bytes produces no response bytes. A trailing space and HTTP/2.0 are rejected. A request that is only the header terminator returns 404. A leading space, HTTP/0.9, and a trailing slash are rejected. A fragment, a bare HTTP/1, and a doubled slash are rejected. A trailing dot, a percent-encoded suffix, and an asterisk form are rejected. A parent segment, an encoded slash, and a backslash are rejected. An encoded parent, an uppercase percent, and a semicolon are rejected. An encoded NUL, an encoded space, and an encoded question mark are rejected. The layer reader rejects a non-regular tar typeflag, a non-ustar magic, a link name, a prefix, a non-zero mtime, a device number, a name that is not the profile shape, an owner or mode other than 65532 and 0555, a version other than 00, a user or group name other than nonroot, a name field that is not NUL-terminated and zero-padded, a numeric field that is not zero-padded octal, a checksum field whose trailing byte is not a space, a non-zero header pad, a name other than srv/, srv/dockyard/, or srv/dockyard/service, an empty service payload, a repeated profile name, and a profile name out of order, skipped, or without the executable. start runs that layout with runc when runc is already installed, in a loopback-only network namespace, records that same loopback listener there, requires the device table to name only lo, rejects a foreign route, a gateway, or a default mask, rejects an IPv6 address that is not on lo, rejects an ARP neighbor, rejects a TCP peer that is not loopback, rejects a UDP, raw, UDP6, packet, UDP-Lite, ICMP, ICMPv6, raw6, UDP-Lite6, a flow label, an anycast address, a multicast route, a multicast vif, or a multicast filter, and reports cpu and memory as unset. On an unprivileged host image user 65532 is not applied and runtime qualification stays BLOCKED. Scenes, secrets, and other architectures are rejected." },
    ]
}

pub fn edges() -> &'static [(&'static str, &'static str)] {
    &[
        ("sash", "goya-lang"),
        ("goya-plugin-host", "goya-wayland"),
        ("goya-plugin-host", "goya-webview"),
        ("goya-document", "goya-wayland"),
        ("goya-sdk", "goya-lang"),
        ("goya-sdk", "goya-ir"),
        ("goya-sdk", "goya-layout"),
        ("goya-sdk", "goya-runtime"),
        ("goya-sdk", "goya-state"),
        ("goya-sdk", "goya-executable"),
        ("goya-sdk", "goya-vee"),
        ("goya-bridge", "goya-bridge-physics"),
        ("goya-bridge", "goya-chroma"),
        ("goya-dgpm", "goya-change"),
        ("goya-speculation", "goya-rpi"),
        ("goya-speculation", "goya-rpi2"),
        ("goya-speculation", "goya-ir"),
        ("goya-audio-linux", "goya-audio-dsp"),
        ("goya-wayland", "goya-runtime"),
        ("goya-wayland", "goya-layout"),
        ("goya-wayland", "goya-change"),
        ("goya-wayland", "goya-csr"),
        ("goya-wayland", "goya-hcar"),
        ("goya-wayland", "goya-window-management"),
        ("goya-csr-cpu", "goya-csr"),
        ("goya-csr-cpu", "goya-runtime"),
        ("goya-csr-cpu", "goya-change"),
        ("goya-csr-cpu", "goya-layout"),
        ("neptune", "goya-lang"),
        ("neptune", "goya-ir"),
        ("neptune", "goya-executable"),
        ("neptune", "goya-sdk"),
        ("goya-spatial-audio", "goya-layout"),
        ("goya-settlement", "goya-x402"),
        ("goya-executable", "goya-determinism"),
        ("goya-executable", "goya-state"),
        ("goya-executable", "goya-ir"),
        ("goya-fsc", "goya-change"),
        ("goya-clipboard", "goya-provenance"),
        ("goya-lang", "goya-chroma"),
        ("goya-lang", "goya-ir"),
        ("goya-lang", "goya-determinism"),
        ("goya-lang", "goya-executable"),
        ("goya-lang", "goya-vee"),
        ("goya-a11y", "goya-ddh"),
        ("goya-a11y", "goya-chroma"),
        ("goya-a11y", "goya-change"),
        ("goya-zcode", "goya-a11y"),
        ("goya-zcode", "goya-change"),
        ("goya-zcode", "goya-chroma"),
        ("goya-zcode", "goya-ddh"),
        ("goya-zcode", "goya-clipboard"),
        ("goya-zcode", "goya-provenance"),
        ("goya-zcode", "goya-lang"),
        ("goya-zcode", "goya-layout"),
        ("goya-zcode", "goya-runtime"),
        ("goya-zcode", "goya-wayland"),
        ("goya-zcode-native", "goya-a11y"),
        ("goya-zcode-native", "goya-change"),
        ("goya-zcode-native", "goya-chroma"),
        ("goya-zcode-native", "goya-ddh"),
        ("goya-zcode-native", "goya-clipboard"),
        ("goya-zcode-native", "goya-provenance"),
        ("goya-zcode-native", "goya-lang"),
        ("goya-zcode-native", "goya-layout"),
        ("goya-zcode-native", "goya-runtime"),
        ("goya-zcode-native", "goya-wayland"),
        ("goya-bench", "goya-watermark"),
        ("goya-bench", "goya-artifact"),
        ("goya-layout", "goya-ir"),
        ("goya-hotswap", "goya-executable"),
        ("goya-hotswap", "goya-determinism"),
        ("goya-hotswap", "goya-state"),
        ("goya-runtime", "goya-ir"),
        ("goya-runtime", "goya-state"),
        ("goya-runtime", "goya-layout"),
        ("goya-runtime", "goya-speculation"),
        ("goya-runtime", "goya-rpi2"),
        ("goya-hello", "goya-ir"),
        ("goya-hello", "goya-state"),
        ("goya-hello", "goya-layout"),
        ("goya-hello", "goya-runtime"),
        ("goya-hello", "goya-wayland"),
        ("goya-hello", "goya-sonic"),
        ("goya-hello", "goya-speculation"),
        ("goya-hello", "goya-rpi"),
        ("goya-hello", "goya-chroma"),
        ("goya-hello", "goya-bridge"),
        ("goya-hello", "goya-bridge-physics"),
        ("goya-hello", "goya-lang"),
        ("goya-hello", "goya-hcar"),
        ("goya-hello", "goya-csr"),
        ("goya-hello", "goya-rars"),
        ("goya-hello", "goya-change"),
        ("goya-hello", "goya-spatial-audio"),
        ("goya-hello", "goya-audio-dsp"),
        ("goya-hello", "goya-audio-linux"),
        ("goya-hello", "goya-ddh"),
        ("goya-ddh", "goya-lang"),
        ("goya-ddh", "goya-change"),
        ("goya-studio", "goya-sdk"),
        ("goya-studio", "goya-lang"),
        ("goya-ecosystem", "goya-lang"),
        ("goya-webview", "goya-wayland"),
        ("goya-webview", "goya-runtime"),
        ("goya-webview", "goya-layout"),
        ("goya-vee", "goya-determinism"),
        ("goya-vee", "goya-executable"),
        ("goya-vee", "goya-ir"),
        ("goya-csr", "goya-change"),
    ]
}

pub fn waves() -> &'static [Wave] {
    &[
        Wave { id: "wayland", wave: "1", name: "Live Wayland surface", status: "done", note: "GY-WAYLAND-001. Pointer, keyboard, resize, and shared-memory presentation are in the host." },
        Wave { id: "pipewire", wave: "2a", name: "PipeWire audio", status: "done", note: "goya-audio-linux plays through pw-cat. A host and device smoke is recorded." },
        Wave { id: "hotswap", wave: "2b", name: "Hot-swap and Tov", status: "done", note: "Interface reconciliation plus the argument-safe Tov CLI adapter." },
        Wave { id: "edc", wave: "2c", name: "EDC, D0, D1, D2", status: "done", note: "Reference VM, bounded D1, and journal-backed D2. Authored D0, branches, loops, and comparisons are a separate contract." },
        Wave { id: "vee", wave: "3a", name: "VEE workers", status: "done", note: "Loopback submit, status, and result. Landlock ran on this host on 2026-11-12. Production coverage still depends on the deployed kernel." },
        Wave { id: "companions", wave: "3b", name: "Companion adapters", status: "done", note: "RUGID, 3form, Mesut, and Theosis. Companion binaries are optional installations." },
        Wave { id: "c2pa", wave: "3c", name: "C2PA", status: "partial", note: "Official-tool signing and an ephemeral trust check passed. Signer identity, trust anchors, and key custody are open." },
        Wave { id: "hrtf", wave: "3d", name: "HRTF", status: "partial", note: "Bounded SOFA converter and a 64-direction fixture. More subjects remain caller-supplied." },
        Wave { id: "rpi2", wave: "3e", name: "RPI level 2", status: "done", note: "Opt-in learned anticipation with an explicit consent record." },
        Wave { id: "a11y", wave: "4", name: "AT-SPI bridge", status: "partial", note: "The bridge refreshes a live tree. Orca logged no focus target, so navigation and WCAG evidence remain open." },
        Wave { id: "studio", wave: "4", name: "SDK, Studio, packages", status: "partial", note: "Facade, loopback editor, geometry inspector, and signed offline packages. A full IDE and publisher custody are open." },
        Wave { id: "watermark", wave: "4", name: "Watermark corpus", status: "partial", note: "Synthetic and caller-supplied images, plus MJPEG, PNG, H.264, and MPEG-4 on the local ffmpeg. A larger licensed corpus is open." },
        Wave { id: "language", wave: "lang", name: "Authored control flow", status: "open", note: "Branches, loops, comparisons, and authored D0 stay outside GY-LANG/1." },
    ]
}

pub fn findings() -> &'static [Finding] {
    &[
        Finding { kind: "strength", title: "The frame path is revision-gated", body: "Runtime::handle is synchronous. A state commit swaps a staged map and bumps the revision only when the invariant accepts. Layout runs again only after that bump. A pointer that hits nothing, and a scroll that only changes runtime offsets, leave the published geometry alone." },
        Finding { kind: "strength", title: "Layout and the IR stay pure", body: "goya-ir, goya-state, goya-layout, and goya-chroma declare no third-party crates. Layout is a function of module and constraints, in millipoints. Chroma resolution is a function of theme and polarity. If a contrast target cannot be reached, the resolver returns ContrastUnreachable." },
        Finding { kind: "strength", title: "Effects stay outside the VM", body: "D0 is a pure tree. D1 writes only declared state. D2 replays an effect journal and has no live host path. Nondeterministic processes are refused before compilation. VEE workers are emitted after the Rust VM accepts the program, import only the needed stdlib modules, and request an empty capability list." },
        Finding { kind: "boundary", title: "The runtime's dependency note has drifted", body: "The README still says the core IR, state, layout, and runtime crates have no external dependencies. IR, state, layout, and chroma still match that sentence. goya-runtime also depends on unicode-segmentation, goya-speculation, and goya-rpi2, so a frame can consult opt-in level-2 prediction." },
        Finding { kind: "boundary", title: "Compilers stop at a named gate", body: "Sash can emit one Win32 static window and still reports runtime qualification as BLOCKED. Neptune can emit Swift previews and a D1 state owner and still reports Apple admission as BLOCKED. Dockyard verify still reports image start as BLOCKED. The executable listens on 127.0.0.1. dockyard start can run that layout under an already-installed runc and records a read-only root, dropped capabilities, a loopback-only network namespace, a device table that names only lo, no foreign route, gateway, or default mask, no IPv6 address off lo, no ARP neighbor, no TCP peer off loopback, and no UDP, raw, UDP6, packet, UDP-Lite, ICMP, ICMPv6, raw6, UDP-Lite6, a flow label, an anycast address, a multicast route, a multicast vif, or a multicast filter; on an unprivileged host it does not apply image user 65532, and runtime qualification stays BLOCKED. None of these paths is a second Goya runtime." },
        Finding { kind: "open", title: "Release gates that are still open", body: "Orca and WCAG conformance, a full Studio IDE, publisher key custody, HRTF subjects beyond the included fixture, a larger watermark corpus, authored control flow, and Landlock coverage on kernels other than the one smoked here. Experimental shells (native ZCode, webview, document, plugin host) document their own qualification limits." },
    ]
}

pub fn boundaries() -> &'static [Boundary] {
    &[
        Boundary { id: "wayland", name: "Wayland compositor", via: "goya-wayland", note: "wl_shm ARGB8888 pools, seat pointer and keyboard, and data-device offers." },
        Boundary { id: "pipewire", name: "PipeWire", via: "goya-audio-linux", note: "Device playback is handed to pw-cat. Spatial gain is computed before that handoff." },
        Boundary { id: "atspi", name: "AT-SPI", via: "goya-a11y", note: "A live accessibility tree can be published. Screen-reader navigation is not yet evidenced." },
        Boundary { id: "vee", name: "VEE service", via: "goya-vee", note: "POST /vee/submit and POST /vee/status. Bearer auth is used when the service requires the submit scope." },
        Boundary { id: "landlock", name: "Landlock", via: "goya-vee", note: "The supervisor sets no_new_privs, then landlock_restrict_self. EPERM falls back to rlimits." },
        Boundary { id: "python", name: "CPython worker", via: "goya-vee", note: "Generated source only. D0 imports json. D2 also imports hashlib. No file, network, or shell calls in the worker." },
        Boundary { id: "tov", name: "Tov CLI", via: "goya-tov", note: "Spawned with an argument vector." },
        Boundary { id: "companions", name: "Companion tools", via: "goya-ecosystem", note: "RUGID rdf_render, 3form-agent NDJSON, an in-process Mesut bridge, and the Theosis CLI. Each call needs its grant." },
        Boundary { id: "chromium", name: "Chromium child", via: "goya-webview", note: "Ephemeral profile and loopback DevTools. --no-sandbox is forbidden. The page is captured back into the Wayland surface." },
        Boundary { id: "foreign", name: "Foreign UI toolchains", via: "sash / neptune", note: "Win32 and Swift outputs are checked artifacts. They do not link the live frame runtime." },
        Boundary { id: "oci", name: "OCI image layout", via: "dockyard", note: "One static executable in an uncompressed layer. The executable listens on 127.0.0.1. No daemon is required to write the layout. An SPDX document and an unsigned provenance predicate sit beside the layout and are not in the layer. verify rejects a file outside that closed set. The layer reader rejects a non-regular tar typeflag, a non-ustar magic, a link name, and a prefix. dockyard start uses runc when it is already installed, records a loopback-only network namespace, requires that device table to name only lo, rejects a foreign route, a gateway, or a default mask, rejects an IPv6 address that is not on lo, rejects an ARP neighbor, rejects a TCP peer that is not loopback, and rejects a UDP, raw, UDP6, packet, UDP-Lite, ICMP, ICMPv6, raw6, UDP-Lite6, a flow label, an anycast address, a multicast route, a multicast vif, or a multicast filter. Image user 65532 is not qualified on an unprivileged host." },
    ]
}

pub fn paths() -> &'static [PathView] {
    &[
        PathView {
            id: "frame",
            title: "Live frame backend",
            summary: "A seat event becomes a transaction, then a frame, only when state actually changes.",
            units: &[
                Unit { name: "Seat and pump", crate_name: "goya-wayland", detail: "The host drains queued Wayland events and converts positions into millipoints." },
                Unit { name: "Dispatch", crate_name: "goya-runtime", detail: "handle is synchronous. Hit testing walks reverse draw order and separates button actions from input focus." },
                Unit { name: "Transaction", crate_name: "goya-state", detail: "One writer. Commit publishes a swapped map. Rejection leaves the previous revision in place." },
                Unit { name: "Revision gate", crate_name: "goya-runtime", detail: "Layout is skipped when the revision did not advance. Scroll offsets live on the runtime, not in the store." },
                Unit { name: "Layout", crate_name: "goya-layout", detail: "Pure function. Identical module and constraints produce an identical frame." },
                Unit { name: "Present", crate_name: "goya-wayland", detail: "Primitives are copied into an ARGB8888 shared-memory buffer." },
            ],
        },
        PathView {
            id: "audio",
            title: "Audio backend",
            summary: "Geometry becomes gain, then samples, then a PipeWire handoff.",
            units: &[
                Unit { name: "Policy", crate_name: "goya-sonic", detail: "Sonic events enter under GY-SONIC/1 before any samples are built." },
                Unit { name: "Space", crate_name: "goya-spatial-audio", detail: "Layout geometry becomes pan and gain. HRTF uses the included 64-direction fixture unless the caller supplies more." },
                Unit { name: "DSP", crate_name: "goya-audio-dsp", detail: "Mixer, limiter, and synthesis run in-process." },
                Unit { name: "Device", crate_name: "goya-audio-linux", detail: "The Linux adapter hands the buffer to pw-cat." },
            ],
        },
        PathView {
            id: "execute",
            title: "Checked execution backend",
            summary: "Authored behaviour is classified, run on a Rust VM, then mirrored by a capability-empty worker.",
            units: &[
                Unit { name: "Lower", crate_name: "goya-lang", detail: "A .gscene becomes a complete tree. Children of an input are linked by a synthetic column." },
                Unit { name: "Classify", crate_name: "goya-determinism", detail: "D0 forbids effects. D1 requires a state contract. D2 may list effects. N is refused." },
                Unit { name: "Reference VM", crate_name: "goya-executable", detail: "D0 builds a tree. D1 applies a bounded transition. D2 consumes a journal in order and rejects leftovers." },
                Unit { name: "Worker", crate_name: "goya-vee", detail: "Python is generated only after the Rust VM accepts the program. The request carries no capabilities." },
                Unit { name: "Sandbox", crate_name: "goya-vee", detail: "Submit goes to loopback VEE. Landlock is preferred. Stdout may carry a sandbox prefix before the JSON line." },
                Unit { name: "Activate", crate_name: "goya-hotswap", detail: "A migrated scene replaces the running one only after the state commit succeeds." },
            ],
        },
    ]
}

pub fn traces() -> &'static [Trace] {
    &[
        Trace {
            id: "pointer-button",
            label: "Pointer on a button",
            summary: "A hit action commits, the revision advances, and layout runs.",
            revision: "advanced",
            steps: &[
                Step { id: "seat", title: "Seat event", crate_name: "goya-wayland", state: "run", detail: "pump delivers the pointer. The host maps the surface position into millipoints." },
                Step { id: "hit", title: "Hit test", crate_name: "goya-runtime", state: "run", detail: "hit_test_target walks reverse draw order and returns the button ActionId." },
                Step { id: "txn", title: "Commit", crate_name: "goya-state", state: "run", detail: "begin takes the single writer. commit swaps the staged map and bumps the revision." },
                Step { id: "gate", title: "Revision gate", crate_name: "goya-runtime", state: "run", detail: "The revision changed, so the cached frame is stale." },
                Step { id: "layout", title: "Layout", crate_name: "goya-layout", state: "run", detail: "layout(module, constraints) rebuilds the millipoint frame." },
                Step { id: "present", title: "Present", crate_name: "goya-wayland", state: "run", detail: "render_list primitives are copied to the shared-memory buffer." },
            ],
        },
        Trace {
            id: "pointer-miss",
            label: "Pointer miss",
            summary: "Empty space does not open a transaction, so the published frame stays.",
            revision: "unchanged",
            steps: &[
                Step { id: "seat", title: "Seat event", crate_name: "goya-wayland", state: "run", detail: "The pointer position is mapped and dispatched." },
                Step { id: "hit", title: "Hit test", crate_name: "goya-runtime", state: "run", detail: "No button or input owns the point. The event is inert." },
                Step { id: "txn", title: "Transaction", crate_name: "goya-state", state: "skip", detail: "begin is not called. The committed revision stays." },
                Step { id: "layout", title: "Layout", crate_name: "goya-layout", state: "skip", detail: "The revision did not advance, so layout is not called." },
                Step { id: "present", title: "Frame", crate_name: "goya-runtime", state: "skip", detail: "The last render list remains the published frame." },
            ],
        },
        Trace {
            id: "key",
            label: "Text key",
            summary: "A grapheme edit commits through the same revision gate as a button.",
            revision: "advanced",
            steps: &[
                Step { id: "key", title: "Key event", crate_name: "goya-wayland", state: "run", detail: "The host delivers the evdev US subset. XKB and Compose stay available beside commit text." },
                Step { id: "text", title: "Text buffer", crate_name: "goya-runtime", state: "run", detail: "Edits stay on extended grapheme boundaries, with selection and bounded undo." },
                Step { id: "txn", title: "Commit", crate_name: "goya-state", state: "run", detail: "The input value is published and the revision bumps." },
                Step { id: "layout", title: "Layout", crate_name: "goya-layout", state: "run", detail: "The new text participates in the next pure layout." },
                Step { id: "present", title: "Present", crate_name: "goya-wayland", state: "run", detail: "The updated primitives are presented." },
            ],
        },
        Trace {
            id: "scroll",
            label: "Scroll",
            summary: "Offsets move on the runtime. Layout does not.",
            revision: "unchanged",
            steps: &[
                Step { id: "delta", title: "Native delta", crate_name: "goya-wayland", state: "run", detail: "The host converts device units before dispatch." },
                Step { id: "apply", title: "Scroll state", crate_name: "goya-runtime", state: "run", detail: "Event::Scroll adds content-space deltas on a registered node. Positive deltas move toward the right and bottom. Values clamp." },
                Step { id: "ignore", title: "Unknown node", crate_name: "goya-runtime", state: "skip", detail: "An unregistered node, or a non-finite delta, leaves offsets untouched." },
                Step { id: "layout", title: "Layout", crate_name: "goya-layout", state: "skip", detail: "Scroll does not commit store state, so the revision gate stays shut." },
            ],
        },
        Trace {
            id: "ime",
            label: "IME commit",
            summary: "Commit text arrives whole. Preedit is not a series of key characters.",
            revision: "advanced",
            steps: &[
                Step { id: "commit", title: "Commit text", crate_name: "goya-wayland", state: "run", detail: "PlatformEvent::CommitText delivers the finished string. The host inserts it at its own cursor." },
                Step { id: "preedit", title: "Composition", crate_name: "goya-runtime", state: "run", detail: "The composition region is independent of the selection. FinishComposition commits preedit. Focus loss cancels it." },
                Step { id: "atomic", title: "Atomic edit", crate_name: "goya-runtime", state: "run", detail: "CommitInputText writes the text and the caret together. Offsets are UTF-8 bytes expanded to whole graphemes." },
                Step { id: "layout", title: "Layout", crate_name: "goya-layout", state: "run", detail: "A successful commit advances the revision and relayouts." },
            ],
        },
        Trace {
            id: "swap",
            label: "Hot-swap",
            summary: "The candidate appears only after migration and commit both succeed.",
            revision: "advanced",
            steps: &[
                Step { id: "chain", title: "Migrate", crate_name: "goya-hotswap", state: "run", detail: "migrate_state clones the map and applies an ordered one-version chain. A gap or rename collision returns no partial map." },
                Step { id: "stage", title: "Stage", crate_name: "goya-state", state: "run", detail: "stage_snapshot replaces the staged map, so keys can be renamed or removed." },
                Step { id: "commit", title: "Commit", crate_name: "goya-state", state: "run", detail: "commit publishes that snapshot. Failure leaves the supervisor on the previous candidate." },
                Step { id: "activate", title: "Activate", crate_name: "goya-hotswap", state: "run", detail: "propose_migrated_swap activates the candidate only after commit returns Ok." },
                Step { id: "layout", title: "Layout", crate_name: "goya-layout", state: "run", detail: "The new revision forces a new frame." },
            ],
        },
        Trace {
            id: "d0",
            label: "D0 worker",
            summary: "A pure tree is checked in Rust, then repeated by a Python worker with no capabilities.",
            revision: "not-applicable",
            steps: &[
                Step { id: "lower", title: "Lower", crate_name: "goya-lang", state: "run", detail: "The scene lowers to a complete tree, including an input whose children sit in a synthetic column." },
                Step { id: "class", title: "Classify", crate_name: "goya-determinism", state: "run", detail: "D0 requires an empty effect list. Any declared effect is RejectReason::UnresolvedClaim." },
                Step { id: "vm", title: "Reference VM", crate_name: "goya-executable", state: "run", detail: "Bytecode version, hash, and structure are checked. Spacer values keep their IEEE-754 bit patterns." },
                Step { id: "worker", title: "Emit worker", crate_name: "goya-vee", state: "run", detail: "The worker imports json only and prints the same compact document as the Rust serializer." },
                Step { id: "submit", title: "Submit", crate_name: "goya-vee", state: "run", detail: "POST /vee/submit carries an empty capability list. Callers parse the JSON line, because stdout may include a sandbox prefix." },
            ],
        },
        Trace {
            id: "d1",
            label: "D1 transition",
            summary: "A bounded state transition is validated twice: once in Rust, once in the worker.",
            revision: "not-applicable",
            steps: &[
                Step { id: "admit", title: "Admit", crate_name: "goya-lang", state: "run", detail: "Authored source covers the bounded D1 subset. Control flow is not part of this lowering." },
                Step { id: "class", title: "Classify", crate_name: "goya-determinism", state: "run", detail: "A D1 claim without a state contract is MissingStateContract." },
                Step { id: "vm", title: "D1 VM", crate_name: "goya-executable", state: "run", detail: "Constants, state reads, input reads, checked addition, and state writes. Snapshot keys must match the contract." },
                Step { id: "worker", title: "Worker", crate_name: "goya-vee", state: "run", detail: "Rust executes first. The worker repeats the checks and emits the same program, input, and outcome hashes." },
                Step { id: "owner", title: "Host owner", crate_name: "goya-sdk", state: "run", detail: "D1Application commits only when the action and the full resulting state validate. Failure keeps the previous revision." },
            ],
        },
        Trace {
            id: "d2",
            label: "D2 journal",
            summary: "Declared effects replay from a journal. The worker never performs them.",
            revision: "not-applicable",
            steps: &[
                Step { id: "class", title: "Classify", crate_name: "goya-determinism", state: "run", detail: "D2 is effect-deterministic. An effect that was not declared is UndisclosedEffect." },
                Step { id: "journal", title: "Journal match", crate_name: "goya-executable", state: "run", detail: "Each request consumes the next entry with the same capability and request bytes. Leftover entries reject the run." },
                Step { id: "types", title: "Typed results", crate_name: "goya-executable", state: "run", detail: "Integers are a tag plus eight little-endian bytes. Text is a tag, a length, and UTF-8. Booleans are a tag plus 0 or 1." },
                Step { id: "worker", title: "Worker", crate_name: "goya-vee", state: "run", detail: "The worker imports hashlib and json, rechecks the SHA-256 chain, and requests no capabilities. Bounds are 256 journal entries and 64 KiB." },
                Step { id: "submit", title: "Submit", crate_name: "goya-vee", state: "run", detail: "VeeClient must be asked to submit. The request does not open a live effect path." },
            ],
        },
        Trace {
            id: "classify-refuse",
            label: "Classifier refusal",
            summary: "N never becomes bytecode, and a bad claim never becomes a worker.",
            revision: "rejected",
            steps: &[
                Step { id: "claim", title: "Claim", crate_name: "goya-determinism", state: "run", detail: "The declaration names a class, an effect list, and whether a state contract is present." },
                Step { id: "n", title: "Refuse N", crate_name: "goya-determinism", state: "reject", detail: "DeterminismClass::Nondeterministic returns RejectReason::Nondeterministic. The process is not compiled." },
                Step { id: "d0", title: "Refuse impure D0", crate_name: "goya-determinism", state: "reject", detail: "A pure claim that lists any effect is UnresolvedClaim." },
                Step { id: "stop", title: "Stop", crate_name: "goya-vee", state: "skip", detail: "No worker is generated and nothing is submitted." },
            ],
        },
    ]
}

pub fn workspace_version() -> &'static str {
    "0.1.8"
}

pub fn version_file() -> &'static str {
    "10.27.2"
}

pub fn dangling_edges() -> Vec<(&'static str, &'static str)> {
    let known = |id: &str| crates().iter().any(|c| c.id == id);
    edges()
        .iter()
        .copied()
        .filter(|(from, to)| !known(from) || !known(to))
        .collect()
}

pub fn model_json() -> String {
    let mut out = String::from("{");
    field(&mut out, "workspace_version", &json_str(workspace_version()), true);
    field(&mut out, "version_file", &json_str(version_file()), false);
    field(&mut out, "platform", &json_str("Linux/Wayland first"), false);
    field(&mut out, "source", &json_str("docs/contracts.md, README.md, and workspace manifests"), false);
    field(&mut out, "layers", &list(layers(), layer_json), false);
    field(&mut out, "crates", &list(crates(), crate_json), false);
    field(&mut out, "edges", &edges_json(), false);
    field(&mut out, "waves", &list(waves(), wave_json), false);
    field(&mut out, "findings", &list(findings(), finding_json), false);
    field(&mut out, "boundaries", &list(boundaries(), boundary_json), false);
    field(&mut out, "paths", &list(paths(), path_json), false);
    out.push('}');
    out
}

pub fn trace_catalog_json() -> String {
    let mut out = String::from("{\"events\":");
    out.push('[');
    for (i, trace) in traces().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        field(&mut out, "id", &json_str(trace.id), true);
        field(&mut out, "label", &json_str(trace.label), false);
        out.push('}');
    }
    out.push_str("]}");
    out
}

pub fn trace_json(id: &str) -> Option<String> {
    traces().iter().find(|trace| trace.id == id).map(one_trace)
}

fn one_trace(trace: &Trace) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(trace.id), true);
    field(&mut out, "label", &json_str(trace.label), false);
    field(&mut out, "summary", &json_str(trace.summary), false);
    field(&mut out, "revision", &json_str(trace.revision), false);
    field(&mut out, "steps", &list(trace.steps, step_json), false);
    out.push('}');
    out
}

fn layer_json(layer: &Layer) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(layer.id), true);
    field(&mut out, "title", &json_str(layer.title), false);
    field(&mut out, "summary", &json_str(layer.summary), false);
    out.push('}');
    out
}

fn crate_json(item: &Crate) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(item.id), true);
    field(&mut out, "layer", &json_str(item.layer), false);
    field(&mut out, "contract", &json_str(item.contract), false);
    field(&mut out, "status", &json_str(item.status), false);
    field(&mut out, "role", &json_str(item.role), false);
    out.push('}');
    out
}

fn wave_json(item: &Wave) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(item.id), true);
    field(&mut out, "wave", &json_str(item.wave), false);
    field(&mut out, "name", &json_str(item.name), false);
    field(&mut out, "status", &json_str(item.status), false);
    field(&mut out, "note", &json_str(item.note), false);
    out.push('}');
    out
}

fn finding_json(item: &Finding) -> String {
    let mut out = String::from("{");
    field(&mut out, "kind", &json_str(item.kind), true);
    field(&mut out, "title", &json_str(item.title), false);
    field(&mut out, "body", &json_str(item.body), false);
    out.push('}');
    out
}

fn boundary_json(item: &Boundary) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(item.id), true);
    field(&mut out, "name", &json_str(item.name), false);
    field(&mut out, "via", &json_str(item.via), false);
    field(&mut out, "note", &json_str(item.note), false);
    out.push('}');
    out
}

fn path_json(item: &PathView) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(item.id), true);
    field(&mut out, "title", &json_str(item.title), false);
    field(&mut out, "summary", &json_str(item.summary), false);
    field(&mut out, "units", &list(item.units, unit_json), false);
    out.push('}');
    out
}

fn unit_json(item: &Unit) -> String {
    let mut out = String::from("{");
    field(&mut out, "name", &json_str(item.name), true);
    field(&mut out, "crate", &json_str(item.crate_name), false);
    field(&mut out, "detail", &json_str(item.detail), false);
    out.push('}');
    out
}

fn step_json(item: &Step) -> String {
    let mut out = String::from("{");
    field(&mut out, "id", &json_str(item.id), true);
    field(&mut out, "title", &json_str(item.title), false);
    field(&mut out, "crate", &json_str(item.crate_name), false);
    field(&mut out, "detail", &json_str(item.detail), false);
    field(&mut out, "state", &json_str(item.state), false);
    out.push('}');
    out
}

fn edges_json() -> String {
    let mut out = String::from("[");
    for (i, (from, to)) in edges().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        field(&mut out, "from", &json_str(from), true);
        field(&mut out, "to", &json_str(to), false);
        out.push('}');
    }
    out.push(']');
    out
}

fn list<T>(items: &[T], encode: fn(&T) -> String) -> String {
    let mut out = String::from("[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&encode(item));
    }
    out.push(']');
    out
}

fn field(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    out.push_str(&json_str(key));
    out.push(':');
    out.push_str(value);
}

fn json_str(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_edge_names_a_crate() {
        assert!(dangling_edges().is_empty(), "{:?}", dangling_edges());
    }

    #[test]
    fn crate_ids_are_unique() {
        let mut ids: Vec<_> = crates().iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len());
        assert_eq!(before, 51);
    }

    #[test]
    fn layers_cover_every_crate() {
        for item in crates() {
            assert!(layers().iter().any(|layer| layer.id == item.layer), "{}", item.id);
        }
    }

    #[test]
    fn scroll_trace_skips_layout() {
        let json = trace_json("scroll").unwrap();
        assert!(json.contains("\"state\":\"skip\""));
        assert!(json.contains("goya-layout"));
        assert!(json.contains("\"revision\":\"unchanged\""));
    }

    #[test]
    fn model_mentions_the_runtime_gate() {
        let json = model_json();
        assert!(json.contains("goya-runtime"));
        assert!(json.contains("GREY_LINE"));
        assert!(json.contains("10.27.2"));
    }
}
