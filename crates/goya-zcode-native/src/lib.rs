//! Goya native monitor adapters.
//!
//! `agent` owns the desktop-continuous app-server connection and command
//! admission; `protocol` validates v4 frames and projects durable rows;
//! `view` owns the UI-facing transcript and viewport. Rendering, input,
//! layout, accessibility, and presentation adapters live in their named
//! modules. Current protocol and parity rules are in `docs/protocol-parity.md`
//! and `docs/gui-parity.md`.

pub mod a11y;
pub mod agent;
pub mod clipboard;
pub mod composer;
pub mod layout;
pub mod markdown;
pub mod protocol;
pub mod theme;
pub mod view;

pub mod accessibility_bridge;
