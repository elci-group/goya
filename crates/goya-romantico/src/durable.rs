//! Caller-held durable journal.
//!
//! `RJN1` is a byte string the caller keeps. It holds one `RSN1` or `RCK1`
//! base and an ordered `RMU1` tail of at most [`MAX_JOURNAL`] entries.
//! Reloading those bytes builds a new replica. A torn or gapped journal is
//! refused and does not change a live replica. This module does not open a
//! file and it does not open a socket. Power loss on a device disk is outside
//! this crate. A pending intent is not part of the journal.

use crate::authority::{admitted_writer, decode_checkpoint, HandoffRecord};
use crate::canonical::{fingerprint_bytes, Value, MAX_BYTES};
use crate::channel::Channel;
use crate::mutation::{decode_envelope, Apply};
use crate::schema::Schema;
use crate::session::{Replica, MAX_JOURNAL};
use crate::surface::Permissions;
use crate::sync::deliver_mutation;
use crate::Error;
use std::collections::BTreeMap;

const CORRUPT: Error = Error::new("RMT-E6001", "durable journal is truncated or corrupt");
const DOES_NOT_EXTEND: Error = Error::new("RMT-E6002", "durable journal does not extend");
const WINDOW_FULL: Error = Error::new("RMT-E6002", "durable journal window is full");
const BUDGET: Error = Error::new("RMT-E8001", "durable journal exceeds the byte budget");
const SESSION: Error = Error::new("RMT-E5002", "mutation belongs to a different session");
const SNAPSHOT_SESSION: Error = Error::new("RMT-E5002", "snapshot belongs to a different session");
const SUSPENDED: Error = Error::new(
    "RMT-E5008",
    "authority record does not match the snapshot; writes stay suspended",
);
const NOT_WRITER: Error = Error::new("RMT-E4006", "journal authority is not the paired writer");

struct SnapHeader {
    session: [u8; 16],
    epoch: u64,
    revision: u64,
    authority: [u8; 16],
}

#[derive(Debug)]
struct Base {
    bytes: Vec<u8>,
    session: [u8; 16],
    epoch: u64,
    revision: u64,
    authority: [u8; 16],
    snapshot: Vec<u8>,
}

#[derive(Debug)]
struct TailEntry {
    base_revision: u64,
    revision: u64,
    bytes: Vec<u8>,
}

/// Ordered journal the caller stores and reloads.
///
/// The base is a snapshot or a checkpoint. The tail is the mutations that
/// extend it. Compaction replaces the base and drops the tail entries that
/// base already covers.
#[derive(Debug)]
pub struct Durable {
    base: Base,
    tail: Vec<TailEntry>,
    tip: u64,
}

impl Durable {
    /// Store one validated `RSN1` or `RCK1` and an empty tail.
    pub fn open(base: &[u8]) -> Result<Self, Error> {
        let base = load_base(base)?;
        let journal = Self {
            tip: base.revision,
            base,
            tail: Vec::new(),
        };
        journal.framed_len()?;
        Ok(journal)
    }

    /// Append one canonical envelope that extends the tip.
    ///
    /// Linkage is the session, the base revision, and the next revision.
    /// The application invariant runs when the journal is applied, not here.
    pub fn append(&mut self, envelope: &[u8]) -> Result<(), Error> {
        let decoded = decode_envelope(envelope)?;
        if decoded.session != self.base.session {
            return Err(SESSION);
        }
        if decoded.base_revision != self.tip || self.tip.checked_add(1) != Some(decoded.revision) {
            return Err(DOES_NOT_EXTEND);
        }
        if self.tail.len() >= MAX_JOURNAL {
            return Err(WINDOW_FULL);
        }
        self.fits(envelope.len())?;
        self.tail.push(TailEntry {
            base_revision: decoded.base_revision,
            revision: decoded.revision,
            bytes: envelope.to_vec(),
        });
        self.tip = decoded.revision;
        Ok(())
    }

