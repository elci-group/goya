//! Live delivery of one session across two channels.
//!
//! The follower seals an intent. The authority commits or rejects it. A
//! committed intent becomes an `RMU1` frame; the follower applies that frame
//! and seals an acknowledgement. A rejection is acknowledged too, so the
//! sequence counters stay paired. The follower's pending queue is not the
//! commit. This module does not open a socket.

use crate::canonical::{decode, encode, Value};
use crate::channel::{AppKind, Channel};
use crate::mutation::{decode_envelope, encode_envelope, Apply, Envelope};
use crate::session::{Intent, Replica};
use crate::Error;
use std::collections::BTreeMap;

const OUTCOME_APPLIED: u8 = 1;
const OUTCOME_DUPLICATE: u8 = 2;
const OUTCOME_REJECTED: u8 = 3;
const OUTCOME_STALE: u8 = 4;
const MAX_CODE: usize = 32;

struct ExpectedAck<'a> {
    id: [u8; 16],
    outcome: u8,
    revision: u64,
    code: &'a str,
}

struct Ack {
    id: [u8; 16],
    outcome: u8,
    revision: u64,
    code: String,
}

/// Seal `bytes` as a mutation, apply them on `replica`, and acknowledge.
///
/// `Applied`, `Duplicate`, and `Stale` acknowledge that outcome. An apply
/// error still produces a rejected acknowledgement, then returns the error.
/// The replica is not rolled back.
pub fn deliver_mutation<F>(
    sender: &mut Channel,
    receiver: &mut Channel,
    replica: &mut Replica,
    bytes: &[u8],
    invariant: F,
) -> Result<Apply, Error>
where
    F: FnOnce(&BTreeMap<u32, Value>) -> bool,
{
    let frame = sender.seal_app(AppKind::Mutation, bytes)?;
    let (kind, opened) = receiver.open_app(&frame)?;
    if kind != AppKind::Mutation || opened.as_slice() != bytes {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    let id = decode_envelope(bytes)
        .map(|envelope| envelope.mutation)
        .unwrap_or([0u8; 16]);
    let attempt = replica.apply_remote(&opened, invariant);
    let (outcome, code) = match &attempt {
        Ok(Apply::Applied) => (OUTCOME_APPLIED, ""),
        Ok(Apply::Duplicate) => (OUTCOME_DUPLICATE, ""),
        Ok(Apply::Stale) => (OUTCOME_STALE, ""),
        Err(error) => (OUTCOME_REJECTED, error.code),
    };
    exchange_ack(
        receiver,
        sender,
        ExpectedAck {
            id,
            outcome,
            revision: replica.revision(),
            code,
        },
    )?;
    attempt
}

/// Carry one follower intent to the authority and return the follower's apply result.
///
/// `execute` commits on the authority. This function does not clear the
/// follower's pending queue. An origin that is not the follower channel's
/// local id and the authority channel's peer is `RMT-E4006` and is not executed.
pub fn deliver_intent<E, F>(
    follower_channel: &mut Channel,
    authority_channel: &mut Channel,
    authority: &mut Replica,
    follower: &mut Replica,
    intent: Intent,
    execute: E,
    invariant: F,
) -> Result<Apply, Error>
where
    E: FnOnce(&mut Replica, Intent) -> Result<Envelope, Error>,
    F: FnOnce(&BTreeMap<u32, Value>) -> bool,
{
    let plaintext = encode_intent(&intent)?;
    let frame = follower_channel.seal_app(AppKind::Intent, &plaintext)?;
    let (kind, opened) = authority_channel.open_app(&frame)?;
    if kind != AppKind::Intent {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    let decoded = decode_intent(&opened)?;
    if decoded != intent {
        return Err(Error::new("RMT-E2001", "intent is not canonical"));
    }
    if intent.origin != follower_channel.local_id() || intent.origin != authority_channel.peer_id()
    {
        reject(
            authority_channel,
            follower_channel,
            follower,
            [0u8; 16],
            "RMT-E4006",
        )?;
        return Err(Error::new(
            "RMT-E4006",
            "intent origin is not the paired follower",
        ));
    }
    let envelope = match execute(authority, intent) {
        Ok(envelope) => envelope,
        Err(error) => {
            reject(
                authority_channel,
                follower_channel,
                follower,
                [0u8; 16],
                error.code,
            )?;
            return Err(error);
        }
    };
    let bytes = encode_envelope(&envelope)?;
    deliver_mutation(
        authority_channel,
        follower_channel,
        follower,
        &bytes,
        invariant,
    )
}

pub(crate) fn encode_intent(intent: &Intent) -> Result<Vec<u8>, Error> {
    let argument = i64::try_from(intent.argument)
        .map_err(|_| Error::new("RMT-E2001", "intent is not canonical"))?;
    let mut record = BTreeMap::new();
    record.insert(1, Value::Enum(intent.action));
    record.insert(2, Value::Int(argument));
    record.insert(3, Value::Bytes(intent.origin.to_vec()));
    encode(&Value::Record(record))
}

fn decode_intent(bytes: &[u8]) -> Result<Intent, Error> {
    let value = canonical_record(bytes, 3, "intent is not canonical")?;
    let action = match value.get(&1) {
        Some(Value::Enum(action)) => *action,
        _ => return Err(Error::new("RMT-E2001", "intent is not canonical")),
    };
    let argument = match value.get(&2) {
        Some(Value::Int(argument)) if *argument >= 0 => u64::try_from(*argument)
            .map_err(|_| Error::new("RMT-E2001", "intent is not canonical"))?,
        _ => return Err(Error::new("RMT-E2001", "intent is not canonical")),
    };
    let origin = match value.get(&3) {
        Some(Value::Bytes(origin)) if origin.len() == 16 => {
            let mut id = [0u8; 16];
            id.copy_from_slice(origin);
            id
        }
        _ => return Err(Error::new("RMT-E2001", "intent is not canonical")),
    };
    Ok(Intent {
        action,
        argument,
        origin,
    })
}

fn reject(
    authority_channel: &mut Channel,
    follower_channel: &mut Channel,
    follower: &Replica,
    id: [u8; 16],
    code: &'static str,
) -> Result<(), Error> {
    exchange_ack(
        authority_channel,
        follower_channel,
        ExpectedAck {
            id,
            outcome: OUTCOME_REJECTED,
            revision: follower.revision(),
            code,
        },
    )
}

fn exchange_ack(
    sender: &mut Channel,
    receiver: &mut Channel,
    expected: ExpectedAck<'_>,
) -> Result<(), Error> {
    let plaintext = encode_ack(&expected)?;
    let frame = sender.seal_app(AppKind::Ack, &plaintext)?;
    check_ack(receiver, &frame, expected)
}

fn check_ack(receiver: &mut Channel, frame: &[u8], expected: ExpectedAck<'_>) -> Result<(), Error> {
    let (kind, opened) = receiver.open_app(frame)?;
    if kind != AppKind::Ack {
        return Err(Error::new("RMT-E4005", "transport frame was rejected"));
    }
    let ack = decode_ack(&opened)?;
    if ack.id != expected.id
        || ack.outcome != expected.outcome
        || ack.revision != expected.revision
        || ack.code != expected.code
    {
        return Err(Error::new(
            "RMT-E5002",
            "acknowledgement does not match the mutation",
        ));
    }
    Ok(())
}

fn encode_ack(ack: &ExpectedAck<'_>) -> Result<Vec<u8>, Error> {
    if ack.code.len() > MAX_CODE || !code_matches(ack.outcome, ack.code) {
        return Err(Error::new("RMT-E2001", "acknowledgement is not canonical"));
    }
    let revision = i64::try_from(ack.revision)
        .map_err(|_| Error::new("RMT-E8001", "acknowledgement revision does not fit"))?;
    let mut record = BTreeMap::new();
    record.insert(1, Value::Bytes(ack.id.to_vec()));
    record.insert(2, Value::Enum(u32::from(ack.outcome)));
    record.insert(3, Value::Int(revision));
    record.insert(4, Value::Text(ack.code.to_string()));
    encode(&Value::Record(record))
}

fn decode_ack(bytes: &[u8]) -> Result<Ack, Error> {
    let value = canonical_record(bytes, 4, "acknowledgement is not canonical")?;
    let id = match value.get(&1) {
        Some(Value::Bytes(id)) if id.len() == 16 => {
            let mut out = [0u8; 16];
            out.copy_from_slice(id);
            out
        }
        _ => return Err(Error::new("RMT-E2001", "acknowledgement is not canonical")),
    };
    let outcome = match value.get(&2) {
        Some(Value::Enum(outcome)) => u8::try_from(*outcome)
            .ok()
            .filter(|outcome| matches!(outcome, 1..=4))
            .ok_or(Error::new("RMT-E2001", "acknowledgement is not canonical"))?,
        _ => return Err(Error::new("RMT-E2001", "acknowledgement is not canonical")),
    };
    let revision = match value.get(&3) {
        Some(Value::Int(revision)) if *revision >= 0 => u64::try_from(*revision)
            .map_err(|_| Error::new("RMT-E2001", "acknowledgement is not canonical"))?,
        _ => return Err(Error::new("RMT-E2001", "acknowledgement is not canonical")),
    };
    let code = match value.get(&4) {
        Some(Value::Text(code)) if code.len() <= MAX_CODE && code_matches(outcome, code) => {
            code.clone()
        }
        _ => return Err(Error::new("RMT-E2001", "acknowledgement is not canonical")),
    };
    Ok(Ack {
        id,
        outcome,
        revision,
        code,
    })
}

fn code_matches(outcome: u8, code: &str) -> bool {
    if outcome == OUTCOME_REJECTED {
        !code.is_empty()
    } else {
        code.is_empty()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::Value;
    use crate::id_from_byte;
    use crate::mutation::{encode_envelope, Operation};
    use crate::pairs::{
        completion_permille, enqueue_select, execute_intent, join, open, select, state_ok, F_SCORE,
        F_SELECTED, SCHEMA,
    };
    use crate::session::Intent;
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

    fn run_intent(
        follower_channel: &mut Channel,
        authority_channel: &mut Channel,
        authority: &mut Replica,
        follower: &mut Replica,
        intent: Intent,
    ) -> Result<Apply, Error> {
        deliver_intent(
            follower_channel,
            authority_channel,
            authority,
            follower,
            intent,
            execute_intent,
            state_ok,
        )
    }

    #[test]
    fn pairs_follows_one_session_on_the_channel() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let before = follower.fingerprint().unwrap();
        enqueue_select(&mut follower, 0).unwrap();
        assert_eq!(follower.revision(), 0);
        assert_eq!(follower.pending_len(), 1);
        assert_eq!(follower.fingerprint().unwrap(), before);
        let intent = Intent {
            action: 1,
            argument: 0,
            origin: phone(),
        };
        assert_eq!(
            run_intent(
                &mut follower_channel,
                &mut authority_channel,
                &mut authority,
                &mut follower,
                intent
            )
            .unwrap(),
            Apply::Applied
        );
        assert_eq!(follower.pending_len(), 1);
        assert!(follower.acknowledge_pending(1, 0));
        assert_eq!(follower.pending_len(), 0);
        assert_eq!(authority.revision(), 1);
        assert_eq!(follower.revision(), 1);
        assert_eq!(
            follower.values().get(&F_SELECTED),
            Some(&Value::Array(vec![Value::Int(0)]))
        );
        let envelope = select(&mut authority, 7).unwrap();
        let bytes = encode_envelope(&envelope).unwrap();
        assert_eq!(
            deliver_mutation(
                &mut authority_channel,
                &mut follower_channel,
                &mut follower,
                &bytes,
                state_ok
            )
            .unwrap(),
            Apply::Applied
        );
        assert_eq!(authority.revision(), 2);
        assert_eq!(follower.revision(), 2);
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(1)));
        assert_eq!(completion_permille(&follower).unwrap(), 250);
        assert_eq!(
            authority.fingerprint().unwrap(),
            follower.fingerprint().unwrap()
        );
    }

    #[test]
    fn duplicate_delivery_does_not_score_again() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let first = encode_envelope(&select(&mut authority, 0).unwrap()).unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &first,
            state_ok,
        )
        .unwrap();
        assert_eq!(
            deliver_mutation(
                &mut authority_channel,
                &mut follower_channel,
                &mut follower,
                &first,
                state_ok
            )
            .unwrap(),
            Apply::Duplicate
        );
        assert_eq!(follower.revision(), 1);
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(0)));
    }

    #[test]
    fn gapped_delivery_waits_for_the_missing_revision() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let first = encode_envelope(&select(&mut authority, 0).unwrap()).unwrap();
        let second = encode_envelope(&select(&mut authority, 7).unwrap()).unwrap();
        let error = deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &second,
            state_ok,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E5003");
        assert_eq!(follower.revision(), 0);
        assert_eq!(authority.revision(), 2);
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(0)));
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &first,
            state_ok,
        )
        .unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &second,
            state_ok,
        )
        .unwrap();
        assert_eq!(
            authority.fingerprint().unwrap(),
            follower.fingerprint().unwrap()
        );
        assert_eq!(completion_permille(&follower).unwrap(), 250);
    }

    #[test]
    fn illegal_intent_leaves_both_replicas_unchanged() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let first = encode_envelope(&select(&mut authority, 0).unwrap()).unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &first,
            state_ok,
        )
        .unwrap();
        let fingerprint = follower.fingerprint().unwrap();
        let intent = Intent {
            action: 1,
            argument: 0,
            origin: phone(),
        };
        let error = run_intent(
            &mut follower_channel,
            &mut authority_channel,
            &mut authority,
            &mut follower,
            intent,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E5004");
        assert_eq!(authority.revision(), 1);
        assert_eq!(follower.revision(), 1);
        assert_eq!(follower.fingerprint().unwrap(), fingerprint);
    }

    #[test]
    fn follower_cannot_commit_or_impersonate_the_authority() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let error = follower
            .commit_authority(
                vec![Operation::Set {
                    field: F_SCORE,
                    value: Value::Int(1),
                }],
                state_ok,
            )
            .unwrap_err();
        assert_eq!(error.code, "RMT-E5005");
        let first = select(&mut authority, 0).unwrap();
        let bytes = encode_envelope(&first).unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &bytes,
            state_ok,
        )
        .unwrap();
        let outbound = authority_channel
            .seal_app(AppKind::Mutation, &bytes)
            .unwrap();
        assert_eq!(&outbound[14..30], tv().as_slice());
        assert_eq!(
            authority_channel.open_app(&outbound).unwrap_err().code,
            "RMT-E4005"
        );
        let copied = follower_channel
            .seal_app(AppKind::Mutation, &bytes)
            .unwrap();
        assert_eq!(&copied[14..30], phone().as_slice());
        let (kind, opened) = authority_channel.open_app(&copied).unwrap();
        assert_eq!(kind, AppKind::Mutation);
        assert_eq!(
            authority.apply_remote(&opened, state_ok).unwrap(),
            Apply::Duplicate
        );
        assert_eq!(authority.revision(), 1);
    }

    #[test]
    fn a_phone_sealed_copy_is_a_duplicate_on_the_authority() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let first = encode_envelope(&select(&mut authority, 0).unwrap()).unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &first,
            state_ok,
        )
        .unwrap();
        assert_eq!(
            deliver_mutation(
                &mut follower_channel,
                &mut authority_channel,
                &mut authority,
                &first,
                state_ok
            )
            .unwrap(),
            Apply::Duplicate
        );
        assert_eq!(authority.revision(), 1);
        assert_eq!(authority.values().get(&F_SCORE), Some(&Value::Int(0)));
    }

    #[test]
    fn spoofed_intent_origin_is_refused() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let fingerprint = authority.fingerprint().unwrap();
        let intent = Intent {
            action: 1,
            argument: 0,
            origin: tv(),
        };
        let error = run_intent(
            &mut follower_channel,
            &mut authority_channel,
            &mut authority,
            &mut follower,
            intent,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E4006");
        assert_eq!(authority.revision(), 0);
        assert_eq!(follower.revision(), 0);
        assert_eq!(authority.fingerprint().unwrap(), fingerprint);
        assert_eq!(follower.fingerprint().unwrap(), fingerprint);
    }

    #[test]
    fn acknowledgement_must_name_the_mutation() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let plaintext = encode_ack(&ExpectedAck {
            id: [0u8; 16],
            outcome: OUTCOME_APPLIED,
            revision: 0,
            code: "",
        })
        .unwrap();
        let frame = follower_channel.seal_app(AppKind::Ack, &plaintext).unwrap();
        let error = check_ack(
            &mut authority_channel,
            &frame,
            ExpectedAck {
                id: [9u8; 16],
                outcome: OUTCOME_APPLIED,
                revision: 0,
                code: "",
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E5002");
        assert_eq!(error.detail, "acknowledgement does not match the mutation");
    }

    #[test]
    fn stale_delivery_does_not_rewind() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let first = encode_envelope(&select(&mut authority, 0).unwrap()).unwrap();
        let second = encode_envelope(&select(&mut authority, 7).unwrap()).unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &first,
            state_ok,
        )
        .unwrap();
        deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            &second,
            state_ok,
        )
        .unwrap();
        follower
            .install_snapshot(&authority.encode_snapshot().unwrap())
            .unwrap();
        assert_eq!(
            deliver_mutation(
                &mut authority_channel,
                &mut follower_channel,
                &mut follower,
                &first,
                state_ok
            )
            .unwrap(),
            Apply::Stale
        );
        assert_eq!(follower.revision(), 2);
        assert_eq!(follower.values().get(&F_SCORE), Some(&Value::Int(1)));
    }

    #[test]
    fn rejected_bytes_keep_the_counters_aligned() {
        let (mut authority_channel, mut follower_channel) = negotiated();
        let (mut authority, mut follower) = replicas();
        let error = deliver_mutation(
            &mut authority_channel,
            &mut follower_channel,
            &mut follower,
            b"nope",
            state_ok,
        )
        .unwrap_err();
        assert_eq!(error.code, "RMT-E2001");
        assert_eq!(follower.revision(), 0);
        let bytes = encode_envelope(&select(&mut authority, 0).unwrap()).unwrap();
        assert_eq!(
            deliver_mutation(
                &mut authority_channel,
                &mut follower_channel,
                &mut follower,
                &bytes,
                state_ok
            )
            .unwrap(),
            Apply::Applied
        );
        assert_eq!(
            authority.fingerprint().unwrap(),
            follower.fingerprint().unwrap()
        );
    }

    #[test]
    fn acknowledgement_codes_are_closed() {
        let rejected = encode_ack(&ExpectedAck {
            id: [0u8; 16],
            outcome: OUTCOME_REJECTED,
            revision: 0,
            code: "",
        })
        .unwrap_err();
        assert_eq!(rejected.code, "RMT-E2001");
        let applied = encode_ack(&ExpectedAck {
            id: [0u8; 16],
            outcome: OUTCOME_APPLIED,
            revision: 0,
            code: "RMT-E5004",
        })
        .unwrap_err();
        assert_eq!(applied.code, "RMT-E2001");
    }
}
