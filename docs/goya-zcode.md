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
               [--live | --stdin-dump] [--watch] [--frames <n>] [--duration <seconds>]`

- Default (headless acceptance): parse scene, run projection unit checks,
  print PASS/FAIL, exit non-zero on failure — same shape as `goya-scene`.
- `--stdin-dump`: live protocol run without a window; prints the projection
  (link/sessions/tail) to stdout as it changes. Acceptance path on
  non-Wayland hosts.
- `--live`: Wayland window.
- `--frames <n>`: stop live rendering after `n` frames (also selects live mode).
- `--duration <seconds>`: stop either live or dump mode after the requested
  duration. Both numeric options require positive integers; conflicting live
  and dump modes are rejected.

## Acceptance

1. `cargo test -p goya-zcode` — projection reducer and `wrap_line` tests pass.
2. Live: spawn a real ZCode CLI app-server, run `zcode-monitor --stdin-dump`,
   observe link up, session list, and streaming tail lines for a running
   session; record under `evidence/`.

## Explicit non-goals

- Sending composed text to a session. The current composer is local-only and
  commits entries to the transcript with a `[local]` marker.
- Scrolling (only tail-N virtualization exists here).
- Theming plumbing to the renderer (colors are flat bg/fg per frame).
- Web, phone remote, and any replacement of the existing desktop frontend.

## Wave 2 — platform gaps closed by swarm (GY-ZCODE-002)

Executed per the GY-WAVE2-SWARM convention: concurrent agents with exclusive
crate ownership implementing the shared interface contract below; a central
integration gate runs the workspace gates. The contract is frozen before
dispatch — agents implement against it, never negotiate it.

### Shared interface contract (frozen)

1. **Editable input node (J1: `goya-ir` + `goya-lang`).**
   - `goya_ir::Node` gains `Input { placeholder: String }` (existing variants
     unchanged).
   - Scene syntax: `@input name { position/extent/fill/stroke/text-size;
     text: "<placeholder>" }` (same geometry vocabulary as `@cell`).
   - `goya_lang::DrawNode` gains `pub is_input: bool` (default false;
     `CellKind`/`SceneCell` mirror it).
   - Focus is host-side state; parse/layout stay deterministic (GY-LAYOUT/1).
2. **Renderer (J2: `goya-wayland`).**
   - `present`/`present_dirty` honour per-node fills: scene `fill:` reaches
     the screen. Mechanism: primitives carry the fill (new
     `Primitive::{Text,Button}` variant shape with `Option<u32> fill` in
     `goya-runtime`… **correction**: to keep the frozen boundary tight, the
     fill travels out-of-band — `present`/`present_dirty` gain a parallel
     `fills: &[Option<u32>]` slice indexed by primitive order; the flat
     bg/fg arguments remain for primitives without a fill. `goya-hello`
     call-site migration happens at the integration gate, not by J2.
   - Text containing `'\n'` rasterizes as multiple lines, line height =
     glyph_px * 1.4, clipped to the node rect.
   - `PlatformEvent` gains `KeySpecial(KeySpecial)` with `enum KeySpecial {
     ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Home, End, Delete, Escape,
     Tab }` (Enter stays `'\n'`, Backspace stays `'\u{8}'` in `KeyChar`).
     Evdev US-subset; unmapped keycodes stay `None`.
3. **Composer (J3: `goya-zcode`).**
   - The monitor scene gains an `@input composer` node; the host maintains
     focus (click on the composer via `hit_test` focuses it), cursor
     position, and typed text editable with the KeySpecial set +
     `'\u{8}'`/`'\n'`.
   - Enter commits: the monitor does NOT auto-send (read-only link); Enter
     echoes the composed line into the tail marked `[local]` and clears the
     field. Sending is a later increment (needs `session/send` + v4
     admission).
   - Composer text renders in the input node per frame (host override).

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-ir`, `goya-lang` | `Node::Input` + parser + `DrawNode.is_input` + unit tests |
| J2 | `goya-wayland` | per-node fills, multi-line raster, KeySpecial events + tests |
| J3 | `goya-zcode` | composer state machine + scene update + tests (pure logic) |
| gate | workspace | cargo build/test/clippy/fmt, all `present(fills, ...)` call-site migrations, live keyboard/composer acceptance, evidence |

The workspace gate passes: `cargo test --workspace --lib --tests -j 1`,
`cargo fmt --all -- --check`, and strict workspace Clippy all succeed. The
headless host acceptance also covers composer typing, backspace, and local
commit. The gate remains open until interactive Wayland keyboard/composer
acceptance and the real ZCode app-server session stream are recorded; the
headless check does not establish native input delivery or live protocol
behavior. The public GY-IR/2 executable bytecode backend renders an input
node's static placeholder with the existing text opcode; editable contents
and focus remain host-side state and are not part of bytecode v1.

Contention rule: agents may run `cargo check -p <own crates>` only; the gate
runs all workspace gates. All changes spec-first; each agent updates only its
own crate's docs/comments.

