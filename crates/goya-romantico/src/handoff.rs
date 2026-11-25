//! Cooperative handoff of the single writer.
//!
//! Prepare suspends the origin. Grant advances the epoch and clears the
//! origin's mutate right before the successor activates. The post-grant
//! snapshot is a separate frame. Losing that frame leaves nobody able to
//! commit. The same grant can be activated later. Nothing here rolls the
//! epoch backward, elects a second writer, or opens a socket.

use crate::canonical::{decode, encode, Value};
use crate::channel::{AppKind, Channel};
use crate::session::{Grant, Replica};
use crate::Error;
use std::collections::BTreeMap;

/// Tell the successor that commits are suspended, and suspend them.
pub fn deliver_prepare(
    origin_channel: &mut Channel,
    successor_channel: &mut Channel,
    origin: &mut Replica,
    successor: &Replica,
) -> Result<(), Error> {
    paired(origin_channel, successor_channel, origin, successor)?;
    origin.prepare_handoff(successor.me())?;
    let plain = match encode_prepare(origin) {
        Ok(plain) => plain,
        Err(error) => {
            let _ = origin.abort_handoff();
            return Err(error);
        }
    };
    if let Err(error) = send_app(origin_channel, successor_channel, AppKind::Prepare, &plain) {
        let _ = origin.abort_handoff();
        return Err(error);
    }
    if let Err(error) = accept_prepare(&plain, origin, successor) {
        let _ = origin.abort_handoff();
        return Err(error);
    }
    Ok(())
}

/// Fence the origin and deliver the grant. The successor does not activate.
///
/// A failure after the local grant does not restore the origin's mutate right.
pub fn deliver_grant(
    origin_channel: &mut Channel,
    successor_channel: &mut Channel,
    origin: &mut Replica,
    successor: &Replica,
) -> Result<Grant, Error> {
    paired(origin_channel, successor_channel, origin, successor)?;
    let grant = origin.grant(successor.me())?;
    let plain = encode_grant(&grant)?;
    send_app(origin_channel, successor_channel, AppKind::Grant, &plain)?;
    let decoded = decode_grant(&plain)?;
    if decoded != grant {
        return Err(Error::new(
            "RMT-E5007",
            "handoff grant does not match the prepared device",
        ));
    }
    Ok(grant)
}