    /// Replace the base with `snapshot` when its revision lies on this journal.
    ///
    /// Tail entries at or below that revision are dropped. A foreign session
    /// or a revision outside the journal leaves this log unchanged.
    pub fn compact(&mut self, snapshot: &[u8]) -> Result<(), Error> {
        let header = parse_rsn1(snapshot)?;
        if header.session != self.base.session {
            return Err(SNAPSHOT_SESSION);
        }
        if header.revision < self.base.revision || header.revision > self.tip {
            return Err(DOES_NOT_EXTEND);
        }
        let mut kept = Vec::new();
        for entry in &self.tail {
            if entry.revision <= header.revision {
                continue;
            }
            if kept.is_empty() && entry.base_revision != header.revision {
                return Err(DOES_NOT_EXTEND);
            }
            kept.push(TailEntry {
                base_revision: entry.base_revision,
                revision: entry.revision,
                bytes: entry.bytes.clone(),
            });
        }
        let tip = kept
            .last()
            .map(|entry| entry.revision)
            .unwrap_or(header.revision);
        let next = Self {
            base: Base {
                session: header.session,
                epoch: header.epoch,
                revision: header.revision,
                authority: header.authority,
                snapshot: snapshot.to_vec(),
                bytes: snapshot.to_vec(),
            },
            tail: kept,
            tip,
        };
        next.framed_len()?;
        *self = next;
        Ok(())
    }

