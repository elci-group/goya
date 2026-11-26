# DEL-B01-REACT: Electron + React Counter

This fixture is a minimal Electron + React + TypeScript application that implements the same counter logic as `electron-counter`, but uses React with hooks (`useState`, `useEffect`, `useCallback`) and JSX.

## Structure

```
src/
├── main.ts           # Main process (TypeScript)
├── preload.ts        # Preload bridge (TypeScript)
└── renderer/
    ├── index.html    # HTML entry point
    └── src/
        ├── main.tsx  # React bootstrap
        ├── App.tsx   # Counter component with hooks
        └── style.css # Same proportional layout as electron-counter
```

## Build

```bash
npm install
npm run build
```

The built output in `dist/` is what the extractor analyzes:
- `dist/main.js` (compiled main process)
- `dist/preload.js` (compiled preload)
- `dist/renderer/index.html` (with hashed asset references)
- `dist/renderer/assets/index-<hash>.js` (bundled React app)
- `dist/renderer/assets/index-<hash>.css` (bundled CSS)

## Actual Extraction Results (2026-11-26, goya-disparate-electron v0.2.0)

Run on the `app-dist/` directory (built output without Electron binaries):

```bash
cargo run -p goya-disparate-electron -- extract fixtures/disparates/electron-react-counter/app-dist --summary
```

### Summary

| Metric | Value |
|--------|-------|
| Elements | **509** |
| Relationships | **17** |
| All representability | `extracted` (0 `undetermined`) |
| Entry points | `main` only (renderer page not resolved) |
| Framework | Electron 33.2.1 |

### Elements Breakdown

| Kind | Count | Notes |
|------|-------|-------|
| `package_manifest` | 1 | With React, electron, vite, electron-builder |
| `window_lifecycle` | 6 | `new BrowserWindow`, `loadURL` (dev), `loadFile` (prod), `app.whenReady`, `app.on`, `app.quit` |
| `ipc_channel` | 4 | `counter:add`/`get` × main + preload sides |
| `preload_bridge` | 1 | Key `counter`, members `add`/`get` |
| `logic_unit` | 2 | `parseCanonicalI64`, `createWindow` |
| `state_store` | 1 | `count` (main) — renderer `pending` inlined by bundler |
| `module_import` | 6+ | `electron`, `path`, React internals |
| `component` | 9 | HTML only (`html`, `head`, `meta`, `title`, `script`, `link`, `body`, `div#root`) |
| `event_handler` | 6 | All from bundled JS: `message`, `test`, 4× `click` (no DOM ID targets) |
| `bridge_call` | 2 | `counter.get`, `counter.add` |
| `style_rule` | 5 | CSS rules (same as electron-counter) |
| `unknown` | ~470 | React internals, polyfills, bundled modules — all `extracted` |

### Relationships Produced (17)

| Relation | Count | Basis |
|----------|-------|-------|
| `contains` | 8 | HTML markup nesting |
| `exposes` | 2 | Preload bridge → IPC channels |
| `invokes` | 4 | Preload → main IPC, renderer bridge → preload bridge |
| `mutates` | 1 | Main IPC `counter:add` → `count` state |
| `reads` | 2 | Main IPC `counter:add`/`get` → `count` state |

### Relationships Missing (vs electron-counter's 30)

| Missing | Why |
|---------|-----|
| `handles` (click→button, paste→input) | Bundled JS uses minified variable names; `document.getElementById('window-add')` resolves to `e.getElementById("window-add")` where `e` is not tracked to the DOM |
| `styles` (CSS `#id` → component) | HTML components have `process: "unresolved"` (renderer page not linked to main process) |
| `loads` (window.loadFile → HTML root) | `loadFile` argument is a computed `path.join(__dirname, 'renderer/index.html')`, not a literal |

### Root Cause: Vite Bundling

The extractor runs on **production bundles**, not source. Vite/Rollup:
- Minifies variable names (`document` → `e`, `window` → `t`)
- Inlines `useState`/`useCallback` logic — no distinct function declarations
- Converts React synthetic handlers (`onClick`, `onPaste`) to internal event system — not `addEventListener`
- Produces computed paths for `loadFile`

### What Works

- ✅ Full IPC topology: renderer → preload bridge → main handlers
- ✅ Main process state (`count`) with `reads`/`mutates`
- ✅ Preload bridge exposure (`counter.add`/`get`)
- ✅ Electron framework detection with version
- ✅ CSS rules extracted with proportional values
- ✅ HTML structure preserved

### What Needs Source Maps / Source Analysis

- ❌ DOM handler → component links (`handles`)
- ❌ CSS → component links (`styles`)
- ❌ Window → page link (`loads`)
- ❌ React component tree (`contains` for JSX elements)
- ❌ Hook classification (`useState` → `state_store`)

### Undetermined Elements

**None** — all 509 elements are `extracted`. The bundler eliminates dynamic patterns this extractor flags (no dynamic `require`, no computed IPC channels in this build).

### Known Differences from electron-counter

1. **Bundled output** — 470+ additional `unknown`/`module_import` elements from React internals and polyfills
2. **No React component tree** — JSX compiled to `React.createElement` calls; no `component` elements for `App`, `main`, `input`, etc.
3. **No synthetic event handlers** — `onClick`/`onPaste` become internal React calls, not `addEventListener`
4. **Single `state_store`** — Renderer `pending` promise inlined/minified
5. **Renderer process role `unresolved`** — HTML file not linked to main process `loadFile` (computed path)

### Next Steps for Richer Extraction

1. **Source map support** — Map bundle positions back to source (planned)
2. **Source-level extraction** — Run on `src/renderer/src/` with a React-aware pass
3. **React framework detection** — Identify React, classify hooks, component tree

### Build & Run

```bash
cd fixtures/disparates/electron-react-counter
npm install
npm run build
# Copy dist/main.js, dist/preload.js, dist/renderer/, package.json to app-dist/
cargo run -p goya-disparate-electron -- extract fixtures/disparates/electron-react-counter/app-dist --summary
```