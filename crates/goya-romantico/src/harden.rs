//! Deterministic decoder probe.
//!
//! This is not a coverage-guided fuzzer. Each corpus document is mutated
//! inside a fixed window. Every decoder returns a structured error or `Ok`.
//! A rejected payload does not move a live replica.

use crate::authority::{decode_checkpoint, encode_checkpoint, HandoffRecord};
use crate::canonical::{decode, encode, Value, MAX_BLOB, MAX_BYTES, MAX_DEPTH};
use crate::channel::Channel;
use crate::contract::{decode_contract, pairs_continuity_contract};
use crate::diagnostic::Diagnostic;
use crate::durable::Durable;
use crate::id_from_byte;
use crate::mutation::{decode_envelope, encode_envelope};
use crate::pairing::Pairing;
use crate::pairs::{open, select, state_ok, SCHEMA};
use crate::surface::{decode_capabilities, encode_capabilities, DeviceCaps};
use crate::Error;
use std::collections::BTreeMap;

fn mutants(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![Vec::new(), vec![0], vec![0xff]];
    let window = bytes.len().min(24);
    for index in 0..window {
        let mut copy = bytes.to_vec();
        copy[index] ^= 0xff;
        out.push(copy);
    }
    if bytes.len() > 24 {
        let mut copy = bytes.to_vec();
        let last = copy.len() - 1;
        copy[last] ^= 0xff;
        out.push(copy);
    }
    out.push(bytes.iter().take(4).copied().collect());
    let mut trailed = bytes.to_vec();
    trailed.push(0);
    out.push(trailed);
    out
}

fn expect_structured<T>(result: Result<T, Error>) {
    if let Err(error) = result {
        assert_eq!(error.code.len(), 9);
        assert!(error.code.starts_with("RMT-E"));
        assert!(!error.detail.is_empty());
    }
}

fn corpus() -> Vec<Vec<u8>> {
    let deck = [0u32, 1, 2, 3, 3, 2, 1, 0];
    let mut origin = open(id_from_byte(1), id_from_byte(9), &deck).unwrap();
    let snapshot = origin.encode_snapshot().unwrap();
    let envelope = select(&mut origin, 0).unwrap();
    let mutation = encode_envelope(&envelope).unwrap();
    let contract = pairs_continuity_contract().unwrap();
    let caps = encode_capabilities(&DeviceCaps {
        display: false,
        interactive_input: true,
        touch: false,
        large_display: false,
    })
    .unwrap();
    let checkpoint = encode_checkpoint(
        &snapshot,
        &HandoffRecord::Prepared {
            from: id_from_byte(1),
            to: id_from_byte(2),
            epoch: 1,
            revision: 0,
        },
    )
    .unwrap();
    let mut journal = Durable::open(&snapshot).unwrap();
    journal.append(&mutation).unwrap();
    let bundled = journal.encode().unwrap();
    let boolean = encode(&Value::Bool(true)).unwrap();
    let mut fields = BTreeMap::new();
    fields.insert(1, Value::Int(1));
    let record = encode(&Value::Record(fields)).unwrap();
    vec![
        boolean, record, snapshot, mutation, contract, caps, checkpoint, bundled,
    ]
}

#[test]
fn decoder_probe_returns_a_structured_error_or_ok() {
    for document in corpus() {
        for sample in mutants(&document) {
            expect_structured(decode(&sample));
            expect_structured(decode_envelope(&sample));
            expect_structured(decode_checkpoint(&sample));
            expect_structured(decode_capabilities(&sample));
            expect_structured(decode_contract(&sample));
            expect_structured(Durable::decode(&sample));
            expect_structured(Durable::open(&sample));
        }
    }
}

#[test]
fn limits_fail_closed() {
    let mut record = Vec::from(&b"RCB1"[..]);
    record.push(8);
    record.extend_from_slice(&4097u32.to_le_bytes());
    assert_eq!(decode(&record).unwrap_err().code, "RMT-E8001");

    let mut blob = Vec::from(&b"RCB1"[..]);
    blob.push(11);
    blob.extend_from_slice(&u32::try_from(MAX_BLOB + 1).unwrap().to_le_bytes());
    assert_eq!(decode(&blob).unwrap_err().code, "RMT-E8001");

    let huge = vec![0u8; MAX_BYTES + 1];
    let decoded = decode(&huge).unwrap_err();
    assert_eq!(decoded.code.len(), 9);
    assert!(decoded.code.starts_with("RMT-E"));
    assert!(!decoded.detail.is_empty());
    assert_eq!(Durable::decode(&huge).unwrap_err().code, "RMT-E8001");

    let mut nested = Value::Bool(true);
    for _ in 0..MAX_DEPTH {
        nested = Value::Array(vec![nested]);
    }
    assert_eq!(encode(&nested).unwrap_err().code, "RMT-E2001");
}

#[test]
fn garbage_after_hello_does_not_move_the_counter() {
    let manifest = SCHEMA.manifest().unwrap();
    let secret = *b"pairing-code-16b";
    let mut pairing = Pairing::offer(&secret, 0, 30).unwrap();
    pairing.accept(&secret, true, 1).unwrap();
    let mut alice = Channel::establish(id_from_byte(1), id_from_byte(2), &pairing).unwrap();
    let mut bob = Channel::establish(id_from_byte(2), id_from_byte(1), &pairing).unwrap();
    let hello = alice.seal_hello(&manifest).unwrap();
    let reply = bob.seal_hello(&manifest).unwrap();
    alice.open_hello(&manifest, &reply).unwrap();
    bob.open_hello(&manifest, &hello).unwrap();
    let before = format!("{alice:?}");
    let error = alice.open_app(&[0u8; 8]).unwrap_err();
    assert_eq!(error.code, "RMT-E4005");
    assert!(!error.detail.is_empty());
    assert_eq!(format!("{alice:?}"), before);
}

#[test]
fn garbage_does_not_move_a_replica() {
    let mut origin = open(id_from_byte(1), id_from_byte(9), &[0, 1, 2, 3, 3, 2, 1, 0]).unwrap();
    let snapshot = origin.encode_snapshot().unwrap();
    let revision = origin.revision();
    let before = origin.diagnostics();
    let error = origin.apply_remote(&[0u8; 8], state_ok).unwrap_err();
    assert_eq!(error.code.len(), 9);
    assert!(error.code.starts_with("RMT-E"));
    assert!(!error.detail.is_empty());
    assert_eq!(origin.revision(), revision);
    assert_eq!(origin.encode_snapshot().unwrap(), snapshot);
    let after = origin.diagnostics();
    assert_eq!(after.len(), before.len() + 1);
    assert!(matches!(
        after.last().copied(),
        Some(Diagnostic::MutationRejected { code }) if code.len() == 9 && code.starts_with("RMT-E")
    ));
}

#[test]
fn torn_journal_does_not_grow_the_trace() {
    let mut origin = open(id_from_byte(1), id_from_byte(9), &[0, 1, 2, 3, 3, 2, 1, 0]).unwrap();
    let snapshot = origin.encode_snapshot().unwrap();
    let before = origin.diagnostics();
    let error = Durable::reload(&mut origin, b"RJN1", state_ok).unwrap_err();
    assert_eq!(error.code, "RMT-E6001");
    assert!(!error.detail.is_empty());
    assert_eq!(origin.diagnostics(), before);
    assert_eq!(origin.encode_snapshot().unwrap(), snapshot);
}
