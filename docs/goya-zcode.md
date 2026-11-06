# goya-zcode — ZCode session monitor contract (GY-ZCODE/1)

Status: Candidate. This document is the spec for the `goya-zcode` crate and its
`zcode-monitor` host binary: a read-only native Goya window that mirrors a live
ZCode (drowzy) agent session.

## Motivation

Increment 1 of "rebuild the frontend on Goya". A full replacement of the
Electron/React frontend remains blocked on platform gaps (no general text
wrapping, no scrolling, and non-ASCII glyph rendering). This contract defines
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
   - `PlatformEvent::KeyChar(char)` uses the compositor's XKB_V1 keymap for
     Unicode translation, Shift/Caps Lock/AltGr, and active layout. The host
     reads at most 16 MiB of keymap text; unsupported or invalid maps retain
     the bounded US fallback. Dead-key compose sequences are not implemented.
   - `PlatformEvent` gains `KeySpecial(KeySpecial)` with `enum KeySpecial {
     ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Home, End, Delete, Escape,
     Tab }` (Enter stays `'\n'`, Backspace stays `'\u{8}'` in `KeyChar`).
     Navigation keys use evdev keycodes; unmapped codes stay `None`.
3. **Composer (J3: `goya-zcode`).**
   - The monitor scene gains an `@input composer` node; the host maintains
     focus (click on the composer via `hit_test` focuses it), cursor
     position, and typed text editable with the KeySpecial set +
     `'\u{8}'`/`'\n'`.
   - Enter commits the composed line. The monitor remains read-only by
     default; the later GY-ZCODE-003 increment adds an opt-in send path gated
     by the explicit `--allow-send` flag.
   - Composer text renders in the input node per frame (host override).

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-ir`, `goya-lang` | `Node::Input` + parser + `DrawNode.is_input` + unit tests |
| J2 | `goya-wayland` | per-node fills, multi-line raster, KeySpecial events + tests |
| J3 | `goya-zcode` | composer state machine + scene update + tests (pure logic) |
| gate | workspace | cargo build/test/clippy/fmt, all `present(fills, ...)` call-site migrations, live keyboard/composer acceptance, evidence |

The gate record in `evidence/GY-ZCODE-002.json` captures the workspace
build/test/lint/format checks, headless composer typing/backspace/local
commit, a live `stdin-dump` session stream, and a Wayland presentation run.
Interactive keyboard/composer acceptance in the live window remains open;
headless key simulation does not establish native input delivery. The public
GY-IR/2 executable bytecode backend renders an input node's static placeholder
with the existing text opcode; editable contents and focus remain host-side
state and are not part of bytecode v1.

Contention rule: agents may run `cargo check -p <own crates>` only; the gate
runs all workspace gates. All changes spec-first; each agent updates only its
own crate's docs/comments.

## Wave 3 — send path + history scroll (GY-ZCODE-003)

Same swarm convention. Contract frozen before dispatch.

### Shared interface contract (frozen)

1. **Keys (J1: `goya-wayland`).**
   - `KeySpecial` gains `PageUp`, `PageDown` (evdev codes 104 / 109).
   - `PlatformEvent` gains `PointerScroll(lines: i32)` (evdev REL_WHEEL,
     positive = wheel up / content scroll up); unmapped devices emit nothing.
2. **Send path (J2: `goya-zcode`).**
   - Wire: `session/send` params `{sessionId, content}` only (all other
     fields optional server-side; strict schema verified against drowzy
     `zcodeSessionSendParamsSchema`). Result `{sessionId, accepted: true,
     stateRevision}`. Server rejects with error `-32010` while a turn is
     active — surfaced as view status, never a crash.
   - Channel: `MonitorHandle` gains an inbound command queue
     (`AgentCmd::SendText(String)`); the UI thread enqueues on Enter, the
     agent thread (stdin owner) issues the request. One outstanding send at
     a time; further Enters while pending are dropped with a status hint.
   - View: `SessionView` gains `history: Vec<String>` (bounded, cap 2000) of
     committed tail lines and `scroll: usize` (lines back from newest;
     0 = live). `rows()` renders the 14-line window at `scroll`; new events
     snap to bottom only when already at bottom (no scroll hijack). A
     successful send sets status `sending`; `turn.started` confirms and
     clears it.
   - Bin: Enter (composer focused) sends the committed line; status shows
     `sending` until `turn.started`. PageUp/PageDown and `PointerScroll`
     scroll history when the composer is NOT focused (arrow keys stay
     composer-only). `[local]` echo is retired — the wire send is the
     record.
3. **Gate.** A live acceptance that actually sends is token-bearing, so
   sending is armed ONLY by the explicit `--allow-send` flag (default off).
   Default acceptance exercises the pending/blocked path without spending
   tokens.

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-wayland` | PageUp/PageDown + PointerScroll + tests |
| J2 | `goya-zcode` | AgentCmd queue, session/send, scrollable history, bin wiring, tests |
| gate | workspace | gates, live acceptance (no-send default), evidence |



