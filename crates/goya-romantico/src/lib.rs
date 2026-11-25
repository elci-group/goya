//! GY-ROMANTICO/0.1 — in-process portable session continuity.
//!
//! romántico keeps a Goya session's semantic state independent of the device
//! that is currently drawing it. This crate is the shared contract for that
//! state: a canonical encoding, an authoritative transaction, a single
//! transferable writer, and the Goya Pairs reducer used to exercise them.
//!
//! The crate does not open a socket. The admitted transport is an in-process
//! ChaCha20-Poly1305 channel keyed from the pairing secret. Discovery stays
//! on the manual endpoint. After both hellos, a follower intent and an
//! authoritative mutation cross that channel and the follower acknowledges
//! the journal. A cooperative handoff moves the single writer on that same
//! channel. A caller-held journal reloads that session after the replica is
//! dropped. A controller advertises four capability booleans and submits
//! intents. It does not become the writer. A compiled reducer and an
//! independent interpreter share one continuity contract. Execution mode is
//! not part of that contract. A bounded diagnostic trace records session,
//! mutation, snapshot, and handoff outcomes. It is not part of the snapshot.
//! `.gscene` syntax is unchanged: continuity is opt-in by linking the crate.

mod authority;
mod canonical;
mod channel;
mod contract;
mod diagnostic;
mod durable;
mod handoff;
mod mutation;
mod pairing;
mod pairs;
mod schema;
mod session;
mod surface;
mod sync;
mod transport;

pub use authority::{admitted_writer, decode_checkpoint, encode_checkpoint, HandoffRecord};
pub use canonical::{
    decode, encode, fingerprint_bytes, OrderedF64, Value, MAX_BLOB, MAX_BYTES, MAX_DEPTH, MAX_ITEMS,
};
pub use channel::{AppKind, Channel};
pub use contract::{
    decode_contract, encode_contract, negotiate_contract, pairs_continuity_contract,
    ContinuityContract, ExecutionMode,
};
pub use diagnostic::{Diagnostic, MAX_TRACE};
pub use durable::Durable;
pub use handoff::{deliver_abort, deliver_activation, deliver_grant, deliver_prepare};
pub use mutation::{encode_envelope, Apply, Envelope, Operation};
pub use pairing::Pairing;
pub use pairs::{
    completion_permille, enqueue_select, execute_intent, join, open, remaining_pairs,
    resolve_reveal, resume, select, ACTION_RESOLVE, ACTION_SELECT, PAIRS_ID, SCHEMA,
};
pub use schema::{negotiate, Class, FieldDef, Manifest, Schema, WireType};
pub use session::{Grant, Intent, Replica, Snapshot, MAX_JOURNAL, MAX_PENDING};
pub use surface::{
    adopt_surface, assign_surface, decode_capabilities, deliver_capabilities, encode_capabilities,
    Assignment, DeviceCaps, Permissions, SurfaceDecl, SurfaceRole,
};
pub use sync::{deliver_intent, deliver_mutation};
pub use transport::{
    admit_transport, select_discovery, DiscoveryChoice, DiscoveryMechanism, PlatformDiscovery,
    TransportCapabilities,
};

/// Contract implemented by this crate.
pub const CONTRACT: &str = "GY-ROMANTICO/0.1";

/// Negotiated protocol version. Independent of [`CONTRACT`].
pub const PROTOCOL: &str = "0.1";

/// Recoverable romántico failure. `code` is stable; `detail` is a fixed sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    pub detail: &'static str,
}

impl Error {
    pub const fn new(code: &'static str, detail: &'static str) -> Self {
        Self { code, detail }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for Error {}

/// Build a device or session identifier from one distinguishing byte.
///
/// The other bytes are zero. Callers that need unguessable identities supply
/// those bytes themselves; this helper only keeps tests and hosts explicit.
pub fn id_from_byte(tag: u8) -> [u8; 16] {
    let mut id = [0u8; 16];
    id[15] = tag;
    id
}

#[cfg(test)]
mod conformance;
#[cfg(test)]
mod harden;
