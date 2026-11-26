# Electron Task Manager Fixture

A minimal Electron + React + TypeScript task manager for testing extraction tools.

## Features

- Single window Electron app
- In-memory task store in main process (no filesystem persistence)
- React 18 with TypeScript
- Vite for renderer bundling
- TypeScript for main/preload processes (CommonJS, Node resolution)

## IPC Channels

All channels are bounded and literal:

| Channel       | Direction      | Payload              | Returns                |
|---------------|----------------|----------------------|------------------------|
| `tasks:list`  | Renderer → Main | —                   | `Task[]`               |
| `tasks:add`   | Renderer → Main | `title: string`     | `Task`                 |
| `tasks:toggle`| Renderer → Main | `id: string`        | `Task \| null`         |
| `tasks:delete`| Renderer → Main | `id: string`        | `boolean`              |

## Preload Bridge

Exposed on `window.tasks`:

```ts
interface TasksAPI {
  list: () => Promise<Task[]>;
  add: (title: string) => Promise<Task>;
  toggle: (id: string) => Promise<Task | null>;
  delete: (id: string) => Promise<boolean>;
}
```

## Task Type

```ts
interface Task {
  id: string;
  title: string;
  completed: boolean;
  createdAt: number;
}
```

## React Component Patterns

The `App.tsx` demonstrates extraction-testing patterns:

- `useState` for task array state
- `useEffect` for initial data load
- `useCallback` for all IPC-bound handlers (`handleAdd`, `handleToggle`, `handleDelete`)
- `.map()` rendering of task list (tests template rejection of array iteration)
- Conditional rendering for empty state

## Build & Run

```bash
npm install
npm run build
npm start
```

For development:

```bash
npm run dev
```

## Expected Extraction Output

When extracted, this fixture should produce:

1. **Package manifest** with exact dependencies and scripts
2. **TypeScript configs** for main (CommonJS) and renderer (ESM via Vite)
3. **Main process** with in-memory store and 4 IPC handlers
4. **Preload script** exposing typed `tasks` namespace
5. **Renderer entry** (HTML + React bootstrap)
6. **React component** with CRUD operations via preload bridge
7. **Styles** with proportional layout (flexbox, responsive)
8. **Type declarations** for `window.tasks`

## License

MIT OR Apache-2.0