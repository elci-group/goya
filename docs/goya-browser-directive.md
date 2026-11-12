# Technical Directive: Goya Browser

**Status:** implementation directive for an experimental, single-window browser frontend.

## Objective

Make Goya a capable native browser shell around the existing `goya-webview` Chromium viewport. Goya owns the window, browser chrome, focus, lifecycle and presentation. Chromium renders untrusted web content in an isolated process. The browser must feel responsive and clear while keeping the native event loop independent from renderer work.

“State of the art” is a product target, not a parity claim. Ship capabilities only when their security, accessibility and platform evidence exists.

## Architecture and invariants

- Keep the engine seam small: the host talks to `BrowserSession` and receives owned `RgbaFrame` values. CDP details stay inside `goya-webview`.
- Launch Chromium as an owned child with a fresh private profile, sandbox enabled, loopback-only ephemeral DevTools endpoint and argument-vector process creation. Never attach to a user's profile or a pre-existing debugging port.
- Keep blocking browser protocol work on a bounded worker. Bound command queues, deadlines, frame dimensions, decoded pixel count and message sizes. The Wayland event loop must remain responsive when Chromium stalls or exits.
- Keep native chrome and page pixels composited by the Goya host. Toolbar actions never leak into page input; page input is sent only while the viewport owns focus.
- Reject stale frames after resize or navigation by generation. Validate all dimensions and byte arithmetic before allocation or surface writes.
- Allow only validated credential-free HTTP(S) navigation in the initial product. No generic page JavaScript evaluation API is exposed.
- Make browser controls keyboard reachable, visibly focused, labeled and operable through the native accessibility tree. Page pixels alone are not accessibility support.

## Initial product scope

Deliver one native Goya browser window with Back, Forward, Reload, a focused address/search field, a Chromium content viewport, visible loading/failure feedback, resize handling, keyboard and pointer input, and orderly close/reap behavior. Use a local first-run page to explain the browser and exercise input. Be explicit that this build has one ephemeral session.

Do not show tab, bookmark, history, download, permission, profile-sync or plugin controls until those capabilities exist end to end. The initial first-run page may provide clickable local navigation examples, but must not imply persistent saved destinations.

## Delivery sequence

1. **Shell and local start page:** polished native toolbar and responsive, accessible local landing page; navigation and input work against the existing viewport.
2. **Reliability:** explicit loading/error states, navigation lifecycle, resize stress evidence, child cleanup evidence, and keyboard shortcut coverage.
3. **Accessibility:** semantic native controls plus a documented page-tree adapter; test with Orca/AT-SPI on supported Linux hosts before claiming support.
4. **Persistent browsing:** separately designed profile lifecycle, site-data controls, clear-data flow, and recovery behavior.
5. **Advanced browser capabilities:** tabs, downloads, permissions, extensions and cross-platform packaging each require isolated designs, threat review and acceptance evidence. They are not implicit follow-ons to the viewport.

## Release gates

- Existing security bounds and HTTP(S) URL validation remain enforced, including credential-bearing URL rejection.
- Chromium sandbox is enabled in the actual supported launch environment; absence of `--no-sandbox` alone is not proof.
- Browser subprocess, profile and protocol socket are cleaned up after close and failure.
- Toolbar focus/input routing and stale-frame rejection are demonstrated at the viewport edges and during resize/navigation races.
- Keyboard-only navigation, visible focus, accessible names and real screen-reader interaction are verified before accessibility claims.
- Chromium/CDP compatibility is treated as an explicit risk: direct CDP is not a supported stable third-party API. Set a Chromium update policy and compatibility evidence or replace the transport with a maintained supported wrapper before production.

## Current execution boundary

This directive builds on the current `goya-webview` slice. The local first-run fixture and native shell are the initial execution target. The browser remains experimental until the release gates above are evidenced; a successful local render does not establish full browser parity, production sandbox coverage, cross-platform support or accessibility conformance.