## Wave 3 addendum — extra work packages (same swarm, no crate overlap)

Dispatched while J2/J3 are in flight; exclusive ownership, frozen contract.

4. **Deterministic wrap (J4: `goya-lang`).**
   - Scene vocabulary gains `wrap: <positive integer columns>` on cells
     (parsed like other cell properties, default absent = no wrap).
   - `goya_lang::layout`, when `wrap` is present, splits the cell's text
     into lines at the requested column count (greedy word wrap,
     hard-splitting oversized tokens — same algorithm class as the monitor's
     host-side `wrap_line`) and emits them newline-joined in the node's
     text; the wave-2 renderer already rasterizes `'\n'` (glyph_px*1.4,
     rect-clipped), so wrapped text reaches the screen with no renderer
     change.
   - Layout stays a pure function of (module, constraints) — wrapping is
     deterministic, no measurement (GY-LAYOUT/1 intact). `DrawNode` shape is
     unchanged.
   - Tests: wrap produces expected lines; absent `wrap` unchanged; hot-swap
     accepts wrap edits; layout determinism (same input twice → identical).
   - NOTE for gate/J2: goya-zcode's host-side wrap stays as-is this wave;
     migrating the monitor to scene-level `wrap` is a later increment.
5. **Studio authoring (J5: `goya-studio` + `goya-sdk`).**
   - Studio's `check`/`inspect`/`serve` handle `@input` nodes (node list,
     geometry inspector) and show `wrap` when present; preview renders
     placeholder text for inputs via the existing text opcode (no renderer
     work).
   - `goya_sdk::SceneProject` exposes input nodes (name/placeholder/geometry)
     so SDK consumers can enumerate them.
   - Tests follow existing crate test style.

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J4 | `goya-lang` | `wrap:` property + deterministic layout wrapping + tests |
| J5 | `goya-studio`, `goya-sdk` | @input/wrap awareness in check/inspect/serve + SDK accessors + tests |

## Wave 4 — theming, clipboard ring, markdown blocks (GY-ZCODE-004)

Same swarm convention; contract frozen before dispatch. Ownership: J1
`goya-chroma`, J2 `goya-wayland`, J3 `goya-zcode`; gate runs workspace gates
and live acceptance.

### Shared interface contract (frozen)

