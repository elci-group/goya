# electron-counter — DEL-B01 reference fixture

This is the first Electron reference application for the Disparates programme
(GY-ENT-001, Phase 1). Its extractor is `goya-disparate-electron`, which
DEL-TD-001 calls Delectable. It was written in this repository for this
purpose, so it carries the workspace licence (MIT OR Apache-2.0) and has no
third-party code. It meets the GY-ENT-001 corpus criteria: one window, no
native modules, a fixed set of two IPC channels and no dynamic `require()`.

Its Goya counterpart already exists:
[`crates/sash/fixtures/interactive-live.gscene`](../../../crates/sash/fixtures/interactive-live.gscene).
The pair is the narrowest end-to-end case. It is the only §27 tier that
strict `.gscene` can express today (DELECTABLE-0 §3).

## What it does

The window is 320×160. It shows the caption "Count", an amount field that
starts at `1`, an "Add" button and a total that starts at `0`. Pressing Add
adds the amount to the total. The amount must be a canonical decimal i64
(`0`, `42` or `-7`, but not `01`, `-0`, ` 1` or `1.0`). Anything else, or a
sum outside the i64 range, leaves the total unchanged. Pasting into the
amount field is blocked, and copying from it is allowed. The total lives in
the main process for the session only.

It uses the usual Electron security settings: `contextIsolation`, `sandbox`,
no `nodeIntegration`, a preload bridge and a restrictive CSP. The renderer
cannot add on its own; it asks the main process over IPC.

## Expected extraction (the oracle for `goya-disparate-electron`)

A static extractor must report at least the following, with file and line
provenance. The GY-FAIR/2 kinds are named.

| Construct | Where | FAIR kind | Expected Goya reading |
| --- | --- | --- | --- |
| `BrowserWindow` 320×160, title "Count" | `src/main.js` | `window_lifecycle` | Scene viewport, 320×160 DIP, as Sash links it |
| `whenReady` / `window-all-closed` → `quit` | `src/main.js` | `window_lifecycle` | Target lifecycle; no `.gscene` construct (no Adios) |
| `exposeInMainWorld('counter', {add, get})` | `src/preload.js` | `preload_bridge` | Incidental; disappears |
| `counter:add` (`handle` / `invoke`) | `src/main.js`, `src/preload.js` | `ipc_channel` | Incidental; becomes the D1 interaction `action.add` |
| `counter:get` | `src/main.js`, `src/preload.js` | `ipc_channel` | Incidental; becomes `text-from: state.count` |
| `window.counter.get()` at start-up | `src/renderer.js` | `bridge_call` | The initial display of `state.count` |
| `count` (BigInt, in memory) | `src/main.js` | `state_store` | `@state { count: int = 0 session; }` |
| `parseCanonicalI64` and the range checks | `src/main.js` | `logic_unit` | D3 `int` conversion plus D1 checked `AddInt` |
| `click` on `#window-add` | `src/renderer.js` | `event_handler` | `trigger: button.add.activate` |
| `paste` → `preventDefault` on `#window-amount` | `src/renderer.js` | `event_handler` | `gestures { copy: allow; paste: deny; }` |
| The promise chain on presses | `src/renderer.js` | `state_store` `pending` (reassigned in the click handler; the click handler lists it under `mutates`) | GSSH/2 serialised activations. Recognising the chain as serialisation is later work. |
| `input`, `button`, `output`, caption | `src/index.html` | `component` | `@input amount`, `@button add`, `@cell total`, root `text` |
| `aria-label`, `aria-description` | `src/index.html` | `component` (attributes) | `@iden { label; hint; }` |
| `aria-live="polite"` on the total | `src/index.html` | `component` (attribute) | D9 announcement (wording differs; see below) |
| Five rules, four of them percentage boxes | `src/style.css` | `style_rule` | `position_p` / `extent_p`, the same numbers |

Element IDs are `window-amount` and so on, because HTML IDs cannot contain
the `.` used in Goya's `window.amount`. That mapping is mechanical.

## Known differences in the pair

These belong in an equivalence report, not in a silent fix:

* **Announcements.** Electron's live region reads the new number ("1").
  Goya's D9 reference says "count increased by 1, from 0 to 1".
* **Refused input.** Electron shows nothing when an amount is refused. Goya
  targets report a refused activation separately (GSSH/2 §Execution).
* **Caption placement.** The caption flows at the window's top left. In Goya
  it is the root cell's `text`, which targets place at the root origin.

## Checks run

`goya-disparate-electron` v0.2.0 extracts 39 elements from this fixture, all
`extracted`, and 30 relationships. The relationships trace Add from start to
finish: the click handler `handles` the button and `invokes` the bridge, the
bridge `exposes` the `counter:add` invoke, that invoke `invokes` the main
handler, and the handler `reads` and `mutates` `count`. `counter:get` only
`reads` it. Every row above and that chain are asserted in
`crates/goya-disparate-electron/tests/extract.rs`, and the full output is
pinned in that crate's golden file.

The scripts pass `node --check`. The main-process logic was tested in Node
(v18.19.1) with a stand-in `electron` module: 11 cases covering canonical
parsing, the i64 bounds and overflow all pass. **Electron has not been run**
because it is not installed on the authoring host. `npm install` fetches
Electron 33.2.1; `npm start` runs it.
