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

pub mod agent;
pub mod composer;
pub mod view;
