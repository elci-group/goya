//! The peer end of Goya's `goya.lanlink/0`: the machine a webOS TV pairs
//! with when its scene declares `@link`. Arachnid generates the TV end
//! (`crates/arachnid/src/lanlink.rs`); this crate is the other end, shared
//! by Tapestry's desktop and Bloom's household link.
//!
//! * [`wire`]: the message types and limits.
//! * [`crypto`]: HKDF key and code derivation, and AES-256-GCM sealing,
//!   pinned to the golden vector the TV runtime is tested against.
//! * [`server`]: pairing (commit, reveal, a 6-digit code confirmed on both
//!   sides), sealed messages with a replay window, long polls that carry
//!   presentations or field updates, and acts. The application plugs in
//!   through [`server::App`].
//!
//! **Proof of concept, LAN only.** This is not a reviewed pairing protocol
//! and there is no TLS. Taken from Tapestry's `tv_link.rs` (owner decision,
//! 26 November 2026: one shared peer for Tapestry and Bloom); Tapestry has
//! not been moved onto it yet.

pub mod crypto;
pub mod server;
pub mod wire;

pub use server::{confirm, handle, serve, App, Response, Server};
pub use wire::{DesktopMessage, FieldValue, PROTOCOL};