1. **Palette (J1: `goya-chroma`).**
   - `pub struct Palette { pub bg: Rgb8, pub fg: Rgb8, pub muted: Rgb8,
     pub accent: Rgb8, pub border: Rgb8, pub ok: Rgb8, pub err: Rgb8 }`
     (Rgb8 is the crate's existing type).
   - `pub fn palette(theme: &Theme, polarity: Polarity) -> Result<Palette,
     ResolveError>` derived from the existing `resolve()`; bg/fg/accent/border
     from the resolved roles; muted = surface fg blended toward surface;
     ok/err fixed-hue anchors contrast-checked against bg (AA, like resolve).
   - AA must hold for fg/bg, accent/bg, ok/bg, err/bg; conflict =
     `ResolveError::ContrastUnreachable` (never silently fake, GY-CHR-001 §5).
2. **Shortcuts (J2: `goya-wayland`).**
   - `pub enum Shortcut { Copy, Paste, Quit }`; `PlatformEvent::Shortcut(
     Shortcut)` emitted from ctrl+c / ctrl+v / ctrl+q through the xkb state
     path (ctrl is currently inert there — this is its first consumer;
     modifier tracking already exists).
   - Keymap-free evdev fallback emits no Shortcut events (documented; ctrl is
     untracked there). Other ctrl combos stay inert. Navigation/KeyChar
     behavior byte-identical to wave 3.
   - Pure decision helper extracted + tests (no compositor), wave-3 style.
3. **Monitor (J3: `goya-zcode`).** J1/J2 land concurrently; code against the
   frozen APIs, guard integration points (see wave-2/3 pattern: adapter
   module + graceful degradation, mark TODOs for the gate).
   - **Theming**: new `theme.rs` adapter exposing `monitor_palette() ->
     MonitorPalette` (bg/fg/muted/accent/border/ok/err as `[u8;3]`).
     Implementation calls `goya_chroma::palette` with the monitor's dark
     Theme when available; until it lands, hand-tuned constants matching the
     current scene colors (documented as fallback). The live render path
     passes real fills via the wave-2 `fills` slice (rows, header, composer,
     status ok/err colors). Scene `fill:` values remain as the layout-time
     defaults; the per-frame fill slice overrides them.
   - **Clipboard ring**: new `clipboard.rs` wrapping `goya_clipboard::
     MemoryClipboard` with a local permissive capability policy. Ctrl+C
     copies the focused/last transcript line into the ring; Ctrl+V inserts
     `last_entry().plain_text` at the composer cursor. System (Wayland
     data-device) clipboard is OUT — blocked upstream (GY-WAYLAND-001
     follow-through); document in code and evidence.
   - **Markdown blocks**: new `markdown.rs` — pure renderer from assistant
     text to styled plain lines: ATX headings → prefixed/emphasised lines;
     fenced code → `| ` gutter + indent (markers stripped); bullets → `• ` ;
     ordered lists kept; bold/italic markers stripped; links kept as plain
     text; paragraphs blank-line separated. `SessionView` gains a
     `markdown: bool` mode (default on for turn-completed response lines);
     `push_markdown(text)` routes through the renderer. Unit tests for every
     construct.
   - Bin wiring: Shortcuts dispatch (Copy/Paste/Quit), palette applied per
     frame, status colors ok/err.

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-chroma` | Palette + palette() + AA tests |
| J2 | `goya-wayland` | Shortcut events + pure helper + tests |
| J3 | `goya-zcode` | theme adapter, clipboard ring, markdown renderer, bin wiring, tests |
| gate | workspace | gates, live acceptance (no-send default), evidence |

## Wave 5 — IME text input + tool-call rows (GY-ZCODE-005)

Same swarm convention; contract frozen before dispatch.

### Shared interface contract (frozen)

1. **IME (J1: `goya-wayland`).** xkb Compose already handles dead keys
   (wave 3); what is missing is the input-method protocol for CJK/complex
   input. J1 implements the client side of `zwp_text_input_v3`:
   - `GoyaWindow::set_text_input_active(&mut self, active: bool)` —
     enable/disable on the seat; disabled by default (no protocol surface
     for hosts that don't ask).
   - `PlatformEvent::Preedit(Option<String>)` (None = preedit cleared) and
     `PlatformEvent::CommitText(String)` from the IME; host commit_string
     text is NOT synthesized as KeyChar events — it is delivered whole so
     the host can insert at its own cursor.
   - Pure state machine for the pending commit/preedit queue, unit-tested
     compositor-free (wave-3 style). No tests can prove an actual IME works;
     that is documented as an operator-verified item in evidence.
2. **Tool rows (J2: `goya-zcode`).** Wire decode of `tool.updated`
   (envelope payload kinds: scheduled/started/progress/completed/failed;
   base fields toolCallId, toolName?, description?, per drowzy
   `zcodeToolUpdatedEventPayloadSchema`):
   - New `ProjEvent::ToolUpdate { tool: String, state: ToolState }` where
     `ToolState ∈ {Scheduled, Started, Completed, Failed}` (progress updates
     are folded into Started — the monitor's line model cannot update in
     place, so progress is deliberately not rendered; documented).
   - Rendering is append-only: Started → `⚙ <tool> …` line; Completed →
     `⚙ <tool> ✓`; Failed → `⚙ <tool> ✗`; Scheduled renders nothing
     (avoid noise). Dedup: a tool that already emitted a terminal line emits
     nothing further.
   - `SessionView` tracks a bounded set of in-flight tool ids (cap 32, evict
     oldest) for dedup; unit tests for the state machine, dedup, and
     rendering.
   - Composer: consume `PlatformEvent::CommitText` (insert at cursor) and
     `Preedit` (rendered inline in the input node, dimmed marker); window
     text input activated iff the composer is focused.
3. **Gate.** Workspace gates; live acceptance watches for real
   `tool.updated` events only if a natural turn runs (still token-free by
   default); evidence records operator-verified IME as an open item.

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-wayland` | text-input-v3 client: Preedit/CommitText + activation API + state-machine tests |
| J2 | `goya-zcode` | tool.updated decode + append-only tool rows + composer preedit/commit wiring + tests |
| gate | workspace | gates, live acceptance, evidence |

## Wave 6 — multi-session switching + runtime cleanup (GY-ZCODE-006)

Same swarm convention; contract frozen before dispatch. J1 owns
`goya-runtime` + `goya-wayland`; J2 owns `goya-zcode`; gate runs workspace
gates, live acceptance (token-free default), evidence.

### Shared interface contract (frozen)

1. **Runtime cleanup + session keys (J1: `goya-runtime`, `goya-wayland`).**
   - `goya-runtime`: remove the divergent `Event::ImeBatch` variant left by
     the auto-committer (flagged in GY-ZCODE-005). First verify no code
     inside goya-runtime constructs or matches it; fix exhaustive matches in
     that crate only. Nothing outside consumes it (verified by gate build).
   - `goya-wayland`: `Shortcut` gains `SessionNext`, `SessionPrevious`,
     emitted for ctrl+Right (code 106) / ctrl+Left (code 105) through the
     existing ctrl+xkb shortcut path; plain arrows unchanged; evdev fallback
     emits none (existing asymmetry). Extend the pure helper + tests.
2. **Session switching (J2: `goya-zcode`).**
   - `AgentCmd::SwitchSession { session_id: String }`; the agent run loop
     handles it by tearing down the current connection and starting a fresh
     run pinned to that session (simplest correct semantics: fresh
     `session/list` + `session/subscribe`; the switch must work while a
     connection is up, down, or reconnecting).
   - `SessionView`: transcript (tail/history) is session-scoped and RESET on
     switch (documented; per-session history buffers are a later increment).
     The sessions list gains a selection index; `status` shows
     `"<n>/<m> <status>"` on switch. `SessionView::cycle_selection(delta)`
     returns the newly selected session id (wrapping), pure + tested.
   - Bin: `Shortcut::SessionNext`/`SessionPrevious` cycle the selection and
     enqueue SwitchSession; the composer unfocuses on switch; in-flight
     sends are abandoned (lifecycle reset, status "switching session…").
   - Headless acceptance: cycling over a synthetic session list selects the
     right ids (wrap both directions); transcript reset on switch; existing
     checks unchanged.

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-runtime`, `goya-wayland` | ImeBatch removal + SessionNext/Previous shortcuts + tests |
| J2 | `goya-zcode` | SwitchSession command, session-scoped reset, cycling + bin wiring + tests |
| gate | workspace | gates, live acceptance, evidence |

## Wave 7 — per-session transcripts + a11y for inputs (GY-ZCODE-007)

Same swarm convention; contract frozen before dispatch.

1. **Per-session transcript retention (J1: `goya-zcode`).**
   - `SessionView` transcript state (tail, history, streaming, scroll,
     in-flight/finished tools) is saved on switch and restored on return,
     keyed by sessionId in a bounded LRU (cap 8 sessions; evicting drops the
     oldest transcript). `reset_transcript` semantics stay for sessions with
     no saved state. Composer text+cursor is retained per session too.
   - Owner: `MonitorHandle::switch_session` (existing single owner) —
     snapshot before reset, restore after pin. Pure helpers + tests:
     snapshot/restore round-trip, LRU eviction, composer retention.
   - Live acceptance: switch away and back over a live session list and
     observe retention (still token-free).
2. **A11y for inputs (J2: `goya-a11y`).**
   - The DIG node model gains input-node support: role Input, label =
     placeholder (or live text when the host supplies it), editable state,
     focusable. Extend the existing traversal/hit-region machinery
     (investigate crate structure first; mirror how buttons are modeled).
   - Tests follow crate style: DIG for a scene with cells/buttons/inputs
     exposes input nodes with roles/labels/states; traversal visits them;
     existing tests unchanged.
   - Note for the gate: wiring the monitor's live a11y snapshot is a later
     increment (needs host-side snapshot updates); this wave is the platform
     vocabulary only.

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-zcode` | per-session transcript + composer retention, LRU, tests |
| J2 | `goya-a11y` | Input role/label/state in DIG + traversal + tests |
| gate | workspace | gates, live retention acceptance, evidence |

## Wave 8 — live a11y snapshot + system clipboard (GY-ZCODE-008)

Same swarm convention; contract frozen before dispatch.

1. **Monitor a11y snapshot (J1: `goya-zcode`).** Wire the monitor to
   goya-a11y per the GY-ZCODE-007 gate recipe:
   - Build a DIG for the monitor scene: header title (label), status
     (role StatusText/label), transcript rows (labels = live row text),
     composer (SemanticRole::Textbox, placeholder label, live_text = typed
     text, focused = composer.focused, hit region from scene rect), quit
     button (press action).
   - Republish on change (dirty flag), not per frame; bounded update path
     per goya-a11y's `run_with_updates` mechanism (inspect its API; if it
     exposes only a full-tree publish, use that and note it).
   - `--a11y-dump` bin flag: print the current DIG/A11yTree once per change
     in dump mode (headless-verifiable acceptance; real screen-reader
     verification remains operator territory).
2. **System clipboard (J2: `goya-wayland`).** Wayland data-device:
   - `GoyaWindow::set_clipboard_text(&mut self, text: &str)` — set the
     selection source to a text/plain payload served from an in-memory
     buffer on selection requests.
   - `GoyaWindow::request_clipboard_text(&mut self)` — ask the compositor
     for the current selection; the result arrives as
     `PlatformEvent::ClipboardText(Option<String>)` (None = no text
     selection). One outstanding request at a time.
   - Pure buffer/state machine tests (offer parsing, request lifecycle);
     live compositor interop remains operator-verified territory.
3. **Gate.** Workspace gates; headless a11y acceptance; evidence.

### Assignments

| Agent | Crates (exclusive) | Deliverable |
|---|---|---|
| J1 | `goya-zcode` | DIG construction + republish + --a11y-dump + tests |
| J2 | `goya-wayland` | data-device set/request + ClipboardText event + state-machine tests |
| gate | workspace | gates, acceptance, evidence |
