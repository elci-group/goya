//! In-process authenticated channel.
//!
//! The key is HKDF-SHA256 (RFC 5869) over the accepted 16-byte pairing
//! secret. The cipher is ChaCha20-Poly1305 (RFC 8439). This is not a key
//! exchange and not a socket. Certificate verification is not disabled:
//! the channel has no certificates. A WebSocket adapter still has to verify
//! the peer certificate or refuse to start. After both hellos, application
//! frames carry an intent, a mutation, an acknowledgement, a handoff, or a
//! capability record.

use crate::canonical::{decode, encode, Value};
use crate::pairing::Pairing;
use crate::schema::Manifest;
use crate::Error;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use std::collections::BTreeMap;

const HEADER_LEN: usize = 46;
const TAG_LEN: usize = 16;
const MAX_PAYLOAD: usize = crate::canonical::MAX_BYTES - HEADER_LEN - TAG_LEN;
const KIND_HELLO: u8 = 1;
const KIND_SNAPSHOT: u8 = 2;

/// Application frame carried after both hellos have been accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum AppKind {
    Intent = 3,
    Mutation = 4,
    Ack = 5,
    Prepare = 6,
    Grant = 7,
    Abort = 8,
    /// Four capability booleans. Not a device fingerprint.
    Capabilities = 9,
}

impl AppKind {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            3 => Some(Self::Intent),
            4 => Some(Self::Mutation),
            5 => Some(Self::Ack),
            6 => Some(Self::Prepare),
            7 => Some(Self::Grant),
            8 => Some(Self::Abort),
            9 => Some(Self::Capabilities),
            _ => None,
        }
    }
}
const INFO: &[u8] = b"GY-ROMANTICO/AEAD/1";

/// One direction of an admitted encrypted session between two devices.
pub struct Channel {
    me: [u8; 16],
    peer: [u8; 16],
    key: [u8; 32],
    send_next: u64,
    recv_next: u64,
    hello_sent: bool,
    hello_received: bool,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("me", &self.me)
            .field("peer", &self.peer)
            .field("send_next", &self.send_next)
            .field("recv_next", &self.recv_next)
            .field("hello_sent", &self.hello_sent)
            .field("hello_received", &self.hello_received)
            .finish()
    }
}

impl Channel {
    pub fn establish(me: [u8; 16], peer: [u8; 16], pairing: &Pairing) -> Result<Self, Error> {
        if me == peer {
            return Err(Error::new("RMT-E4005", "transport peers must be distinct"));
        }
        let code = pairing.shared_secret()?;
        Ok(Self {
            me,
            peer,
            key: derive_key(&code, &me, &peer)?,
            send_next: 1,
            recv_next: 1,
            hello_sent: false,
            hello_received: false,
        })
    }

    pub const fn capabilities() -> crate::TransportCapabilities {
        crate::TransportCapabilities {
            authenticated_encryption: true,
            reliable: true,
            max_message_bytes: MAX_PAYLOAD as u32,
        }
    }

    pub fn seal_hello(&mut self, manifest: &Manifest) -> Result<Vec<u8>, Error> {
        if self.hello_sent {
            return Err(Error::new(
                "RMT-E2002",
                "transport hello is already complete",
            ));
        }
        let frame = self.seal(KIND_HELLO, &hello_plaintext(manifest)?)?;
        self.hello_sent = true;
        Ok(frame)
    }

    pub fn open_hello(&mut self, local: &Manifest, frame: &[u8]) -> Result<(), Error> {
        if self.hello_received {
            return Err(Error::new(
                "RMT-E2002",
                "transport hello is already complete",
            ));
        }
        let plaintext = self.open(KIND_HELLO, frame)?;
        if !hello_matches(local, &plaintext)? {
            return Err(Error::new(
                "RMT-E2002",
                "peer schema is incompatible and no migration is registered",
            ));
        }
        self.hello_received = true;
        Ok(())
    }

