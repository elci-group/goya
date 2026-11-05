# goya-zcode — ZCode session monitor contract (GY-ZCODE/1)

Status: Candidate. This document is the spec for the `goya-zcode` crate and its
`zcode-monitor` host binary: a read-only native Goya window that mirrors a live
ZCode (drowzy) agent session.

## Motivation

Increment 1 of "rebuild the frontend on Goya". A full replacement of the
Electron/React frontend is blocked on Goya platform gaps (no text wrapping, no
editable text field, no scrolling, ASCII-only keyboard). This contract defines
the narrowest useful slice that exercises the full integration path:
`.gscene` UI + external data source + live per-frame text override + Wayland
presentation.

## Inputs

- The ZCode CLI protocol server: spawned as `<cli> app-server` (path via
  `--cli <path>` or `ZCODE_CLI` env; default `zcode`). Its stdout is a strict
  NDJSON frame channel; diagnostics go to stderr.
- Wire subset consumed (mirrors `packages/shared/src/zcode-protocol` in the
  drowzy monorepo; server schemas are strict, so requests must match exactly):
  - Requests: `{"id": <n>, "method": ..., "params": {...}}`.
  - Responses: `{"id": <n>, "result"}` / `{"id": <n>, "error"}`.
  - Server notifications: `{"method": "session/event", "params": <envelope>}`,
    `{"method": "state.updated", ...}` (ignored).
  - `session/list` params `{workspace: {workspacePath, workspaceKey}}`
    (workspaceKey = trimmed workspace path for local workspaces; identity rule
    per drowzy `workspaceIdentity` spec), result `{sessions: [...]}`.
  - `session/subscribe` params `{sessionId, deliveryKind: "desktop-continuous"}`
    result `{sessionId, eventSeq, events}`; live events then arrive as
    `session/event` notifications.
  - Event envelope: `{eventId, sessionId, turnId?, seq, timestamp, type,
    payload?, deliveryKind?}`. Handled types: `turn.started`,
    `turn.completed`, `turn.failed`, `part.delta`. All other types are inert
    (Goya runtime convention: unknown events never crash the host).

## Projection state

`SessionView` is the single UI fact source (one owner; the render loop never
mutates it, only reads a snapshot):

- `link`: `connecting | up | down(reason)`.
- `sessions`: list of `(sessionId, title, status)` from the last `session/list`.
- `active`: for the monitored session — `title`, `status`
  (`idle|running|waiting|paused|completed|error`), and `tail`: bounded
  transcript lines (VecDeque, cap 14). Reduction:
  - `turn.started` → push `» <input preview>` line, status `running`.
  - `part.delta` (field `text`) → append to the streaming buffer; the buffer
    renders as the last tail line while running.
  - `turn.completed` → push final response preview, status `idle`;
    `turn.failed` → push error preview, status `error`.
- Lines are wrapped at a fixed character width by a pure `wrap_line` helper
  (no Goya wrapping exists) and truncated with an ellipsis.

Threading: the agent thread owns the protocol connection and is the only
writer; updates land in a `Mutex<SessionView>` and set a dirty flag delivered
over an mpsc tick channel (same shape as the calculator host's redraw path).

## Scene contract

`apps/zcode-monitor.gscene`: header cells `title` / `status`, transcript rows
`row_00`…`row_13`, `quit` button. The host overrides row/header `text` fields
each frame before conversion to primitives (calculator `with_result` pattern).
`--watch` enables transactional hot-swap via `SceneStore::swap`.

## Host binary

`zcode-monitor [--cli <path>] [--workspace <path>] [--session <id>]
               [--live | --stdin-dump] [--watch] [--frames <n>]`

- Default (headless acceptance): parse scene, run projection unit checks,
  print PASS/FAIL, exit non-zero on failure — same shape as `goya-scene`.
- `--stdin-dump`: live protocol run without a window; prints the projection
  (link/sessions/tail) to stdout as it changes. Acceptance path on
  non-Wayland hosts.
- `--live`: Wayland window.

## Acceptance

1. `cargo test -p goya-zcode` — projection reducer and `wrap_line` tests pass.
2. Live: spawn a real ZCode CLI app-server, run `zcode-monitor --stdin-dump`,
   observe link up, session list, and streaming tail lines for a running
   session; record under `evidence/`.

## Explicit non-goals (blocked on Goya platform gaps)

- Sending input / composer (no editable text field; keyboard is ASCII-only).
- Scrolling (only tail-N virtualization exists here).
- Theming plumbing to the renderer (colors are flat bg/fg per frame).
- Web, phone remote, and any replacement of the existing desktop frontend.