    /// Encode the canonical `RJN1` bundle.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        let len = self.framed_len()?;
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(b"RJN1");
        push_u32(&mut out, self.base.bytes.len())?;
        out.extend_from_slice(&self.base.bytes);
        push_u32(&mut out, self.tail.len())?;
        for entry in &self.tail {
            push_u32(&mut out, entry.bytes.len())?;
            out.extend_from_slice(&entry.bytes);
        }
        Ok(out)
    }

    /// Decode one complete `RJN1` bundle. Trailing bytes are corrupt.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(BUDGET);
        }
        if bytes.len() < 12 || &bytes[..4] != b"RJN1" {
            return Err(CORRUPT);
        }
        let mut offset = 4;
        let base_len = read_u32(bytes, &mut offset)? as usize;
        let base_end = offset.checked_add(base_len).ok_or(CORRUPT)?;
        if base_end > bytes.len() {
            return Err(CORRUPT);
        }
        let mut journal = Self::open(&bytes[offset..base_end])?;
        offset = base_end;
        let tail_count = read_u32(bytes, &mut offset)? as usize;
        if tail_count > MAX_JOURNAL {
            return Err(WINDOW_FULL);
        }
        for _ in 0..tail_count {
            let len = read_u32(bytes, &mut offset)? as usize;
            let end = offset.checked_add(len).ok_or(CORRUPT)?;
            if end > bytes.len() {
                return Err(CORRUPT);
            }
            journal.append(&bytes[offset..end])?;
            offset = end;
        }
        if offset != bytes.len() {
            return Err(CORRUPT);
        }
        if journal.encode()?.as_slice() != bytes {
            return Err(CORRUPT);
        }
        Ok(journal)
    }

    /// Build a new replica from the bundle. The tail applies all-or-nothing.
    ///
    /// `me` is the writer only when the base names that device. Local,
    /// ephemeral, private, and pending values are not in the bundle.
    pub fn recover<F>(
        schema: Schema,
        me: [u8; 16],
        bytes: &[u8],
        invariant: F,
    ) -> Result<Replica, Error>
    where
        F: Fn(&BTreeMap<u32, Value>) -> bool,
    {
        let journal = Self::decode(bytes)?;
        let mut replica = journal.materialise(schema, me)?;
        journal.replay(&mut replica, &invariant)?;
        Ok(replica)
    }

    /// Advance `replica` to this journal. On failure `replica` is unchanged.
    ///
    /// A replica behind the base installs the snapshot and then replays the
    /// tail. The same install runs when the revision matches the base but the
    /// epoch or authority does not, which is the post-grant snapshot. A
    /// replica inside the tail replays only the suffix. A replica ahead of
    /// the tip is refused. Installing a snapshot does not change permissions
    /// or the pending queue.
    pub fn catch_up<F>(&self, replica: &mut Replica, invariant: F) -> Result<(), Error>
    where
        F: Fn(&BTreeMap<u32, Value>) -> bool,
    {
        let mut trial = replica.clone();
        self.advance(&mut trial, &invariant)?;
        *replica = trial;
        Ok(())
    }

    /// Decode first. Torn bytes never reach `replica`.
    pub fn reload<F>(replica: &mut Replica, bytes: &[u8], invariant: F) -> Result<(), Error>
    where
        F: Fn(&BTreeMap<u32, Value>) -> bool,
    {
        let journal = Self::decode(bytes)?;
        journal.catch_up(replica, invariant)
    }

    /// Send the missing snapshot and mutations after a local pre-check.
    ///
    /// The pre-check applies the journal to a clone. If that fails, or the
    /// journal authority is not this writer channel, nothing is sealed. At
    /// the tip this is a no-op. The snapshot frame is kind 2 and each
    /// mutation frame is kind 4.
    pub fn reconnect<F>(
        &self,
        writer: &mut Channel,
        follower_channel: &mut Channel,
        follower: &mut Replica,
        invariant: F,
    ) -> Result<(), Error>
    where
        F: Fn(&BTreeMap<u32, Value>) -> bool,
    {
        let mut trial = follower.clone();
        self.advance(&mut trial, &invariant)?;
        if self.base.authority != writer.local_id()
            || self.base.authority != follower_channel.peer_id()
        {
            return Err(NOT_WRITER);
        }
        if self.needs_install(follower) {
            let frame = writer.seal_snapshot(&self.base.snapshot)?;
            let opened = follower_channel.open_snapshot(&frame)?;
            if opened.as_slice() != self.base.snapshot.as_slice() {
                return Err(Error::new("RMT-E4005", "transport frame was rejected"));
            }
            follower.install_snapshot(&opened)?;
        }
        for entry in &self.tail {
            if entry.revision <= follower.revision() {
                continue;
            }
            match deliver_mutation(writer, follower_channel, follower, &entry.bytes, |values| {
                invariant(values)
            })? {
                Apply::Applied | Apply::Duplicate => {}
                Apply::Stale => return Err(DOES_NOT_EXTEND),
            }
        }
        Ok(())
    }

    fn materialise(&self, schema: Schema, me: [u8; 16]) -> Result<Replica, Error> {
        if self.base.bytes.starts_with(b"RCK1") {
            return Replica::restore(schema, me, &self.base.bytes);
        }
        let probe = Replica::join(schema, me, Permissions::reader(), &self.base.snapshot)?;
        let permissions = if probe.authority() == me {
            Permissions::authority()
        } else {
            Permissions::follower()
        };
        Replica::join(schema, me, permissions, &self.base.snapshot)
    }

    fn advance<F>(&self, replica: &mut Replica, invariant: &F) -> Result<(), Error>
    where
        F: Fn(&BTreeMap<u32, Value>) -> bool,
    {
        if replica.session() != self.base.session {
            return Err(SNAPSHOT_SESSION);
        }
        if replica.revision() > self.tip {
            return Err(DOES_NOT_EXTEND);
        }
        if self.needs_install(replica) {
            replica.install_snapshot(&self.base.snapshot)?;
        }
        self.replay(replica, invariant)
    }

    fn needs_install(&self, replica: &Replica) -> bool {
        if replica.revision() < self.base.revision {
            return true;
        }
        replica.revision() == self.base.revision
            && (replica.epoch() != self.base.epoch || replica.authority() != self.base.authority)
    }

    fn replay<F>(&self, replica: &mut Replica, invariant: &F) -> Result<(), Error>
    where
        F: Fn(&BTreeMap<u32, Value>) -> bool,
    {
        for entry in &self.tail {
            if entry.revision <= replica.revision() {
                continue;
            }
            match replica.apply_remote(&entry.bytes, |values| invariant(values))? {
                Apply::Applied | Apply::Duplicate => {}
                Apply::Stale => return Err(DOES_NOT_EXTEND),
            }
        }
        Ok(())
    }

    fn framed_len(&self) -> Result<usize, Error> {
        measure(
            self.base.bytes.len(),
            self.tail.iter().map(|entry| entry.bytes.len()),
        )
    }

    fn fits(&self, extra: usize) -> Result<(), Error> {
        let total = self
            .framed_len()?
            .checked_add(4)
            .and_then(|value| value.checked_add(extra))
            .ok_or(BUDGET)?;
        if total > MAX_BYTES {
            return Err(BUDGET);
        }
        Ok(())
    }
}