/// Deliver the post-grant snapshot and activate `successor`.
///
/// A snapshot from before the grant is `RMT-E5008`. The origin stays fenced.
pub fn deliver_activation(
    origin_channel: &mut Channel,
    successor_channel: &mut Channel,
    successor: &mut Replica,
    grant: &Grant,
    snapshot: &[u8],
) -> Result<(), Error> {
    if origin_channel.local_id() != grant.from
        || origin_channel.peer_id() != grant.to
        || successor_channel.local_id() != grant.to
        || successor_channel.peer_id() != grant.from
        || successor.me() != grant.to
        || successor.session() != grant.session
    {
        return Err(Error::new(
            "RMT-E5007",
            "handoff grant names a different device",
        ));
    }
    let frame = origin_channel.seal_snapshot(snapshot)?;
    let opened = successor_channel.open_snapshot(&frame)?;
    if opened.as_slice() != snapshot {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    successor.activate(grant, &opened)
}

/// Cancel a prepare that has not been granted and tell the successor.
pub fn deliver_abort(
    origin_channel: &mut Channel,
    successor_channel: &mut Channel,
    origin: &mut Replica,
    successor: &Replica,
) -> Result<(), Error> {
    paired(origin_channel, successor_channel, origin, successor)?;
    if origin.me() != origin.authority() {
        return Err(Error::new(
            "RMT-E5005",
            "only the active authority can abort a handoff",
        ));
    }
    let prepared = origin.prepared_record()?;
    let crate::HandoffRecord::Prepared { from, to, .. } = prepared else {
        return Err(Error::new("RMT-E5007", "no handoff is in progress"));
    };
    if from != origin.me() || to != successor.me() {
        return Err(Error::new(
            "RMT-E5007",
            "handoff prepare names a different device",
        ));
    }
    let plain = encode_abort(origin.session(), from, to)?;
    origin.abort_handoff()?;
    send_app(origin_channel, successor_channel, AppKind::Abort, &plain)?;
    accept_abort(&plain, origin, successor)
}

pub(crate) fn encode_prepare(origin: &Replica) -> Result<Vec<u8>, Error> {
    let crate::HandoffRecord::Prepared {
        from,
        to,
        epoch,
        revision,
    } = origin.prepared_record()?
    else {
        return Err(Error::new("RMT-E5007", "no handoff is in progress"));
    };
    encode_parties(origin.session(), from, to, Some((epoch, revision)), None)
}

pub(crate) fn encode_grant(grant: &Grant) -> Result<Vec<u8>, Error> {
    encode_parties(
        grant.session,
        grant.from,
        grant.to,
        Some((grant.epoch, grant.revision)),
        Some(grant.fingerprint),
    )
}

fn encode_abort(session: [u8; 16], from: [u8; 16], to: [u8; 16]) -> Result<Vec<u8>, Error> {
    encode_parties(session, from, to, None, None)
}

fn encode_parties(
    session: [u8; 16],
    from: [u8; 16],
    to: [u8; 16],
    counters: Option<(u64, u64)>,
    fingerprint: Option<[u8; 32]>,
) -> Result<Vec<u8>, Error> {
    if from == to || counters.is_some_and(|(epoch, _)| epoch == 0) {
        return Err(Error::new(
            "RMT-E5008",
            "authority record does not name one successor epoch",
        ));
    }
    let mut record = BTreeMap::new();
    record.insert(1, Value::Bytes(session.to_vec()));
    record.insert(2, Value::Bytes(from.to_vec()));
    record.insert(3, Value::Bytes(to.to_vec()));
    if let Some((epoch, revision)) = counters {
        record.insert(4, Value::Int(fit_counter(epoch)?));
        record.insert(5, Value::Int(fit_counter(revision)?));
    }
    if let Some(fingerprint) = fingerprint {
        record.insert(6, Value::Bytes(fingerprint.to_vec()));
    }
    encode(&Value::Record(record))
}

struct WireParties {
    session: [u8; 16],
    from: [u8; 16],
    to: [u8; 16],
}

struct WirePrepare {
    session: [u8; 16],
    from: [u8; 16],
    to: [u8; 16],
    epoch: u64,
    revision: u64,
}

fn accept_prepare(bytes: &[u8], origin: &Replica, successor: &Replica) -> Result<(), Error> {
    let WirePrepare {
        session,
        from,
        to,
        epoch,
        revision,
    } = decode_prepare(bytes)?;
    if session != origin.session() || session != successor.session() {
        return Err(Error::new(
            "RMT-E5002",
            "handoff belongs to a different session",
        ));
    }
    if from != origin.me() || to != successor.me() || from != successor.authority() {
        return Err(Error::new(
            "RMT-E5007",
            "handoff prepare names a different device",
        ));
    }
    if epoch != origin.epoch() || revision != origin.revision() {
        return Err(Error::new(
            "RMT-E5007",
            "handoff prepare does not match the suspended writer",
        ));
    }
    Ok(())
}

fn accept_abort(bytes: &[u8], origin: &Replica, successor: &Replica) -> Result<(), Error> {
    let WireParties { session, from, to } = decode_abort(bytes)?;
    if session != origin.session() || session != successor.session() {
        return Err(Error::new(
            "RMT-E5002",
            "handoff belongs to a different session",
        ));
    }
    if from != origin.me() || to != successor.me() {
        return Err(Error::new(
            "RMT-E5007",
            "handoff prepare names a different device",
        ));
    }
    Ok(())
}

fn decode_grant(bytes: &[u8]) -> Result<Grant, Error> {
    let fields = canonical_record(bytes, 6, "handoff grant is not canonical")?;
    let WireParties { session, from, to } = party_ids(&fields, "handoff grant is not canonical")?;
    let epoch = counter(&fields, 4, "handoff grant is not canonical")?;
    let revision = counter(&fields, 5, "handoff grant is not canonical")?;
    let fingerprint = match fields.get(&6) {
        Some(Value::Bytes(bytes)) if bytes.len() == 32 => {
            let mut out = [0u8; 32];
            out.copy_from_slice(bytes);
            out
        }
        _ => return Err(Error::new("RMT-E2001", "handoff grant is not canonical")),
    };
    if epoch == 0 || from == to {
        return Err(Error::new(
            "RMT-E5008",
            "authority record does not name one successor epoch",
        ));
    }
    Ok(Grant {
        session,
        from,
        to,
        epoch,
        revision,
        fingerprint,
    })
}

fn decode_prepare(bytes: &[u8]) -> Result<WirePrepare, Error> {
    let fields = canonical_record(bytes, 5, "handoff prepare is not canonical")?;
    let WireParties { session, from, to } = party_ids(&fields, "handoff prepare is not canonical")?;
    let epoch = counter(&fields, 4, "handoff prepare is not canonical")?;
    let revision = counter(&fields, 5, "handoff prepare is not canonical")?;
    if epoch == 0 || from == to {
        return Err(Error::new(
            "RMT-E5008",
            "authority record does not name one successor epoch",
        ));
    }
    Ok(WirePrepare {
        session,
        from,
        to,
        epoch,
        revision,
    })
}

fn decode_abort(bytes: &[u8]) -> Result<WireParties, Error> {
    let fields = canonical_record(bytes, 3, "handoff abort is not canonical")?;
    party_ids(&fields, "handoff abort is not canonical")
}

fn party_ids(fields: &BTreeMap<u32, Value>, detail: &'static str) -> Result<WireParties, Error> {
    Ok(WireParties {
        session: id_field(fields, 1, detail)?,
        from: id_field(fields, 2, detail)?,
        to: id_field(fields, 3, detail)?,
    })
}

fn id_field(
    fields: &BTreeMap<u32, Value>,
    id: u32,
    detail: &'static str,
) -> Result<[u8; 16], Error> {
    match fields.get(&id) {
        Some(Value::Bytes(bytes)) if bytes.len() == 16 => {
            let mut out = [0u8; 16];
            out.copy_from_slice(bytes);
            Ok(out)
        }
        _ => Err(Error::new("RMT-E2001", detail)),
    }
}

fn counter(fields: &BTreeMap<u32, Value>, id: u32, detail: &'static str) -> Result<u64, Error> {
    match fields.get(&id) {
        Some(Value::Int(value)) if *value >= 0 => {
            u64::try_from(*value).map_err(|_| Error::new("RMT-E2001", detail))
        }
        _ => Err(Error::new("RMT-E2001", detail)),
    }
}

fn fit_counter(value: u64) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::new("RMT-E8001", "handoff counter does not fit"))
}

