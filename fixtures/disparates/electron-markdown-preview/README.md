# Electron Markdown Preview Editor Fixture

A complete Electron + React + TypeScript fixture for a markdown preview editor with live preview, native file dialogs, and IPC-based filesystem operations.

## Features

- **Single window** Electron application
- **React 18** with TypeScript for the renderer process
- **Vite** for fast development and optimized production builds
- **Marked** library for markdown-to-HTML rendering
- **Split view**: Editor (textarea) on left, live preview on right
- **Native file dialogs** for open/save operations
- **IPC channels** for filesystem and dialog operations
- **Preload bridge** with context isolation
- **No native modules** - pure JavaScript/TypeScript
- **Bounded, literal IPC channels** - no dynamic channel generation

## Architecture

```
src/
├── main.ts           # Main process (CommonJS)
├── preload.ts        # Preload script (CommonJS)
└── renderer/         # Renderer process (ESM)
    ├── index.html
    ├── src/
    │   ├── main.tsx      # React bootstrap
    │   ├── App.tsx       # Main component with markdown editor
    │   ├── style.css     # Split-view styling
    │   └── vite-env.d.ts # TypeScript declarations
```

## IPC Channels

All channels are literal strings, bounded and explicit:

| Channel | Direction | Description |
|---------|-----------|-------------|
| `fs:readFile` | Renderer → Main | Read file contents as UTF-8 string |
| `fs:writeFile` | Renderer → Main | Write string content to file |
| `dialog:openFile` | Renderer → Main | Show native open file dialog |
| `dialog:saveFile` | Renderer → Main | Show native save file dialog |

## Preload API (`window.electronAPI`)

```typescript
interface ElectronAPI {
  fs: {
    readFile: (filePath: string) => Promise<string>;
    writeFile: (filePath: string, content: string) => Promise<void>;
  };
  dialog: {
    openFile: () => Promise<string | null>;
    saveFile: () => Promise<string | null>;
  };
}
```

## Building

```bash
npm install
npm run build
```

This runs:
1. `tsc -p tsconfig.main.json` - Compile main process to `dist/main/main.js`
2. `tsc -p tsconfig.preload.json` - Compile preload to `dist/preload/preload.js`
3. `vite build` - Bundle renderer to `dist/renderer/`

## Running

```bash
npm start
```

For development:
```bash
npm run dev  # Starts Vite dev server
# In another terminal:
npm start    # Runs Electron pointing to localhost:5173
```

## Expected Extraction Behavior

When this fixture is extracted by Goya:

1. **Main process** (`src/main.ts`) - Should be identified as Electron main process with Node.js filesystem operations
2. **Preload script** (`src/preload.ts`) - Should be identified as context-isolated preload bridge
3. **Renderer** (`src/renderer/`) - Should be identified as React + TypeScript + Vite project
4. **IPC channels** - All four channels should be discoverable as literal strings
5. **Dependencies** - `electron`, `react`, `react-dom`, `marked`, `vite`, `typescript` with exact versions
6. **Build scripts** - `build`, `start`, `dev` should be preserved

## File Operations

- **Open**: Ctrl+O or toolbar button → native dialog → reads file → populates editor
- **Save**: Ctrl+S or toolbar button → writes to current path (or Save As if new)
- **Save As**: Ctrl+Shift+S or toolbar button → native dialog → writes file

## Markdown Rendering

Uses `marked` library with synchronous parsing for live preview. Supports:
- Headings (h1-h6)
- Paragraphs, links, images
- Code blocks (fenced and inline)
- Blockquotes
- Lists (ordered/unordered)
- Tables
- Horizontal rules
- Task lists (checkboxes)

## Styling

- Dark theme optimized for code editing
- 50/50 proportional split (responsive to window size)
- Mobile-responsive: stacks vertically on narrow screens
- Custom scrollbar styling
- Syntax-highlighting-ready code blocks