fn measure(base_len: usize, tails: impl Iterator<Item = usize>) -> Result<usize, Error> {
    let mut total = 12usize.checked_add(base_len).ok_or(BUDGET)?;
    for len in tails {
        total = total
            .checked_add(4)
            .and_then(|value| value.checked_add(len))
            .ok_or(BUDGET)?;
    }
    if total > MAX_BYTES {
        return Err(BUDGET);
    }
    Ok(total)
}

fn load_base(bytes: &[u8]) -> Result<Base, Error> {
    if bytes.starts_with(b"RSN1") {
        let header = parse_rsn1(bytes)?;
        let owned = bytes.to_vec();
        return Ok(Base {
            session: header.session,
            epoch: header.epoch,
            revision: header.revision,
            authority: header.authority,
            snapshot: owned.clone(),
            bytes: owned,
        });
    }
    if bytes.starts_with(b"RCK1") {
        let (snapshot, record) = decode_checkpoint(bytes)?;
        let header = agree(&snapshot, &record)?;
        return Ok(Base {
            session: header.session,
            epoch: header.epoch,
            revision: header.revision,
            authority: header.authority,
            snapshot,
            bytes: bytes.to_vec(),
        });
    }
    Err(CORRUPT)
}

fn agree(snapshot: &[u8], record: &HandoffRecord) -> Result<SnapHeader, Error> {
    let header = parse_rsn1(snapshot)?;
    let (writer, epoch) = admitted_writer(record);
    if header.epoch != epoch || header.authority != writer || header.revision != record.revision() {
        return Err(SUSPENDED);
    }
    if let HandoffRecord::Granted { fingerprint, .. } = *record {
        if fingerprint_bytes(&snapshot[92..]) != fingerprint {
            return Err(SUSPENDED);
        }
    }
    Ok(header)
}

fn parse_rsn1(bytes: &[u8]) -> Result<SnapHeader, Error> {
    if bytes.len() < 92 || &bytes[..4] != b"RSN1" {
        return Err(CORRUPT);
    }
    let mut offset = 4;
    let session = read_array::<16>(bytes, &mut offset)?;
    let _version = read_u32(bytes, &mut offset)?;
    let _fingerprint = read_array::<32>(bytes, &mut offset)?;
    let epoch = read_u64(bytes, &mut offset)?;
    let revision = read_u64(bytes, &mut offset)?;
    let authority = read_array::<16>(bytes, &mut offset)?;
    let len = read_u32(bytes, &mut offset)? as usize;
    if epoch == 0 || offset.checked_add(len) != Some(bytes.len()) {
        return Err(CORRUPT);
    }
    Ok(SnapHeader {
        session,
        epoch,
        revision,
        authority,
    })
}

