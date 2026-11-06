//! goya-zcode — GY-ZCODE/1 read-only ZCode session monitor.
//!
//! See docs/goya-zcode.md for the contract. The crate is split into:
//! - [`view`]: the UI fact source (projection) — pure logic, unit-tested.
//!   Wave 3 (GY-ZCODE-003) adds a bounded scrollback (`history`, cap
//!   2000) with `scroll`/`scroll_up`/`scroll_down`/`scroll_to_bottom`;
//!   new events never hijack the scroll position.
//! - [`agent`]: the NDJSON protocol client for the ZCode CLI
//!   `app-server`. Wave 3 adds the `AgentCmd::SendText` inbound queue on
//!   [`agent::MonitorHandle`]: the UI thread enqueues, the agent thread
//!   (stdin owner) issues `session/send {sessionId, content}` — one
//!   outstanding send at a time, server errors surface as view status.
//! - [`composer`]: pure editable-input state machine for the Wave 2
//!   (GY-ZCODE-002) composer input node — focus, char-based cursor,
//!   bounded commit history; host-mapped Wayland events feed it, it never
//!   depends on goya-wayland types.
//! - [`theme`]: Wave 4 (GY-ZCODE-004) theming adapter — derives the
//!   monitor palette from `goya-chroma` (dark monitor theme, AA-enforced)
//!   and maps node names / status text onto per-frame primitive fills;
//!   scene `fill:` values stay as the layout-time defaults.
//! - [`clipboard`]: Wave 4 clipboard ring over `goya-clipboard`'s
//!   `MemoryClipboard` with a permissive local policy and a bounded
//!   last-8 ring. Ctrl+C/Ctrl+V stay in-process — the Wayland data-device
//!   clipboard is out (blocked upstream, GY-WAYLAND-001 follow-through).
//! - [`markdown`]: Wave 4 pure markdown block renderer (headings, fenced
//!   code gutter, bullets, ordered lists, inline marker stripping);
//!   `SessionView::push_markdown` routes turn-completed responses through
//!   it when `markdown` is on (default).
//! - Wave 5 (GY-ZCODE-005): `agent::map_event` decodes `tool.updated`
//!   (progress folds into Started; unknown kinds inert) into
//!   `ProjEvent::ToolUpdate`; [`view::SessionView`] renders append-only
//!   `⚙ <tool> …` / `✓` / `✗` rows with a bounded (cap 32) in-flight and
//!   terminal-id tracker for dedup — Scheduled is silent and nothing is
//!   emitted after a terminal line. The composer consumes
//!   `PlatformEvent::Preedit` (bracketed « … » overlay) and
//!   `PlatformEvent::CommitText` (insert at cursor); the live host
//!   activates the window text input iff the composer is focused.
//! - Wave 6 (GY-ZCODE-006): multi-session switching.
//!   [`agent::AgentCmd::SwitchSession`] tears down the current run/connection
//!   (up, down or reconnecting) and starts a fresh run pinned to the target
//!   session (fresh `session/list` + `session/subscribe`);
//!   `MonitorHandle::switch_session` owns the sequence: pin, abandon
//!   in-flight sends, reset transcript, `"<n>/<m> switching session…"`
//!   status, skip the reconnect backoff. [`view::SessionView`] gains a
//!   selection index (`cycle_selection`, wrapping both directions) and
//!   `reset_transcript` — the transcript is session-scoped and reset on
//!   switch. The bin
//!   wires `Shortcut::{SessionNext, SessionPrevious}` (gated on J1's
//!   goya-wayland variants; ctrl+Left/Right) to cycle + switch, and the
//!   status line carries the `<n>/<m>` index.

pub mod agent;
pub mod clipboard;
pub mod composer;
pub mod markdown;
pub mod theme;
pub mod view;