fn canonical_record(
    bytes: &[u8],
    count: usize,
    detail: &'static str,
) -> Result<BTreeMap<u32, Value>, Error> {
    let value = decode(bytes)?;
    if encode(&value)? != bytes {
        return Err(Error::new("RMT-E2001", detail));
    }
    match value {
        Value::Record(fields) if fields.len() == count => Ok(fields),
        _ => Err(Error::new("RMT-E2001", detail)),
    }
}

fn paired(
    origin_channel: &Channel,
    successor_channel: &Channel,
    origin: &Replica,
    successor: &Replica,
) -> Result<(), Error> {
    if origin_channel.local_id() != origin.me()
        || successor_channel.local_id() != successor.me()
        || origin_channel.peer_id() != successor.me()
        || successor_channel.peer_id() != origin.me()
        || origin.session() != successor.session()
    {
        return Err(Error::new(
            "RMT-E4006",
            "handoff peers are not the paired devices",
        ));
    }
    Ok(())
}

fn send_app(
    sender: &mut Channel,
    receiver: &mut Channel,
    kind: AppKind,
    plain: &[u8],
) -> Result<(), Error> {
    let frame = sender.seal_app(kind, plain)?;
    let (opened_kind, opened) = receiver.open_app(&frame)?;
    if opened_kind != kind || opened.as_slice() != plain {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::Value;
    use crate::id_from_byte;
    use crate::mutation::encode_envelope;
    use crate::pairs::{completion_permille, join, open, select, state_ok, F_SCORE, SCHEMA};
    use crate::sync::deliver_mutation;
    use crate::Apply;

    fn deck() -> [u32; 8] {
        [0, 1, 2, 3, 3, 2, 1, 0]
    }

    fn tv() -> [u8; 16] {
        id_from_byte(1)
    }

    fn phone() -> [u8; 16] {
        id_from_byte(2)
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

    fn replicas() -> (Replica, Replica) {
        let origin = open(tv(), id_from_byte(9), &deck()).unwrap();
        let follower = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
        (origin, follower)
    }

    #[test]
    fn handoff_moves_the_only_writer() {
        let (mut origin_channel, mut successor_channel) = negotiated();
        let (mut origin, mut successor) = replicas();
        let first = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
        deliver_mutation(
            &mut origin_channel,
            &mut successor_channel,
            &mut successor,
            &first,
            state_ok,
        )
        .unwrap();
        let error = deliver_grant(
            &mut origin_channel,
            &mut successor_channel,
            &mut origin,
            &successor,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E5007");
        assert!(origin.is_writer());
        deliver_prepare(
            &mut origin_channel,
            &mut successor_channel,
            &mut origin,
            &successor,
        )
        .unwrap();
        assert_eq!(select(&mut origin, 7).unwrap_err().code, "RMT-E5006");
        let before = origin.encode_snapshot().unwrap();
        let grant = deliver_grant(
            &mut origin_channel,
            &mut successor_channel,
            &mut origin,
            &successor,
        )
        .unwrap();
        assert_eq!(grant.epoch, 2);
        assert_eq!(grant.from, tv());
        assert_eq!(grant.to, phone());
        assert!(!origin.is_writer());
        assert!(!successor.is_writer());
        assert_eq!(origin.epoch(), 2);
        assert_eq!(successor.epoch(), 1);
        assert_eq!(
            deliver_abort(
                &mut origin_channel,
                &mut successor_channel,
                &mut origin,
                &successor,
            )
            .unwrap_err()
            .code,
            "RMT-E5005"
        );
        assert_eq!(select(&mut origin, 7).unwrap_err().code, "RMT-E5005");
        assert_eq!(select(&mut successor, 7).unwrap_err().code, "RMT-E5005");
        let after = origin.encode_snapshot().unwrap();
        assert_eq!(
            deliver_activation(
                &mut origin_channel,
                &mut successor_channel,
                &mut successor,
                &grant,
                &before,
            )
            .unwrap_err()
            .code,
            "RMT-E5008"
        );
        assert!(!successor.is_writer());
        deliver_activation(
            &mut origin_channel,
            &mut successor_channel,
            &mut successor,
            &grant,
            &after,
        )
        .unwrap();
        assert!(successor.is_writer());
        assert!(!origin.is_writer());
        assert_eq!(successor.epoch(), 2);
        assert_eq!(successor.revision(), 1);
        assert_eq!(
            origin.fingerprint().unwrap(),
            successor.fingerprint().unwrap()
        );
        let checkpoint = Replica::checkpoint(&after, &Replica::granted_record(&grant)).unwrap();
        let restored_phone = Replica::restore(SCHEMA, phone(), &checkpoint).unwrap();
        let restored_tv = Replica::restore(SCHEMA, tv(), &checkpoint).unwrap();
        assert!(restored_phone.is_writer());
        assert!(!restored_tv.is_writer());
        assert_eq!(restored_phone.epoch(), 2);
        let matched = encode_envelope(&select(&mut successor, 7).unwrap()).unwrap();
        deliver_mutation(
            &mut successor_channel,
            &mut origin_channel,
            &mut origin,
            &matched,
            state_ok,
        )
        .unwrap();
        assert_eq!(origin.values().get(&F_SCORE), Some(&Value::Int(1)));
        assert_eq!(completion_permille(&origin).unwrap(), 250);
        assert_eq!(
            origin.fingerprint().unwrap(),
            successor.fingerprint().unwrap()
        );
        assert_eq!(select(&mut origin, 1).unwrap_err().code, "RMT-E5005");
        assert_eq!(
            successor.apply_remote(&first, state_ok).unwrap_err().code,
            "RMT-E5001"
        );
        assert_eq!(successor.revision(), 2);
        assert_eq!(successor.values().get(&F_SCORE), Some(&Value::Int(1)));
    }

    #[test]
    fn abort_restores_the_origin_writer() {
        let (mut origin_channel, mut successor_channel) = negotiated();
        let (mut origin, successor) = replicas();
        let mut phone_replica = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
        let error = deliver_prepare(
            &mut successor_channel,
            &mut origin_channel,
            &mut phone_replica,
            &origin,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E5005");
        deliver_prepare(
            &mut origin_channel,
            &mut successor_channel,
            &mut origin,
            &successor,
        )
        .unwrap();
        deliver_abort(
            &mut origin_channel,
            &mut successor_channel,
            &mut origin,
            &successor,
        )
        .unwrap();
        assert!(origin.is_writer());
        assert_eq!(origin.epoch(), 1);
        let bytes = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
        let mut follower = join(phone(), &replicas().0.encode_snapshot().unwrap()).unwrap();
        deliver_mutation(
            &mut origin_channel,
            &mut successor_channel,
            &mut follower,
            &bytes,
            state_ok,
        )
        .unwrap();
        assert_eq!(follower.revision(), 1);
        assert!(matches!(
            follower.apply_remote(&bytes, state_ok).unwrap(),
            Apply::Duplicate
        ));
    }

    #[test]
    fn stranger_cannot_be_prepared() {
        let (mut origin_channel, mut successor_channel) = negotiated();
        let (mut origin, _successor) = replicas();
        let stranger = join(id_from_byte(3), &origin.encode_snapshot().unwrap()).unwrap();
        let error = deliver_prepare(
            &mut origin_channel,
            &mut successor_channel,
            &mut origin,
            &stranger,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E4006");
        assert!(origin.is_writer());
        assert!(origin.prepared_record().is_err());
    }
}