fn push_u32(out: &mut Vec<u8>, value: usize) -> Result<(), Error> {
    let value = u32::try_from(value).map_err(|_| BUDGET)?;
    out.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(read_array::<4>(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, Error> {
    Ok(u64::from_le_bytes(read_array::<8>(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: &mut usize) -> Result<[u8; N], Error> {
    let end = offset.checked_add(N).ok_or(CORRUPT)?;
    if end > bytes.len() {
        return Err(CORRUPT);
    }
    let mut raw = [0u8; N];
    raw.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::Value;
    use crate::channel::Channel;
    use crate::id_from_byte;
    use crate::mutation::{encode_envelope, Operation};
    use crate::pairs::{
        completion_permille, enqueue_select, join, open, select, state_ok, F_MOVES, F_SCORE,
        F_SECRET, F_VIEWPORT, SCHEMA,
    };
    use crate::session::Replica;
    use crate::HandoffRecord;

    fn deck() -> [u32; 8] {
        [0, 1, 2, 3, 3, 2, 1, 0]
    }

    fn tv() -> [u8; 16] {
        id_from_byte(1)
    }

    fn phone() -> [u8; 16] {
        id_from_byte(2)
    }

    fn session() -> [u8; 16] {
        id_from_byte(9)
    }

    fn started() -> Replica {
        open(tv(), session(), &deck()).unwrap()
    }

    fn accepted() -> crate::Pairing {
        let mut pairing = crate::Pairing::offer(b"pairing-code-16b", 0, 30).unwrap();
        pairing.accept(b"pairing-code-16b", true, 1).unwrap();
        pairing
    }

    fn negotiated() -> (Channel, Channel) {
        let manifest = SCHEMA.manifest().unwrap();
        let mut origin = Channel::establish(tv(), phone(), &accepted()).unwrap();
        let mut follower = Channel::establish(phone(), tv(), &accepted()).unwrap();
        let hello = origin.seal_hello(&manifest).unwrap();
        let reply = follower.seal_hello(&manifest).unwrap();
        origin.open_hello(&manifest, &reply).unwrap();
        follower.open_hello(&manifest, &hello).unwrap();
        (origin, follower)
    }

    fn send_next(channel: &Channel) -> u64 {
        let text = format!("{channel:?}");
        let marker = "send_next: ";
        let start = text.find(marker).expect("send counter") + marker.len();
        let end = start + text[start..].find(',').expect("send comma");
        text[start..end].parse().expect("send number")
    }

    fn bump(origin: &mut Replica) -> Vec<u8> {
        let moves = match origin.values().get(&F_MOVES) {
            Some(Value::Int(moves)) => *moves,
            _ => panic!("moves missing"),
        };
        let envelope = origin
            .commit_authority(
                vec![Operation::Set {
                    field: F_MOVES,
                    value: Value::Int(moves + 1),
                }],
                state_ok,
            )
            .unwrap();
        encode_envelope(&envelope).unwrap()
    }

    #[test]
    fn dropped_replica_reloads_the_same_session() {
        let mut origin = started();
        let idle = origin.encode_snapshot().unwrap();
        origin
            .set_local(F_SECRET, Value::Text("local-secret-marker".into()))
            .unwrap();
        origin.set_local(F_VIEWPORT, Value::Int(1920)).unwrap();
        enqueue_select(&mut origin, 4).unwrap();
        let mutation = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
        let mut log = Durable::open(&idle).unwrap();
        log.append(&mutation).unwrap();
        let bytes = log.encode().unwrap();
        assert_eq!(Durable::decode(&bytes).unwrap().encode().unwrap(), bytes);
        let fingerprint = origin.fingerprint().unwrap();
        drop(origin);
        drop(log);

        let recovered = Durable::recover(SCHEMA, tv(), &bytes, state_ok).unwrap();
        assert!(recovered.is_writer());
        assert_eq!(recovered.revision(), 1);
        assert_eq!(recovered.fingerprint().unwrap(), fingerprint);
        assert_eq!(recovered.pending_len(), 0);
        assert_eq!(recovered.local(F_VIEWPORT), None);
        assert_eq!(recovered.local(F_SECRET), None);
        let spectator = Durable::recover(SCHEMA, phone(), &bytes, state_ok).unwrap();
        assert!(!spectator.is_writer());
        assert_eq!(spectator.fingerprint().unwrap(), fingerprint);
    }

    #[test]
    fn torn_journal_leaves_the_live_replica() {
        let mut origin = started();
        let idle = origin.encode_snapshot().unwrap();
        let mutation = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
        let mut log = Durable::open(&idle).unwrap();
        log.append(&mutation).unwrap();
        let bytes = log.encode().unwrap();
        let mut follower = join(phone(), &idle).unwrap();
        follower.set_local(F_VIEWPORT, Value::Int(1080)).unwrap();
        enqueue_select(&mut follower, 1).unwrap();

        let mut torn = bytes.clone();
        torn.pop();
        assert_eq!(
            Durable::reload(&mut follower, &torn, state_ok)
                .unwrap_err()
                .code,
            "RMT-E6001"
        );
        let mut flipped = bytes.clone();
        let last = flipped.len() - 1;
        flipped[last] ^= 0x01;
        let flipped_error = Durable::decode(&flipped).unwrap_err();
        assert!(flipped_error.code == "RMT-E2001" || flipped_error.code == "RMT-E6001");
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            Durable::reload(&mut follower, &trailing, state_ok)
                .unwrap_err()
                .code,
            "RMT-E6001"
        );
        assert_eq!(Durable::open(b"NOPE").unwrap_err().code, "RMT-E6001");
        assert_eq!(follower.revision(), 0);
        assert_eq!(follower.pending_len(), 1);
        assert_eq!(follower.local(F_VIEWPORT), Some(&Value::Int(1080)));
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(0)));
    }

    #[test]
    fn spliced_tail_does_not_extend() {
        let mut first_origin = started();
        let mut second_origin = started();
        let idle = first_origin.encode_snapshot().unwrap();
        let first = encode_envelope(&select(&mut first_origin, 0).unwrap()).unwrap();
        let second = bump(&mut second_origin);
        let mut log = Durable::open(&idle).unwrap();
        log.append(&first).unwrap();
        let mut bytes = log.encode().unwrap();
        let count_at = 8 + idle.len();
        bytes[count_at..count_at + 4].copy_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&(u32::try_from(second.len()).unwrap()).to_le_bytes());
        bytes.extend_from_slice(&second);
        let error = Durable::decode(&bytes).unwrap_err();
        assert_eq!(error.code, "RMT-E6002");
        assert_eq!(error.detail, "durable journal does not extend");
        let mut follower = join(phone(), &idle).unwrap();
        Durable::reload(&mut follower, &bytes, state_ok).unwrap_err();
        assert_eq!(follower.revision(), 0);
    }

    #[test]
    fn illegal_recovery_leaves_the_live_replica() {
        let mut origin = started();
        let idle = origin.encode_snapshot().unwrap();
        let envelope = origin
            .commit_authority(
                vec![Operation::Set {
                    field: F_SCORE,
                    value: Value::Int(99),
                }],
                |_| true,
            )
            .unwrap();
        let mut log = Durable::open(&idle).unwrap();
        log.append(&encode_envelope(&envelope).unwrap()).unwrap();
        let bytes = log.encode().unwrap();
        assert_eq!(
            Durable::recover(SCHEMA, tv(), &bytes, state_ok)
                .unwrap_err()
                .code,
            "RMT-E5004"
        );
        let mut follower = join(phone(), &idle).unwrap();
        follower.set_local(F_VIEWPORT, Value::Int(1080)).unwrap();
        enqueue_select(&mut follower, 1).unwrap();
        let (mut writer, mut follower_channel) = negotiated();
        let counters = format!("{writer:?}{follower_channel:?}");
        assert_eq!(
            log.reconnect(&mut writer, &mut follower_channel, &mut follower, state_ok)
                .unwrap_err()
                .code,
            "RMT-E5004"
        );
        assert_eq!(format!("{writer:?}{follower_channel:?}"), counters);
        assert_eq!(
            Durable::reload(&mut follower, &bytes, state_ok)
                .unwrap_err()
                .code,
            "RMT-E5004"
        );
        assert_eq!(follower.revision(), 0);
        assert_eq!(follower.pending_len(), 1);
        assert_eq!(follower.local(F_VIEWPORT), Some(&Value::Int(1080)));
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(0)));
    }

    #[test]
    fn compaction_keeps_the_later_tail() {
        let mut origin = started();
        let idle = origin.encode_snapshot().unwrap();
        let first = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
        let at_revision_one = origin.encode_snapshot().unwrap();
        select(&mut origin, 7).unwrap();
        select(&mut origin, 1).unwrap();
        assert_eq!(origin.revision(), 3);
        let mut log = Durable::open(&idle).unwrap();
        for entry in origin.journal_after(0).unwrap() {
            log.append(&entry).unwrap();
        }
        log.compact(&at_revision_one).unwrap();
        let before = log.encode().unwrap();
        let mut foreign = idle.clone();
        foreign[19] = 8;
        assert_eq!(log.compact(&foreign).unwrap_err().code, "RMT-E5002");
        assert_eq!(log.compact(&idle).unwrap_err().code, "RMT-E6002");
        assert_eq!(log.encode().unwrap(), before);

        let recovered = Durable::recover(SCHEMA, tv(), &before, state_ok).unwrap();
        assert_eq!(recovered.revision(), 3);
        assert_eq!(
            recovered.fingerprint().unwrap(),
            origin.fingerprint().unwrap()
        );
        assert_eq!(completion_permille(&recovered).unwrap(), 250);

        let mut behind = join(phone(), &idle).unwrap();
        behind.set_local(F_VIEWPORT, Value::Int(720)).unwrap();
        log.catch_up(&mut behind, state_ok).unwrap();
        assert_eq!(behind.fingerprint().unwrap(), origin.fingerprint().unwrap());
        assert_eq!(behind.local(F_VIEWPORT), Some(&Value::Int(720)));
        assert!(!behind.is_writer());

        let mut inside = join(phone(), &idle).unwrap();
        inside.apply_remote(&first, state_ok).unwrap();
        let (mut writer, mut follower_channel) = negotiated();
        let sent = send_next(&writer);
        log.reconnect(&mut writer, &mut follower_channel, &mut inside, state_ok)
            .unwrap();
        assert_eq!(send_next(&writer), sent + 2);
        assert_eq!(inside.fingerprint().unwrap(), origin.fingerprint().unwrap());
        assert_eq!(inside.revision(), 3);

        let mut older = join(phone(), &idle).unwrap();
        let (mut writer, mut follower_channel) = negotiated();
        let sent = send_next(&writer);
        log.reconnect(&mut writer, &mut follower_channel, &mut older, state_ok)
            .unwrap();
        assert_eq!(send_next(&writer), sent + 3);
        assert_eq!(older.revision(), 3);
        assert!(!older.is_writer());
    }

    #[test]
    fn full_window_compacts_and_accepts_the_next() {
        let mut origin = started();
        let idle = origin.encode_snapshot().unwrap();
        let mut log = Durable::open(&idle).unwrap();
        for _ in 0..MAX_JOURNAL {
            log.append(&bump(&mut origin)).unwrap();
        }
        let tip = origin.encode_snapshot().unwrap();
        assert_eq!(origin.revision(), MAX_JOURNAL as u64);
        let held = log.encode().unwrap();
        let extra = bump(&mut origin);
        let error = log.append(&extra).unwrap_err();
        assert_eq!(error.code, "RMT-E6002");
        assert_eq!(error.detail, "durable journal window is full");
        assert_eq!(log.encode().unwrap(), held);
        log.compact(&tip).unwrap();
        log.append(&extra).unwrap();
        let recovered = Durable::recover(SCHEMA, tv(), &log.encode().unwrap(), state_ok).unwrap();
        assert_eq!(recovered.revision(), origin.revision());
        assert_eq!(
            recovered.fingerprint().unwrap(),
            origin.fingerprint().unwrap()
        );
    }

    #[test]
    fn reconnect_catches_a_follower_up_to_the_match() {
        let mut origin = started();
        let idle = origin.encode_snapshot().unwrap();
        select(&mut origin, 0).unwrap();
        select(&mut origin, 7).unwrap();
        let mut log = Durable::open(&idle).unwrap();
        for entry in origin.journal_after(0).unwrap() {
            log.append(&entry).unwrap();
        }
        let mut follower = join(phone(), &idle).unwrap();
        let (mut writer, mut follower_channel) = negotiated();
        log.reconnect(&mut writer, &mut follower_channel, &mut follower, state_ok)
            .unwrap();
        assert_eq!(follower.revision(), 2);
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(1)));
        assert_eq!(completion_permille(&follower).unwrap(), 250);
        assert_eq!(
            follower.fingerprint().unwrap(),
            origin.fingerprint().unwrap()
        );
        let counters = format!("{writer:?}{follower_channel:?}");
        log.reconnect(&mut writer, &mut follower_channel, &mut follower, state_ok)
            .unwrap();
        assert_eq!(format!("{writer:?}{follower_channel:?}"), counters);

        let mut swapped = join(phone(), &idle).unwrap();
        let (mut writer, mut follower_channel) = negotiated();
        let counters = format!("{writer:?}{follower_channel:?}");
        assert_eq!(
            log.reconnect(&mut follower_channel, &mut writer, &mut swapped, state_ok)
                .unwrap_err()
                .code,
            "RMT-E4006"
        );
        assert_eq!(format!("{writer:?}{follower_channel:?}"), counters);
        assert_eq!(swapped.revision(), 0);

        let mut ahead = origin.clone();
        let idle_only = Durable::open(&idle).unwrap();
        let (mut writer, mut follower_channel) = negotiated();
        let counters = format!("{writer:?}{follower_channel:?}");
        let error = idle_only
            .reconnect(&mut writer, &mut follower_channel, &mut ahead, state_ok)
            .unwrap_err();
        assert_eq!(error.code, "RMT-E6002");
        assert_eq!(error.detail, "durable journal does not extend");
        assert_eq!(format!("{writer:?}{follower_channel:?}"), counters);
        assert_eq!(ahead.revision(), 2);
    }

    #[test]
    fn granted_journal_admits_only_the_successor() {
        let mut origin = started();
        select(&mut origin, 0).unwrap();
        let before = origin.encode_snapshot().unwrap();
        let mut phone_replica = join(phone(), &before).unwrap();
        origin.prepare_handoff(phone()).unwrap();
        let grant = origin.grant(phone()).unwrap();
        let after = origin.encode_snapshot().unwrap();
        let mismatched = Replica::checkpoint(
            &before,
            &HandoffRecord::Granted {
                from: tv(),
                to: phone(),
                epoch: grant.epoch,
                revision: grant.revision,
                fingerprint: grant.fingerprint,
            },
        )
        .unwrap();
        assert_eq!(Durable::open(&mismatched).unwrap_err().code, "RMT-E5008");
        phone_replica.activate(&grant, &after).unwrap();
        phone_replica
            .set_local(F_SECRET, Value::Text("phone-secret".into()))
            .unwrap();
        phone_replica
            .set_local(F_VIEWPORT, Value::Int(1080))
            .unwrap();
        enqueue_select(&mut phone_replica, 2).unwrap();
        let envelope = select(&mut phone_replica, 7).unwrap();
        let checkpoint = Replica::checkpoint(&after, &Replica::granted_record(&grant)).unwrap();
        let mut log = Durable::open(&checkpoint).unwrap();
        log.append(&encode_envelope(&envelope).unwrap()).unwrap();
        let bytes = log.encode().unwrap();
        drop(phone_replica);

        let successor = Durable::recover(SCHEMA, phone(), &bytes, state_ok).unwrap();
        let fenced = Durable::recover(SCHEMA, tv(), &bytes, state_ok).unwrap();
        assert!(successor.is_writer());
        assert!(!fenced.is_writer());
        assert_eq!(successor.revision(), 2);
        assert_eq!(successor.epoch(), 2);
        assert_eq!(successor.pending_len(), 0);
        assert_eq!(successor.local(F_SECRET), None);
        assert_eq!(successor.values().get(&F_SCORE), Some(&Value::Int(1)));
        assert_eq!(completion_permille(&successor).unwrap(), 250);
        assert_eq!(
            successor.fingerprint().unwrap(),
            fenced.fingerprint().unwrap()
        );
        assert_eq!(
            Durable::recover(SCHEMA, id_from_byte(3), &bytes, state_ok)
                .unwrap_err()
                .code,
            "RMT-E4006"
        );

        let mut live = join(phone(), &before).unwrap();
        assert_eq!(live.revision(), 1);
        assert_eq!(live.epoch(), 1);
        log.catch_up(&mut live, state_ok).unwrap();
        assert!(!live.is_writer());
        assert_eq!(live.epoch(), 2);
        assert_eq!(
            live.fingerprint().unwrap(),
            successor.fingerprint().unwrap()
        );
    }
}