    pub fn seal_snapshot(&mut self, snapshot: &[u8]) -> Result<Vec<u8>, Error> {
        if !self.negotiated() {
            return Err(Error::new(
                "RMT-E2002",
                "transport negotiation is incomplete",
            ));
        }
        self.seal(KIND_SNAPSHOT, snapshot)
    }

    pub fn open_snapshot(&mut self, frame: &[u8]) -> Result<Vec<u8>, Error> {
        if !self.negotiated() {
            return Err(Error::new(
                "RMT-E2002",
                "transport negotiation is incomplete",
            ));
        }
        self.open(KIND_SNAPSHOT, frame)
    }

    pub const fn local_id(&self) -> [u8; 16] {
        self.me
    }

    pub const fn peer_id(&self) -> [u8; 16] {
        self.peer
    }

    pub fn seal_app(&mut self, kind: AppKind, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        if !self.negotiated() {
            return Err(Error::new(
                "RMT-E2002",
                "transport negotiation is incomplete",
            ));
        }
        self.seal(kind as u8, plaintext)
    }

    pub fn open_app(&mut self, frame: &[u8]) -> Result<(AppKind, Vec<u8>), Error> {
        if !self.negotiated() {
            return Err(Error::new(
                "RMT-E2002",
                "transport negotiation is incomplete",
            ));
        }
        if frame.len() < HEADER_LEN {
            return Err(Error::new("RMT-E4005", "transport frame was rejected"));
        }
        let kind = AppKind::from_byte(frame[13])
            .ok_or(Error::new("RMT-E4005", "transport frame was rejected"))?;
        let plaintext = self.open(kind as u8, frame)?;
        Ok((kind, plaintext))
    }

    fn negotiated(&self) -> bool {
        self.hello_sent && self.hello_received
    }

    fn seal(&mut self, kind: u8, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        if plaintext.is_empty() || plaintext.len() > MAX_PAYLOAD {
            return Err(Error::new(
                "RMT-E8001",
                "transport payload is outside the canonical budget",
            ));
        }
        let sequence = self.send_next;
        let frame = seal_frame(&self.key, sequence, kind, &self.me, &self.peer, plaintext)?;
        self.send_next = self
            .send_next
            .checked_add(1)
            .ok_or(Error::new("RMT-E8001", "transport sequence overflowed"))?;
        Ok(frame)
    }

    fn open(&mut self, kind: u8, frame: &[u8]) -> Result<Vec<u8>, Error> {
        let plaintext = open_frame(&self.key, self.recv_next, kind, &self.peer, &self.me, frame)?;
        self.recv_next = self
            .recv_next
            .checked_add(1)
            .ok_or(Error::new("RMT-E8001", "transport sequence overflowed"))?;
        Ok(plaintext)
    }
}

fn send_direction(me: &[u8; 16], peer: &[u8; 16]) -> u8 {
    if me < peer {
        0
    } else {
        1
    }
}

fn derive_key(code: &[u8; 16], me: &[u8; 16], peer: &[u8; 16]) -> Result<[u8; 32], Error> {
    let (low, high) = if me <= peer { (me, peer) } else { (peer, me) };
    let mut salt = [0u8; 32];
    salt[..16].copy_from_slice(low);
    salt[16..].copy_from_slice(high);
    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&salt), code)
        .expand(INFO, &mut key)
        .map_err(|_| Error::new("RMT-E4005", "transport key derivation was rejected"))?;
    Ok(key)
}

fn nonce_bytes(direction: u8, sequence: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[0] = direction;
    nonce[4..].copy_from_slice(&sequence.to_le_bytes());
    nonce
}

fn header(
    sequence: u64,
    direction: u8,
    kind: u8,
    sender: &[u8; 16],
    receiver: &[u8; 16],
) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[..4].copy_from_slice(b"RTR1");
    out[4..12].copy_from_slice(&sequence.to_le_bytes());
    out[12] = direction;
    out[13] = kind;
    out[14..30].copy_from_slice(sender);
    out[30..46].copy_from_slice(receiver);
    out
}

