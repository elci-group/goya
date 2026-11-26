# DEL-B01-EDITOR: Electron + React Text Editor

This fixture is a minimal Electron + React + TypeScript text editor application that demonstrates filesystem operations (read/write) and native dialogs (open/save) via IPC. It uses React with hooks (`useState`, `useCallback`) and JSX.

## Structure

```
src/
├── main.ts           # Main process (TypeScript) - fs.readFile, fs.writeFile, dialog handlers
├── preload.ts        # Preload bridge (TypeScript) - exposes fs and dialog APIs
└── renderer/
    ├── index.html    # HTML entry point
    └── src/
        ├── main.tsx  # React bootstrap
        ├── App.tsx   # TextEditor component with hooks, large text state (>64KB potential)
        ├── style.css # Proportional layout with toolbar + editor area
        └── vite-env.d.ts # Window type declarations
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

## Expected Extraction (Oracle)

### IPC Channels (bounded, literal strings)

| Channel | Direction | Description |
|---------|-----------|-------------|
| `fs:readFile` | Renderer → Main | Read file contents (utf-8) |
| `fs:writeFile` | Renderer → Main | Write file contents (utf-8) |
| `dialog:openFile` | Renderer → Main | Show native open file dialog |
| `dialog:saveFile` | Renderer → Main | Show native save file dialog |

### Preload Bridge

Two namespaces exposed on `window`:
- `window.fs` — `{ readFile, writeFile }`
- `window.dialog` — `{ openFile, saveFile }`

### Main Process State

No persistent state in main process — all file operations are stateless. The text content state lives in the React renderer (`useState` for text, can exceed 64KB to test D1 text cap).

### React Component Tree

- `App` (root)
  - `header#window-toolbar`
    - `button#window-open`
    - `button#window-save`
    - `span#window-filepath`
  - `textarea#window-editor` (controlled, large text state)
  - `footer#window-status`

### CSS Layout (Proportional)

| Selector | Position | Size |
|----------|----------|------|
| `#window-root` | relative | 100% × 100% |
| `#window-toolbar` | absolute top | 100% × 8% (min 40px) |
| `#window-editor` | absolute | 98% × 82% (1% margins) |
| `#window-status` | absolute bottom | 98% × 6% (min 24px) |

### Relationships Expected

| Relation | Count | Basis |
|----------|-------|-------|
| `contains` | ~10 | HTML markup nesting (html → head/body, body → div#root, root → main → header/textarea/footer, header → buttons/span) |
| `exposes` | 4 | Preload bridge: `fs.readFile`, `fs.writeFile`, `dialog.openFile`, `dialog.saveFile` |
| `invokes` | 8 | Preload → main IPC (4), Renderer bridge → preload bridge (4) |
| `handles` | 4+ | Click handlers on buttons, change handler on textarea |
| `styles` | 6+ | CSS rules → components |
| `reads`/`mutates` | 0 | No main process state (stateless IPC) |
| `loads` | 1 | `window.loadFile` → renderer HTML |

### Key Differences from electron-react-counter

1. **No main process state** — All file operations are stateless; text state lives in renderer
2. **Large text state** — `useState('')` for text content can exceed 64KB (tests D1 text cap)
3. **Two preload namespaces** — `fs` and `dialog` vs single `counter`
4. **Native dialogs** — `dialog.showOpenDialog` / `showSaveFile` in main process
5. **Textarea** — Controlled component with `onChange` vs input with paste prevention
6. **Proportional flex layout** — Toolbar + editor + status bar vs absolute positioned counter

### Build & Run

```bash
cd fixtures/disparates/electron-text-editor
npm install
npm run build
# Copy dist/main.js, dist/preload.js, dist/renderer/, package.json to app-dist/
cargo run -p goya-disparate-electron -- extract fixtures/disparates/electron-text-editor/app-dist --summary
```