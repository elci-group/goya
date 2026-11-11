# Goya Webview: owned Chromium viewport

Status: first native browser-engine slice; implementation and security gates are in progress. This document defines a bounded renderer integration, not a claim of full browser, plugin UI, document, CUA, or accessibility parity.

## Goal and boundary

Provide a real page rendered by an app-owned, sandboxed Chromium process and composited into a Goya Wayland surface. The native Goya window owns browser process lifecycle, the CDP connection, target page and ephemeral profile. The web page is content: it never receives the DevTools connection or Goya host APIs.

The browser backend is a separate `goya-webview` crate. It returns an owned top-down RGBA8 frame and accepts explicit viewport input (navigation, mouse, key and committed text); it does not own the Goya window, application/session state, or transcript. The Wayland host remains the only presentation owner and decides when a captured frame is copied to a released shared-memory slot. A bounded worker/actor owns blocking CDP calls so it cannot stall the window event pump.

This slice uses Chromium's maintained DevTools Protocol over a loopback WebSocket. The controller launches a configured executable with argument vectors (never a shell), a fresh private profile directory, `--headless`, and `--remote-debugging-port=0`. It reads `DevToolsActivePort` from that profile and accepts only a loopback WebSocket endpoint. It does not connect to a user's existing browser profile or any pre-existing DevTools port. Chromium sandboxing stays enabled; `--no-sandbox` is forbidden. Chrome 136 and later require a non-default `--user-data-dir` for remote debugging, which this isolated profile provides ([Chrome security change](https://developer.chrome.com/blog/remote-debugging-port?hl=en)).

The screenshot and input methods are defined by the [CDP Page](https://chromedevtools.github.io/devtools-protocol/tot/Page/), [Input](https://chromedevtools.github.io/devtools-protocol/tot/Input/), [Target](https://chromedevtools.github.io/devtools-protocol/tot/Target/), and [Accessibility](https://chromedevtools.github.io/devtools-protocol/tot/Accessibility/) domains. **Compatibility risk:** the official CDP project says direct CDP use by third-party products is unsupported and makes no backward-compatibility guarantee; it recommends supported wrappers such as Puppeteer, ChromeDriver, or Chrome DevTools for agents ([protocol scope](https://chromedevtools.github.io/devtools-protocol/)). The current Rust controller is therefore a constrained engineering spike, not an approved stable engine API. Before production, the platform owner must either approve this risk with a Chromium version/update policy and compatibility evidence or replace the transport with a supported maintained wrapper. The controller uses only a small method set, fails closed on malformed responses, and retains real-browser smoke evidence for the configured Chromium version.

## State ownership and lifecycle

| State                                                         | Sole owner                              | Lifetime / persistence                                                                                                                                     |
| ------------------------------------------------------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chromium child process, CDP socket and request IDs            | `BrowserSession` in `goya-webview`      | One session. `close` and `Drop` terminate and reap the exact child; no child is detached.                                                                  |
| Browser user-data directory                                   | `BrowserSession` private `TempDir`      | Unique per session, private permissions, removed after child exit. This first slice does not persist cookies or site data across app restarts.             |
| Page/target and viewport dimensions                           | `BrowserSession`                        | Current browser session only. Dimensions are bounded and changed through CDP before capture.                                                               |
| Decoded screenshot                                            | `RgbaFrame` returned by the engine      | Owned frame; max width/height and byte count enforced before allocation and before surface copy. No shared mutable frame buffer.                           |
| Window, focus, clipboard, shell events and frame presentation | Native Goya host / `goya-wayland`       | Existing host lifecycle. Browser input is routed only while its viewport owns focus; close/navigation/tab state is never inferred from a stale screenshot. |
| Page origin and web storage                                   | Chromium's isolated per-session profile | Different sessions receive distinct profiles. This does not yet implement Drowzy's persistent plugin-origin partitions or trusted plugin identity.         |

The controller sends one bounded CDP command at a time and correlates each response by numeric request ID. Unsolicited events are bounded/ignored unless required for navigation/capture. Every wait has a deadline and checks child exit. On protocol, timeout, malformed frame, oversize payload, or child failure, mark the viewport unavailable; do not retry a command with side effects automatically. Closing kills and waits for the process before the profile directory is removed.

## Protocol and security rules

- Accept only `http:` and `https:` navigation in this first slice. Reject `javascript:`, `data:`, `file:`, unknown schemes, malformed URLs and unsupported credential-bearing URLs. Local smoke tests use an HTTP server bound to `127.0.0.1`.
- Spawn the configured browser without a shell. The private profile is created with owner-only permissions where supported. Never pass the real Chrome profile path, remote-debugging origin wildcards, `--no-sandbox`, or arbitrary extra arguments from the renderer.
- Choose an ephemeral debugging port and derive its websocket path from the child-owned profile's `DevToolsActivePort`. Require loopback host and the expected Chrome path before opening the WebSocket. Do not publish the port/path to web content or application logs.
- Set explicit WebSocket message/frame/write limits before reading protocol responses. Limit JSON response bytes, base64 PNG bytes, decoded width, decoded height, pixel count, and RGBA output bytes. Check dimensions and arithmetic overflow before allocating output buffers. Reject unsupported PNG bit depth/color combinations or apply a documented, bounded transform.
- Initial bounds: maximum viewport 2048×2048; maximum decoded pixels 4,194,304; maximum RGBA frame 16 MiB; maximum PNG/CDP message 24 MiB; protocol request deadline 5 seconds and process startup deadline 10 seconds. All limits are constants and tests cover boundary and overflow values.
- The host controls which URL is opened and which input events are dispatched. No generic JavaScript evaluation API is exposed. Browser permission prompts, popup/download policy, file chooser, persistent profiles, and arbitrary plugin bridge are outside this initial public API and remain blocked parity rows. Direct CDP's official unsupported status is a production release gate, not a hidden implementation detail.
- Chromium's process sandbox is mandatory for production. A smoke test must prove the launched process does not contain `--no-sandbox`, and that the configured binary starts successfully under the supported user environment; checking arguments alone is not evidence that the OS sandbox is active.

## Public crate contract

### Native browser chrome and composition contract

`GoyaWindow::present_composited` draws native `Primitive` chrome first and then
an optional tightly packed RGBA viewport layer in the same SHM frame. The
viewport is clipped against both the window and its explicit clip rectangle;
negative/offscreen origins are valid, but source dimensions, stride and byte
length are validated before any destination write. Straight alpha blends over
the already rendered native background. Browser input coordinates remain
window-local and are translated by subtracting the viewport origin; toolbar
events remain native chrome events. A capture is published only when both
resize and navigation generations still match.

The standalone browser shows an address field plus Back, Forward and Reload
controls. Enter commits only a validated HTTP(S) URL. The browser viewport
starts below the toolbar; pointer positions are offset into that viewport and
toolbar presses do not reach page content. Acceptance covers clipping,
alpha/negative bounds, toolbar input mapping, stale resize capture rejection,
and a real 3-second native run.

`BrowserSession` provides:

- `launch(config)`: validate bounds and executable configuration, create the private profile, start Chromium, discover its loopback CDP endpoint, create/attach one page target, and set the initial viewport.
- `navigate(url)`: accept a validated HTTP(S) URL, navigate the owned target, and wait only until a bounded load/first-frame condition or timeout.
- `go_back` / `go_forward` / `reload`: navigate only the owned target's bounded Chromium history; no profile or cross-tab history is exposed.
- `set_viewport(width, height)`: clamp/reject out-of-policy sizes and update device metrics.
- `capture_frame() -> RgbaFrame`: issue `Page.captureScreenshot`, decode its PNG with explicit output caps, and return top-down RGBA8 bytes plus dimensions and stride.
- `dispatch_pointer` / `dispatch_key` / `insert_text`: translate host-provided viewport-local coordinates and key/text input to the small supported CDP Input calls. Do not forward events when the host says the viewport lacks focus.
- `selected_text`: return a bounded selection value only for an explicit host copy action; this is not an implicit clipboard integration.
- `accessibility_snapshot`: return a bounded semantic projection of the owned page's accessibility tree for a native AT adapter. It is a qualification interface, not proof of AT-SPI parity.
- `close()`: idempotently close target/socket, kill/wait child, and release private profile. `Drop` is the final cleanup backstop.

The browser backend does not call `goya-wayland` directly. A separate host adapter copies its `RgbaFrame` into the public bounded pixel-surface API from `goya-wayland`; screenshot and window resize races are resolved by a generation token so frames from an old navigation or size cannot overwrite the current surface. The adapter must avoid blocking the Wayland event loop on CDP.

## Goya first-slice responsibilities

The first integrated window is a dedicated web/document viewport in the current native Goya shell, not an external browser. It must support navigation to an allowed HTTP(S) URL, image frame presentation, pointer click/scroll, key input and committed text, visible URL/title and explicit close. The surface is experimental until accessible browser controls and page content are exposed to Orca; pixels alone are not accessibility parity. Do not route AT-SPI actions through arbitrary JS.

The platform seam should remain engine-neutral: `BrowserSession` and `RgbaFrame` are the only engine-specific types visible to its host. The existing Goya app-server protocol is independent of web rendering. Do not add a second Agent or Local Host to operate the browser. In standalone Goya mode the native app owns the local app-server; attaching to an existing Drowzy Host is a separate topology and is not implemented by this crate.

## Explicit non-goals and blocked claims

- This slice does not establish plugin UI hosting. MCP plugin pages are untrusted documents and require a distinct origin/profile or browser context per trusted plugin identity, strict CSP and permission policy, a scoped host capability broker, account revocation, storage migration, lifecycle teardown, download/clipboard rules, and accessible-tree projection. A shared browser profile or generic CDP bridge is not an acceptable substitute.
- This slice does not provide a full browser chrome, tab/session persistence, site permissions, downloads, file chooser, history, bookmark, CUA screenshot action, cross-window sharing, browser permission settings, or phone UI.
- This slice does not implement an embedded GTK/WebKitGTK view. The current Goya window is a custom direct Wayland `wl_surface` rendered into ARGB8888 shared memory; a GTK widget cannot be placed into that render list by adding a Cargo dependency. WebKitGTK 4.1 is present on the current development host, but is not a portable engine/runtime dependency established by the Goya repository.
- This slice does not implement PDFs or native document parsing; the separate Goya document foundation owns those file/rendering paths.
- This slice does not claim parity for Windows or macOS; the prepared Goya product slice currently targets Linux/Wayland.

## Acceptance evidence

| Case                     | Setup / action                                                                                                                                                                                                                                           | Required evidence                                                                                                                                                            |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GYP-WEBVIEW-UNIT-001`   | Validate URL policy, port parsing, CDP response correlation, request deadlines and launch argument construction.                                                                                                                                         | Deterministic tests; no environment-dependent browser timing.                                                                                                                |
| `GYP-WEBVIEW-UNIT-002`   | Decode valid PNGs at normal and boundary sizes; reject wrong color/depth, oversized encoded responses, dimensions, pixel count, truncated PNG and arithmetic overflow.                                                                                   | Deterministic test corpus with exact error outcomes and no allocation above cap.                                                                                             |
| `GYP-WEBVIEW-SMOKE-001`  | Launch installed/pinned Chrome with its sandbox enabled, isolated temp profile and loopback-only CDP; navigate to `apps/webview-fixture.html` served from loopback; capture and validate the expected dimensions and changed pixels after pointer input. | Real binary version, child PID, argv with no `--no-sandbox`, renderer seccomp state on Linux, endpoint host, frame dimensions/hash, interaction result, cleanup/reap result. |
| `GYP-WEBVIEW-COMPAT-001` | Upgrade the managed browser version at least one step and exercise the exact command subset, PNG capture, IME, process cleanup and sandbox checks on each supported platform.                                                                            | Versioned pass/fail evidence and a release decision for the official CDP compatibility risk; an installed system browser version alone is not a deployment policy.           |
| `GYP-WEBVIEW-SMOKE-002`  | Force browser exit, CDP timeout, navigation failure and close while capture is pending.                                                                                                                                                                  | Host gets bounded error; child is reaped, no stale screenshot is published, profile is removed.                                                                              |
| `GYP-WEBVIEW-HOST-001`   | Resize Goya viewport during navigation/capture; dispatch click/scroll/key/text, unfocus and refocus; close viewport.                                                                                                                                     | Pixel frame matches current dimensions/generation; no event-loop stall; no input leaks to composer while browser owns focus or vice versa.                                   |
| `GYP-WEBVIEW-A11Y-001`   | Use Orca to reach browser chrome and representative page controls/content, navigate focus both directions, and inspect roles/names/state after reload.                                                                                                   | Native AT-SPI evidence. Until this passes, browser feature is marked experimental and a11y parity remains blocked.                                                           |
| `GYP-PLUGIN-HOST-001`    | Load two untrusted plugin identities with identical web storage keys; attempt cross-origin access, host bridge access, clipboard, permission and account reuse.                                                                                          | Separate identity partitions and explicit denied/approved capabilities, no data/authority crossover, revocation proof, AT-SPI projection. Not part of the current smoke.     |

## Sources consulted

- Goya surface and ARGB8888 shm renderer: `crates/goya-wayland/src/lib.rs` (`BufferPair`, `ShmFrame`, `GoyaWindow::present`).
- Goya render primitives: `crates/goya-runtime/src/lib.rs` (`Primitive` currently has `Panel`, `Text`, `Button`).
- Native pointer/key/IME/clipboard events: `crates/goya-wayland/src/lib.rs` (`PlatformEvent`).
- Existing Goya GTK accessibility adapter: `crates/goya-a11y/Cargo.toml`, `crates/goya-a11y/src/gtk_atspi.rs` (optional GTK 3 bridge, not wired into the Wayland monitor).
- Official engine control schema: [Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/), [Page](https://chromedevtools.github.io/devtools-protocol/tot/Page/), [Input](https://chromedevtools.github.io/devtools-protocol/tot/Input/), [Target](https://chromedevtools.github.io/devtools-protocol/tot/Target/), [Accessibility](https://chromedevtools.github.io/devtools-protocol/tot/Accessibility/).
- Official Chrome profile/debugging security requirement: [Changes to remote debugging switches](https://developer.chrome.com/blog/remote-debugging-port?hl=en).