fn seal_frame(
    key: &[u8; 32],
    sequence: u64,
    kind: u8,
    sender: &[u8; 16],
    receiver: &[u8; 16],
    plaintext: &[u8],
) -> Result<Vec<u8>, Error> {
    let direction = send_direction(sender, receiver);
    let head = header(sequence, direction, kind, sender, receiver);
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| Error::new("RMT-E4005", "transport key was rejected"))?;
    let nonce_bytes = nonce_bytes(direction, sequence);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad: &head,
            },
        )
        .map_err(|_| Error::new("RMT-E4005", "transport frame failed authentication"))?;
    let mut frame = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    frame.extend_from_slice(&head);
    frame.extend_from_slice(&ciphertext);
    if frame.len() > crate::canonical::MAX_BYTES {
        return Err(Error::new(
            "RMT-E8001",
            "transport payload is outside the canonical budget",
        ));
    }
    Ok(frame)
}

fn open_frame(
    key: &[u8; 32],
    expected: u64,
    kind: u8,
    sender: &[u8; 16],
    receiver: &[u8; 16],
    frame: &[u8],
) -> Result<Vec<u8>, Error> {
    if frame.len() < HEADER_LEN + TAG_LEN || frame.len() > crate::canonical::MAX_BYTES {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    if &frame[..4] != b"RTR1" {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    let sequence = u64::from_le_bytes(frame[4..12].try_into().unwrap());
    let direction = frame[12];
    let found_kind = frame[13];
    let found_sender: [u8; 16] = frame[14..30].try_into().unwrap();
    let found_receiver: [u8; 16] = frame[30..46].try_into().unwrap();
    if sequence != expected
        || direction != send_direction(sender, receiver)
        || found_kind != kind
        || found_sender != *sender
        || found_receiver != *receiver
    {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    let head = &frame[..HEADER_LEN];
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| Error::new("RMT-E4005", "transport key was rejected"))?;
    let nonce_bytes = nonce_bytes(direction, sequence);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(
            nonce,
            Payload {
                msg: &frame[HEADER_LEN..],
                aad: head,
            },
        )
        .map_err(|_| Error::new("RMT-E4005", "transport frame failed authentication"))?;
    if plaintext.is_empty() {
        return Err(Error::new("RMT-E2001", "transport payload is empty"));
    }
    Ok(plaintext)
}

pub(crate) fn hello_plaintext(manifest: &Manifest) -> Result<Vec<u8>, Error> {
    let mut record = BTreeMap::new();
    record.insert(1, Value::Text(manifest.protocol.to_string()));
    record.insert(2, Value::Text(manifest.application_id.to_string()));
    record.insert(3, Value::Int(i64::from(manifest.state_schema_version)));
    record.insert(4, Value::Bytes(manifest.schema_fingerprint.to_vec()));
    record.insert(5, Value::Text(manifest.authority_model.to_string()));
    record.insert(
        6,
        Value::Array(
            manifest
                .supported_features
                .iter()
                .copied()
                .map(|feature| Value::Text(feature.to_string()))
                .collect(),
        ),
    );
    encode(&Value::Record(record))
}

fn hello_matches(local: &Manifest, bytes: &[u8]) -> Result<bool, Error> {
    let value = decode(bytes)?;
    if encode(&value)? != bytes {
        return Err(Error::new("RMT-E2001", "transport hello is not canonical"));
    }
    let Value::Record(fields) = value else {
        return Err(Error::new("RMT-E2001", "transport hello is not canonical"));
    };
    if fields.len() != 6 {
        return Err(Error::new("RMT-E2001", "transport hello is not canonical"));
    }
    let protocol = text_field(&fields, 1)?;
    let application = text_field(&fields, 2)?;
    let version = version_field(&fields, 3)?;
    let fingerprint = bytes_field(&fields, 4)?;
    let authority = text_field(&fields, 5)?;
    let features = text_list(&fields, 6)?;
    let same_features = features.len() == local.supported_features.len()
        && features
            .iter()
            .zip(local.supported_features.iter().copied())
            .all(|(got, want)| got == want);
    Ok(protocol == local.protocol
        && application == local.application_id
        && version == local.state_schema_version
        && fingerprint == local.schema_fingerprint
        && authority == local.authority_model
        && same_features)
}

fn text_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<String, Error> {
    match fields.get(&id) {
        Some(Value::Text(text)) => Ok(text.clone()),
        _ => Err(Error::new("RMT-E2001", "transport hello is not canonical")),
    }
}

fn version_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<u32, Error> {
    match fields.get(&id) {
        Some(Value::Int(number)) => u32::try_from(*number)
            .map_err(|_| Error::new("RMT-E2001", "transport hello is not canonical")),
        _ => Err(Error::new("RMT-E2001", "transport hello is not canonical")),
    }
}

fn bytes_field(fields: &BTreeMap<u32, Value>, id: u32) -> Result<[u8; 32], Error> {
    match fields.get(&id) {
        Some(Value::Bytes(bytes)) if bytes.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(bytes);
            Ok(out)
        }
        _ => Err(Error::new("RMT-E2001", "transport hello is not canonical")),
    }
}

fn text_list(fields: &BTreeMap<u32, Value>, id: u32) -> Result<Vec<String>, Error> {
    let Some(Value::Array(items)) = fields.get(&id) else {
        return Err(Error::new("RMT-E2001", "transport hello is not canonical"));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Value::Text(text) = item else {
            return Err(Error::new("RMT-E2001", "transport hello is not canonical"));
        };
        out.push(text.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit_transport;
    use crate::id_from_byte;
    use crate::pairs::{open, SCHEMA};

    fn code() -> [u8; 16] {
        *b"pairing-code-16b"
    }

    fn accepted(secret: &[u8]) -> Pairing {
        let mut pairing = Pairing::offer(secret, 0, 30).unwrap();
        pairing.accept(secret, true, 1).unwrap();
        pairing
    }

    fn linked() -> (Channel, Channel, Manifest) {
        let manifest = SCHEMA.manifest().unwrap();
        let alice =
            Channel::establish(id_from_byte(1), id_from_byte(2), &accepted(&code())).unwrap();
        let bob = Channel::establish(id_from_byte(2), id_from_byte(1), &accepted(&code())).unwrap();
        (alice, bob, manifest)
    }

    fn negotiate(alice: &mut Channel, bob: &mut Channel, manifest: &Manifest) {
        let hello = alice.seal_hello(manifest).unwrap();
        let reply = bob.seal_hello(manifest).unwrap();
        alice.open_hello(manifest, &reply).unwrap();
        bob.open_hello(manifest, &hello).unwrap();
    }

    #[test]
    fn consent_is_required_before_the_channel_exists() {
        let pairing = Pairing::offer(&code(), 0, 30).unwrap();
        let error = Channel::establish(id_from_byte(1), id_from_byte(2), &pairing).unwrap_err();
        assert_eq!(error.code, "RMT-E4001");
    }

    #[test]
    fn identical_device_ids_are_rejected() {
        let error =
            Channel::establish(id_from_byte(1), id_from_byte(1), &accepted(&code())).unwrap_err();
        assert_eq!(error.code, "RMT-E4005");
    }

    #[test]
    fn encrypted_channel_is_admitted() {
        admit_transport(&Channel::capabilities()).unwrap();
    }

    #[test]
    fn snapshot_requires_a_completed_hello() {
        let (mut alice, mut bob, manifest) = linked();
        let snapshot = open(id_from_byte(1), id_from_byte(9), &[0, 1, 2, 3, 3, 2, 1, 0])
            .unwrap()
            .encode_snapshot()
            .unwrap();
        assert_eq!(
            alice.seal_snapshot(&snapshot).unwrap_err().code,
            "RMT-E2002"
        );
        negotiate(&mut alice, &mut bob, &manifest);
        let frame = alice.seal_snapshot(&snapshot).unwrap();
        assert!(!frame
            .windows(snapshot.len())
            .any(|window| window == snapshot));
        assert_eq!(bob.open_snapshot(&frame).unwrap(), snapshot);
    }

    #[test]
    fn tampered_frame_is_rejected() {
        let (mut alice, mut bob, manifest) = linked();
        let mut hello = alice.seal_hello(&manifest).unwrap();
        let last = hello.len() - 1;
        hello[last] ^= 0xff;
        assert_eq!(
            bob.open_hello(&manifest, &hello).unwrap_err().code,
            "RMT-E4005"
        );
    }

    #[test]
    fn replay_and_gap_are_rejected() {
        let (mut alice, mut bob, manifest) = linked();
        negotiate(&mut alice, &mut bob, &manifest);
        let first = alice.seal_snapshot(b"one").unwrap();
        let second = alice.seal_snapshot(b"two").unwrap();
        assert_eq!(bob.open_snapshot(&second).unwrap_err().code, "RMT-E4005");
        assert_eq!(bob.open_snapshot(&first).unwrap(), b"one");
        assert_eq!(bob.open_snapshot(&first).unwrap_err().code, "RMT-E4005");
        assert_eq!(bob.open_snapshot(&second).unwrap(), b"two");
    }

    #[test]
    fn wrong_pairing_code_fails_authentication() {
        let manifest = SCHEMA.manifest().unwrap();
        let mut alice =
            Channel::establish(id_from_byte(1), id_from_byte(2), &accepted(&code())).unwrap();
        let other = [9u8; 16];
        let mut bob =
            Channel::establish(id_from_byte(2), id_from_byte(1), &accepted(&other)).unwrap();
        let hello = alice.seal_hello(&manifest).unwrap();
        assert_eq!(
            bob.open_hello(&manifest, &hello).unwrap_err().code,
            "RMT-E4005"
        );
    }

    #[test]
    fn incompatible_hello_blocks_the_snapshot() {
        let (mut alice, mut bob, manifest) = linked();
        let hello = alice.seal_hello(&manifest).unwrap();
        let mut drifted = manifest.clone();
        drifted.schema_fingerprint[0] ^= 0xff;
        assert_eq!(
            bob.open_hello(&drifted, &hello).unwrap_err().code,
            "RMT-E2002"
        );
        let reply = bob.seal_hello(&manifest).unwrap();
        alice.open_hello(&manifest, &reply).unwrap();
        assert_eq!(
            bob.seal_snapshot(b"snapshot").unwrap_err().code,
            "RMT-E2002"
        );
    }

    #[test]
    fn application_frames_follow_hello_and_leave_snapshots_alone() {
        let (mut alice, mut bob, manifest) = linked();
        assert_eq!(
            alice
                .seal_app(AppKind::Mutation, b"mutation")
                .unwrap_err()
                .code,
            "RMT-E2002"
        );
        negotiate(&mut alice, &mut bob, &manifest);
        let snapshot = alice.seal_snapshot(b"snapshot").unwrap();
        assert_eq!(bob.open_app(&snapshot).unwrap_err().code, "RMT-E4005");
        assert_eq!(bob.open_snapshot(&snapshot).unwrap(), b"snapshot");
        let frame = alice.seal_app(AppKind::Intent, b"intent").unwrap();
        assert_eq!(frame[13], AppKind::Intent as u8);
        let (kind, plaintext) = bob.open_app(&frame).unwrap();
        assert_eq!(kind, AppKind::Intent);
        assert_eq!(plaintext, b"intent");
    }

    #[test]
    fn frame_is_bound_to_the_peer_pair() {
        let (mut alice, _bob, manifest) = linked();
        let hello = alice.seal_hello(&manifest).unwrap();
        let mut other =
            Channel::establish(id_from_byte(3), id_from_byte(1), &accepted(&code())).unwrap();
        assert_eq!(
            other.open_hello(&manifest, &hello).unwrap_err().code,
            "RMT-E4005"
        );
    }
}
