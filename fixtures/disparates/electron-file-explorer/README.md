# DEL-B01-REACT: Electron + React File Explorer

This fixture is a minimal Electron + React + TypeScript application that implements a file explorer with directory tree navigation and file content viewing. It demonstrates filesystem operations via IPC channels with bounded, literal channel names.

## Structure

```
src/
├── main.ts           # Main process (TypeScript) - fs.readdir, fs.readFile, dialog.showOpenDialog
├── preload.ts        # Preload bridge (TypeScript) - exposes fs and dialog APIs
└── renderer/
    ├── index.html    # HTML entry point
    └── src/
        ├── main.tsx  # React bootstrap
        ├── App.tsx   # FileExplorer component with useState, useEffect for directory listing
        ├── style.css # Proportional layout (sidebar + main pane)
        └── vite-env.d.ts # Window types for fs and dialog bridges
```

## IPC Channels (Bounded, Literal)

| Channel | Direction | Payload | Response |
|---------|-----------|---------|----------|
| `fs:readdir` | Renderer → Main | `dirPath: string` | `{ ok: boolean, entries?: DirectoryEntry[], error?: string }` |
| `fs:readFile` | Renderer → Main | `filePath: string` | `{ ok: boolean, content?: string, error?: string }` |
| `dialog:openDirectory` | Renderer → Main | (none) | `{ ok: boolean, path?: string, error?: string }` |

## Preload Bridge

```typescript
window.fs = {
  readdir: (dirPath: string) => Promise<ReaddirResult>,
  readFile: (filePath: string) => Promise<ReadFileResult>,
};

window.dialog = {
  openDirectory: () => Promise<OpenDirectoryResult>,
};
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

## Expected Extraction Results (Oracle)

Run on the `app-dist/` directory (built output without Electron binaries):

```bash
cargo run -p goya-disparate-electron -- extract fixtures/disparates/electron-file-explorer/app-dist --summary
```

### Summary (Expected)

| Metric | Value |
|--------|-------|
| Elements | ~600+ |
| Relationships | ~25+ |
| All representability | `extracted` (0 `undetermined`) |
| Entry points | `main` only (renderer page not resolved) |
| Framework | Electron 33.2.1 |

### Elements Breakdown (Expected)

| Kind | Count | Notes |
|------|-------|-------|
| `package_manifest` | 1 | With React, electron, vite, electron-builder |
| `window_lifecycle` | 6 | `new BrowserWindow`, `loadURL` (dev), `loadFile` (prod), `app.whenReady`, `app.on`, `app.quit` |
| `ipc_channel` | 6 | `fs:readdir`, `fs:readFile`, `dialog:openDirectory` × main + preload sides |
| `preload_bridge` | 1 | Key `fs` with `readdir`/`readFile`, key `dialog` with `openDirectory` |
| `logic_unit` | 4 | `createWindow`, `readdir handler`, `readFile handler`, `openDirectory handler` |
| `state_store` | 1 | Renderer state (rootPath, tree, selectedFile, fileContent, loading, error) — inlined by bundler |
| `module_import` | 8+ | `electron`, `fs/promises`, `path`, React internals |
| `component` | 12+ | HTML only (`html`, `head`, `meta`, `title`, `script`, `link`, `body`, `div#root`, `main`, `header`, `aside`, `section`, `nav`, `pre`) |
| `event_handler` | 8+ | From bundled JS: `click` (tree nodes, open button), React synthetic handlers |
| `bridge_call` | 4 | `fs.readdir`, `fs.readFile`, `dialog.openDirectory` × preload + renderer |
| `style_rule` | 15+ | CSS rules with proportional layout values |
| `unknown` | ~500+ | React internals, polyfills, bundled modules — all `extracted` |

### Relationships Produced (Expected)

| Relation | Count | Basis |
|----------|-------|-------|
| `contains` | 15+ | HTML markup nesting |
| `exposes` | 3 | Preload bridge → IPC channels |
| `invokes` | 6 | Preload → main IPC, renderer bridge → preload bridge |
| `mutates` | 0 | No main process state mutation (stateless handlers) |
| `reads` | 3 | Main IPC handlers read filesystem |

### Root Cause: Vite Bundling (Same as electron-react-counter)

The extractor runs on **production bundles**, not source. Vite/Rollup:
- Minifies variable names
- Inlines `useState`/`useCallback`/`useEffect`/`useMemo` logic — no distinct function declarations
- Converts React synthetic handlers (`onClick`, `onChange`) to internal event system — not `addEventListener`
- Produces computed paths for `loadFile`

### What Works

- ✅ Full IPC topology: renderer → preload bridge → main handlers
- ✅ Bounded, literal IPC channels (`fs:readdir`, `fs:readFile`, `dialog:openDirectory`)
- ✅ Preload bridge exposure (`fs.readdir`, `fs.readFile`, `dialog.openDirectory`)
- ✅ Electron framework detection with version
- ✅ CSS rules extracted with proportional values
- ✅ HTML structure preserved
- ✅ Directory tree state management (inlined by bundler)

### What Needs Source Maps / Source Analysis

- ❌ DOM handler → component links (`handles`)
- ❌ CSS → component links (`styles`)
- ❌ Window → page link (`loads`)
- ❌ React component tree (`contains` for JSX elements)
- ❌ Hook classification (`useState` → `state_store`, `useEffect` → side effects)
- ❌ Recursive tree rendering (`renderNode` inlined)

### Undetermined Elements

**None expected** — all elements should be `extracted`. The bundler eliminates dynamic patterns this extractor flags (no dynamic `require`, no computed IPC channels in this build).

### Key Differences from electron-react-counter

1. **Stateless main process** — No `state_store` in main; filesystem is external state
2. **Three IPC channels** — `fs:readdir`, `fs:readFile`, `dialog:openDirectory` (vs 2 counter channels)
3. **Dual preload bridge keys** — `fs` and `dialog` (vs single `counter`)
4. **Complex renderer state** — 6 `useState` values, `useEffect` for initial load, `useCallback`/`useMemo` for handlers
5. **Recursive tree rendering** — `renderNode` function with depth-based indentation
6. **Proportional two-pane layout** — Sidebar (30%) + Main (70%) with overflow handling

### Build & Run

```bash
cd fixtures/disparates/electron-file-explorer
npm install
npm run build
# Copy dist/main.js, dist/preload.js, dist/renderer/, package.json to app-dist/
cargo run -p goya-disparate-electron -- extract fixtures/disparates/electron-file-explorer/app-dist --summary
```