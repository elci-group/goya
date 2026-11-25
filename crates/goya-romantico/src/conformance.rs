//! In-process analogues of the P0 continuity checks.
//!
//! These tests share one address space. They do not open a socket, drive
//! webOS, or drive Android. `javascript_encoder_matches_rust_snapshots`
//! compares this crate with an independent Node encoder on the same host.

use super::mutation::decode_envelope;
use super::pairs::{state_ok, F_MOVES, F_SCORE, F_SECRET, F_VIEWPORT};
use super::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

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

#[test]
fn two_replicas_restore_the_same_snapshot() {
    let origin = started();
    let bytes = origin.encode_snapshot().unwrap();
    let joined = join(phone(), &bytes).unwrap();
    assert_eq!(origin.fingerprint().unwrap(), joined.fingerprint().unwrap());
    assert_eq!(joined.revision(), 0);
    assert!(!joined.is_writer());
    assert_eq!(remaining_pairs(&joined).unwrap(), 4);
    assert_eq!(completion_permille(&joined).unwrap(), 0);
}

#[test]
fn duplicate_and_gapped_mutations_do_not_diverge() {
    let mut origin = started();
    let initial = origin.encode_snapshot().unwrap();
    select(&mut origin, 0).unwrap();
    let first = origin.journal_after(0).unwrap();
    let mut other = join(phone(), &initial).unwrap();
    assert!(matches!(
        other.apply_remote(&first[0], state_ok).unwrap(),
        Apply::Applied
    ));
    assert!(matches!(
        other.apply_remote(&first[0], state_ok).unwrap(),
        Apply::Duplicate
    ));
    assert_eq!(other.revision(), 1);
    select(&mut origin, 1).unwrap();
    let second = origin.journal_after(1).unwrap();
    let mut gapped = join(id_from_byte(3), &initial).unwrap();
    let error = gapped.apply_remote(&second[0], state_ok).unwrap_err();
    assert_eq!(error.code, "RMT-E5003");
    assert_eq!(gapped.revision(), 0);
    gapped.apply_remote(&first[0], state_ok).unwrap();
    gapped.apply_remote(&second[0], state_ok).unwrap();
    assert_eq!(origin.fingerprint().unwrap(), gapped.fingerprint().unwrap());
}

#[test]
fn broken_transaction_does_not_publish() {
    let mut origin = started();
    let revision = origin.revision();
    let fingerprint = origin.fingerprint().unwrap();
    let error = origin
        .commit_authority(
            vec![Operation::Set {
                field: F_SCORE,
                value: Value::Int(9),
            }],
            state_ok,
        )
        .unwrap_err();
    assert_eq!(error.code, "RMT-E5004");
    assert_eq!(origin.revision(), revision);
    assert_eq!(origin.fingerprint().unwrap(), fingerprint);
}

#[test]
fn private_and_local_state_stay_out_of_the_snapshot() {
    let mut origin = started();
    origin
        .set_local(F_SECRET, Value::Text("local-secret-marker".into()))
        .unwrap();
    origin.set_local(F_VIEWPORT, Value::Int(1920)).unwrap();
    let bytes = origin.encode_snapshot().unwrap();
    let marker = b"local-secret-marker";
    assert!(!bytes.windows(marker.len()).any(|window| window == marker));
    let error = origin
        .commit_authority(
            vec![Operation::Set {
                field: F_SECRET,
                value: Value::Text("nope".into()),
            }],
            state_ok,
        )
        .unwrap_err();
    assert_eq!(error.code, "RMT-E2003");
    assert_eq!(origin.revision(), 0);
    let mut other = join(phone(), &bytes).unwrap();
    other.set_local(F_VIEWPORT, Value::Int(1080)).unwrap();
    assert_eq!(origin.fingerprint().unwrap(), other.fingerprint().unwrap());
    assert_eq!(other.local(F_VIEWPORT), Some(&Value::Int(1080)));
}

#[test]
fn follower_intent_is_not_committed_until_the_authority_accepts_it() {
    let mut origin = started();
    let mut other = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
    let before = other.fingerprint().unwrap();
    enqueue_select(&mut other, 0).unwrap();
    assert_eq!(other.revision(), 0);
    assert_eq!(other.fingerprint().unwrap(), before);
    assert!(!other.is_writer());
    let envelope = execute_intent(
        &mut origin,
        Intent {
            action: 1,
            argument: 0,
            origin: phone(),
        },
    )
    .unwrap();
    let bytes = encode_envelope(&envelope).unwrap();
    other.apply_remote(&bytes, state_ok).unwrap();
    other.acknowledge_pending(1, 0);
    assert_eq!(other.pending_len(), 0);
    assert_eq!(origin.fingerprint().unwrap(), other.fingerprint().unwrap());
    assert_eq!(remaining_pairs(&origin).unwrap(), 4);
}

#[test]
fn read_only_surface_cannot_submit() {
    let origin = started();
    let mut reader = Replica::join(
        SCHEMA,
        phone(),
        Permissions::reader(),
        &origin.encode_snapshot().unwrap(),
    )
    .unwrap();
    assert_eq!(
        reader
            .enqueue_intent(Intent {
                action: 1,
                argument: 0,
                origin: phone(),
            })
            .unwrap_err()
            .code,
        "RMT-E4006"
    );
}

#[test]
fn cooperative_handoff_moves_the_only_writer() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    select(&mut origin, 7).unwrap();
    let mut other = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
    assert_eq!(completion_permille(&other).unwrap(), 250);
    origin.prepare_handoff(phone()).unwrap();
    let prepared = origin.encode_snapshot().unwrap();
    let grant = origin.grant(phone()).unwrap();
    let granted = origin.encode_snapshot().unwrap();
    assert!(other.activate(&grant, &prepared).is_err());
    assert!(!other.is_writer());
    other.activate(&grant, &granted).unwrap();
    assert!(other.is_writer());
    assert!(!origin.is_writer());
    assert_eq!(
        origin
            .commit_authority(
                vec![Operation::Set {
                    field: F_MOVES,
                    value: Value::Int(2),
                }],
                state_ok,
            )
            .unwrap_err()
            .code,
        "RMT-E5005"
    );
    select(&mut other, 1).unwrap();
    let tail = other.journal_after(origin.revision()).unwrap();
    origin.apply_remote(&tail[0], state_ok).unwrap();
    assert_eq!(origin.fingerprint().unwrap(), other.fingerprint().unwrap());
}

#[test]
fn superseded_epoch_is_rejected() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    let old = origin.journal_after(0).unwrap();
    let mut other = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
    origin.prepare_handoff(phone()).unwrap();
    let grant = origin.grant(phone()).unwrap();
    other
        .activate(&grant, &origin.encode_snapshot().unwrap())
        .unwrap();
    assert_eq!(
        other.apply_remote(&old[0], state_ok).unwrap_err().code,
        "RMT-E5001"
    );
    assert_eq!(other.revision(), 1);
}

#[test]
fn reveal_handoff_completes_once() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    select(&mut origin, 1).unwrap();
    let mut other = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
    origin.prepare_handoff(phone()).unwrap();
    let grant = origin.grant(phone()).unwrap();
    other
        .activate(&grant, &origin.encode_snapshot().unwrap())
        .unwrap();
    let revision = other.revision();
    resolve_reveal(&mut other).unwrap();
    assert_eq!(resolve_reveal(&mut other).unwrap_err().code, "RMT-E5004");
    assert_eq!(other.revision(), revision + 1);
    assert!(resolve_reveal(&mut origin).is_err());
    let tail = other.journal_after(revision).unwrap();
    assert_eq!(tail.len(), 1);
    origin.apply_remote(&tail[0], state_ok).unwrap();
    assert!(matches!(
        origin.apply_remote(&tail[0], state_ok).unwrap(),
        Apply::Duplicate
    ));
    assert_eq!(origin.fingerprint().unwrap(), other.fingerprint().unwrap());
    assert_eq!(remaining_pairs(&origin).unwrap(), 4);
}

#[test]
fn prepared_checkpoint_keeps_a_single_writer() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    origin.prepare_handoff(phone()).unwrap();
    let checkpoint = Replica::checkpoint(
        &origin.encode_snapshot().unwrap(),
        &origin.prepared_record().unwrap(),
    )
    .unwrap();
    let restored_tv = Replica::restore(SCHEMA, tv(), &checkpoint).unwrap();
    let restored_phone = Replica::restore(SCHEMA, phone(), &checkpoint).unwrap();
    assert!(restored_tv.is_writer());
    assert!(!restored_phone.is_writer());
    assert_eq!(restored_tv.epoch(), restored_phone.epoch());
}

#[test]
fn granted_checkpoint_names_the_successor_only() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    origin.prepare_handoff(phone()).unwrap();
    let prepared = origin.encode_snapshot().unwrap();
    let grant = origin.grant(phone()).unwrap();
    let granted = origin.encode_snapshot().unwrap();
    let checkpoint = Replica::checkpoint(&granted, &Replica::granted_record(&grant)).unwrap();
    let restored_tv = Replica::restore(SCHEMA, tv(), &checkpoint).unwrap();
    let restored_phone = Replica::restore(SCHEMA, phone(), &checkpoint).unwrap();
    assert!(!restored_tv.is_writer());
    assert!(restored_phone.is_writer());
    assert!(restored_phone.revision() >= 1);
    let conflict = Replica::checkpoint(&prepared, &Replica::granted_record(&grant)).unwrap();
    assert_eq!(
        Replica::restore(SCHEMA, tv(), &conflict).unwrap_err().code,
        "RMT-E5008"
    );
    assert_eq!(
        Replica::restore(SCHEMA, phone(), &conflict)
            .unwrap_err()
            .code,
        "RMT-E5008"
    );
}

#[test]
fn truncated_checkpoint_does_not_replace_a_live_replica() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    let revision = origin.revision();
    let mut snapshot = origin.encode_snapshot().unwrap();
    snapshot.pop();
    assert_eq!(
        origin.install_snapshot(&snapshot).unwrap_err().code,
        "RMT-E6001"
    );
    assert_eq!(origin.revision(), revision);
    let mut checkpoint = Replica::checkpoint(
        &origin.encode_snapshot().unwrap(),
        &HandoffRecord::Prepared {
            from: tv(),
            to: phone(),
            epoch: origin.epoch(),
            revision: origin.revision(),
        },
    )
    .unwrap();
    checkpoint.pop();
    assert_eq!(
        Replica::restore(SCHEMA, tv(), &checkpoint)
            .unwrap_err()
            .code,
        "RMT-E6001"
    );
    assert_eq!(origin.revision(), revision);
}

#[test]
fn resume_after_restart_keeps_the_deck_and_the_writer() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    select(&mut origin, 7).unwrap();
    let bytes = origin.encode_snapshot().unwrap();
    let resumed = resume(tv(), &bytes).unwrap();
    assert!(resumed.is_writer());
    assert_eq!(
        resumed.fingerprint().unwrap(),
        origin.fingerprint().unwrap()
    );
    assert_eq!(completion_permille(&resumed).unwrap(), 250);
    let spectator = resume(phone(), &bytes).unwrap();
    assert!(!spectator.is_writer());
}

#[test]
fn journal_recovers_a_disconnected_replica_inside_the_window() {
    let mut origin = started();
    let mut other = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
    enqueue_select(&mut other, 4).unwrap();
    select(&mut origin, 0).unwrap();
    select(&mut origin, 7).unwrap();
    assert_ne!(origin.fingerprint().unwrap(), other.fingerprint().unwrap());
    assert_eq!(other.revision(), 0);
    for entry in origin.journal_after(other.revision()).unwrap() {
        other.apply_remote(&entry, state_ok).unwrap();
    }
    assert_eq!(origin.fingerprint().unwrap(), other.fingerprint().unwrap());
    assert_eq!(other.pending_len(), 1);
    assert_eq!(other.revision(), origin.revision());
}

#[test]
fn compacted_journal_refuses_a_replica_outside_the_window() {
    let mut origin = started();
    for _ in 0..MAX_JOURNAL + 1 {
        let moves = match origin.values().get(&F_MOVES) {
            Some(Value::Int(moves)) => *moves,
            _ => panic!("moves missing"),
        };
        origin
            .commit_authority(
                vec![Operation::Set {
                    field: F_MOVES,
                    value: Value::Int(moves + 1),
                }],
                state_ok,
            )
            .unwrap();
    }
    assert_eq!(origin.journal_after(0).unwrap_err().code, "RMT-E6002");
    assert_eq!(origin.journal_after(1).unwrap().len(), MAX_JOURNAL);
}

#[test]
fn incompatible_schema_is_rejected() {
    let manifest = SCHEMA.manifest().unwrap();
    let mut remote = manifest.clone();
    remote.state_schema_version = 2;
    assert_eq!(negotiate(&manifest, &remote).unwrap_err().code, "RMT-E2002");
    let json = SCHEMA.manifest_json().unwrap();
    assert!(json.contains("\"application_id\": \"goya.pairs\""));
    assert!(json.contains("sha256:"));
    let mut origin = started();
    let mut bytes = origin.encode_snapshot().unwrap();
    bytes[24] ^= 0xff;
    assert_eq!(
        origin.install_snapshot(&bytes).unwrap_err().code,
        "RMT-E2002"
    );
    assert_eq!(origin.revision(), 0);
}

#[test]
fn malformed_mutation_is_rejected() {
    assert_eq!(decode_envelope(b"nope").unwrap_err().code, "RMT-E2001");
}

#[test]
fn pending_queue_is_bounded() {
    let origin = started();
    let mut other = join(phone(), &origin.encode_snapshot().unwrap()).unwrap();
    for argument in 0..MAX_PENDING as u64 {
        other
            .enqueue_intent(Intent {
                action: 1,
                argument,
                origin: phone(),
            })
            .unwrap();
    }
    assert_eq!(
        other
            .enqueue_intent(Intent {
                action: 1,
                argument: 99,
                origin: phone(),
            })
            .unwrap_err()
            .code,
        "RMT-E8001"
    );
    assert_eq!(other.pending_len(), MAX_PENDING);
    assert_eq!(other.revision(), 0);
}

#[test]
fn completed_deck_rejects_another_selection() {
    let mut origin = started();
    for (left, right) in [(0u32, 7u32), (1, 6), (2, 5), (3, 4)] {
        select(&mut origin, left).unwrap();
        select(&mut origin, right).unwrap();
    }
    assert_eq!(remaining_pairs(&origin).unwrap(), 0);
    assert_eq!(completion_permille(&origin).unwrap(), 1000);
    assert_eq!(select(&mut origin, 0).unwrap_err().code, "RMT-E5004");
}

fn type_showcase() -> Value {
    let mut map = BTreeMap::new();
    map.insert("b".to_string(), Value::Int(-2));
    map.insert("a".to_string(), Value::Bool(true));
    let mut record = BTreeMap::new();
    record.insert(
        1,
        Value::Array(vec![
            Value::Bool(false),
            Value::Int(i64::MIN),
            Value::Float(OrderedF64::try_new(1.5).unwrap()),
            Value::Text("ñ".to_string()),
            Value::Enum(7),
        ]),
    );
    record.insert(2, Value::Map(map));
    record.insert(3, Value::Optional(Some(Box::new(Value::Ref(9)))));
    record.insert(4, Value::Optional(None));
    record.insert(5, Value::Bytes(vec![1, 2, 3]));
    record.insert(6, Value::TimestampUs(-1));
    Value::Record(record)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn read_hex(path: &Path) -> Vec<u8> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!("read {}: {err}", path.display());
    });
    let text = text.trim();
    assert!(text.len().is_multiple_of(2), "{}", path.display());
    (0..text.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&text[index..index + 2], 16)
                .unwrap_or_else(|_| panic!("hex in {}", path.display()))
        })
        .collect()
}

fn expect_line(lines: &mut std::str::Lines<'_>, label: &str, bytes: &[u8]) {
    let line = lines.next().unwrap_or_else(|| panic!("missing {label}"));
    assert_eq!(line, format!("{label} {}", hex_encode(bytes)), "{label}");
}

#[test]
fn javascript_encoder_matches_rust_snapshots() {
    let origin = started();
    let mut matched = origin.clone();
    select(&mut matched, 0).unwrap();
    select(&mut matched, 7).unwrap();

    let types = encode(&type_showcase()).unwrap();
    let open_body = origin.portable_document().unwrap();
    let open_snap = origin.encode_snapshot().unwrap();
    let matched_body = matched.portable_document().unwrap();
    let matched_snap = matched.encode_snapshot().unwrap();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(types, read_hex(&root.join("fixtures/types.rcb.hex")));
    assert_eq!(
        open_body,
        read_hex(&root.join("fixtures/pairs-open.rcb.hex"))
    );
    assert_eq!(
        open_snap,
        read_hex(&root.join("fixtures/pairs-open.rsn.hex"))
    );
    assert_eq!(
        matched_body,
        read_hex(&root.join("fixtures/pairs-matched.rcb.hex"))
    );
    assert_eq!(
        matched_snap,
        read_hex(&root.join("fixtures/pairs-matched.rsn.hex"))
    );

    let schema_fingerprint = SCHEMA.fingerprint().unwrap();
    assert_eq!(&open_snap[24..56], schema_fingerprint.as_slice());
    assert_ne!(&open_snap[24..56], fingerprint_bytes(&open_body).as_slice());
    assert_eq!(&open_snap[..4], b"RSN1");
    assert_eq!(open_snap[19], 9);
    assert_eq!(open_snap[87], 1);
    assert_eq!(u64::from_le_bytes(open_snap[56..64].try_into().unwrap()), 1);
    assert_eq!(u64::from_le_bytes(open_snap[64..72].try_into().unwrap()), 0);
    assert_eq!(
        u64::from_le_bytes(matched_snap[64..72].try_into().unwrap()),
        2
    );

    let restored = join(phone(), &matched_snap).unwrap();
    assert_eq!(restored.revision(), 2);
    assert_eq!(restored.epoch(), 1);
    assert_eq!(remaining_pairs(&restored).unwrap(), 3);
    assert_eq!(completion_permille(&restored).unwrap(), 250);
    let Value::Record(fields) = decode(&matched_body).unwrap() else {
        panic!("matched body is not a record");
    };
    assert!(!fields.contains_key(&9));
    assert!(!fields.contains_key(&11));
    assert_eq!(fields.get(&5), Some(&Value::Int(1)));

    let output = Command::new("node")
        .arg(root.join("conformance/check.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "types", &types);
    expect_line(&mut lines, "open-body", &open_body);
    expect_line(&mut lines, "open-snap", &open_snap);
    expect_line(&mut lines, "matched-body", &matched_body);
    expect_line(&mut lines, "matched-snap", &matched_snap);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);
}

fn accepted_pairing() -> Pairing {
    let mut pairing = Pairing::offer(b"pairing-code-16b", 0, 30).unwrap();
    pairing.accept(b"pairing-code-16b", true, 1).unwrap();
    pairing
}

#[test]
fn javascript_aead_matches_rust_frames() {
    let manifest = SCHEMA.manifest().unwrap();
    let mut alice = Channel::establish(tv(), phone(), &accepted_pairing()).unwrap();
    let mut bob = Channel::establish(phone(), tv(), &accepted_pairing()).unwrap();
    let hello_plain = super::channel::hello_plaintext(&manifest).unwrap();
    let hello = alice.seal_hello(&manifest).unwrap();
    let reply = bob.seal_hello(&manifest).unwrap();
    alice.open_hello(&manifest, &reply).unwrap();
    bob.open_hello(&manifest, &hello).unwrap();
    let snapshot_plain = started().encode_snapshot().unwrap();
    let snapshot = alice.seal_snapshot(&snapshot_plain).unwrap();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(hello, read_hex(&root.join("fixtures/hello.rtr.hex")));
    assert_eq!(snapshot, read_hex(&root.join("fixtures/snapshot.rtr.hex")));
    assert!(!snapshot
        .windows(snapshot_plain.len())
        .any(|window| window == snapshot_plain));

    let output = Command::new("node")
        .arg(root.join("conformance/transport.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "hello-plain", &hello_plain);
    expect_line(&mut lines, "hello", &hello);
    expect_line(&mut lines, "snapshot-plain", &snapshot_plain);
    expect_line(&mut lines, "snapshot", &snapshot);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);
}

#[test]
fn javascript_mutation_matches_rust() {
    let mut origin = started();
    let mutation = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
    let manifest = SCHEMA.manifest().unwrap();
    let mut alice = Channel::establish(tv(), phone(), &accepted_pairing()).unwrap();
    let mut bob = Channel::establish(phone(), tv(), &accepted_pairing()).unwrap();
    let hello = alice.seal_hello(&manifest).unwrap();
    let reply = bob.seal_hello(&manifest).unwrap();
    alice.open_hello(&manifest, &reply).unwrap();
    bob.open_hello(&manifest, &hello).unwrap();
    let frame = alice.seal_app(AppKind::Mutation, &mutation).unwrap();
    let intent = super::sync::encode_intent(&Intent {
        action: 1,
        argument: 0,
        origin: phone(),
    })
    .unwrap();
    assert_eq!(frame[13], AppKind::Mutation as u8);
    assert_eq!(u64::from_le_bytes(frame[4..12].try_into().unwrap()), 2);

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(mutation, read_hex(&root.join("fixtures/select0.rmu.hex")));
    assert_eq!(frame, read_hex(&root.join("fixtures/select0.rtr.hex")));
    assert_eq!(intent, read_hex(&root.join("fixtures/select0.intent.hex")));
    let (kind, opened) = bob.open_app(&frame).unwrap();
    assert_eq!(kind, AppKind::Mutation);
    assert_eq!(opened, mutation);

    let output = Command::new("node")
        .arg(root.join("conformance/mutation.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "mutation", &mutation);
    expect_line(&mut lines, "mutation-frame", &frame);
    expect_line(&mut lines, "intent", &intent);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);
}

#[test]
fn javascript_handoff_matches_rust() {
    let mut origin = started();
    let manifest = SCHEMA.manifest().unwrap();
    let mut alice = Channel::establish(tv(), phone(), &accepted_pairing()).unwrap();
    let mut bob = Channel::establish(phone(), tv(), &accepted_pairing()).unwrap();
    let hello = alice.seal_hello(&manifest).unwrap();
    let reply = bob.seal_hello(&manifest).unwrap();
    alice.open_hello(&manifest, &reply).unwrap();
    bob.open_hello(&manifest, &hello).unwrap();
    origin.prepare_handoff(phone()).unwrap();
    let prepare = super::handoff::encode_prepare(&origin).unwrap();
    let prepare_frame = alice.seal_app(AppKind::Prepare, &prepare).unwrap();
    let (prepare_kind, opened_prepare) = bob.open_app(&prepare_frame).unwrap();
    assert_eq!(prepare_kind, AppKind::Prepare);
    assert_eq!(opened_prepare, prepare);
    let grant = origin.grant(phone()).unwrap();
    let grant_plain = super::handoff::encode_grant(&grant).unwrap();
    let grant_frame = alice.seal_app(AppKind::Grant, &grant_plain).unwrap();
    assert_eq!(grant.epoch, 2);
    assert_eq!(grant.revision, 0);
    assert_eq!(grant.fingerprint, origin.fingerprint().unwrap());
    assert_eq!(grant_frame[13], AppKind::Grant as u8);
    assert_eq!(
        u64::from_le_bytes(grant_frame[4..12].try_into().unwrap()),
        3
    );
    let (grant_kind, opened_grant) = bob.open_app(&grant_frame).unwrap();
    assert_eq!(grant_kind, AppKind::Grant);
    assert_eq!(opened_grant, grant_plain);
    assert!(!origin.is_writer());

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(prepare, read_hex(&root.join("fixtures/prepare.rcb.hex")));
    assert_eq!(grant_plain, read_hex(&root.join("fixtures/grant.rcb.hex")));
    assert_eq!(grant_frame, read_hex(&root.join("fixtures/grant.rtr.hex")));

    let output = Command::new("node")
        .arg(root.join("conformance/handoff.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "prepare", &prepare);
    expect_line(&mut lines, "grant", &grant_plain);
    expect_line(&mut lines, "grant-frame", &grant_frame);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);
}

#[test]
fn javascript_journal_matches_rust() {
    let mut origin = started();
    let snapshot = origin.encode_snapshot().unwrap();
    let mutation = encode_envelope(&select(&mut origin, 0).unwrap()).unwrap();
    let mut log = Durable::open(&snapshot).unwrap();
    log.append(&mutation).unwrap();
    let journal = log.encode().unwrap();
    let recovered = Durable::recover(SCHEMA, tv(), &journal, state_ok).unwrap();
    assert!(recovered.is_writer());
    assert_eq!(recovered.revision(), 1);
    assert_eq!(
        recovered.fingerprint().unwrap(),
        origin.fingerprint().unwrap()
    );

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(journal, read_hex(&root.join("fixtures/select0.rjn.hex")));

    let output = Command::new("node")
        .arg(root.join("conformance/journal.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "journal", &journal);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);
}

#[test]
fn javascript_capabilities_match_rust() {
    let caps = DeviceCaps {
        display: false,
        interactive_input: true,
        touch: false,
        large_display: false,
    };
    let record = encode_capabilities(&caps).unwrap();
    assert_eq!(decode_capabilities(&record).unwrap(), caps);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(record, read_hex(&root.join("fixtures/controller.rcb.hex")));

    let output = Command::new("node")
        .arg(root.join("conformance/surface.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "capabilities", &record);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);
}

#[test]
fn javascript_interpreter_matches_compiled_reducer() {
    let mut origin = started();
    select(&mut origin, 0).unwrap();
    origin.prepare_handoff(phone()).unwrap();
    let grant = origin.grant(phone()).unwrap();
    assert_eq!(grant.epoch, 2);
    assert_eq!(grant.revision, 1);
    assert!(!origin.is_writer());
    let snapshot = origin.encode_snapshot().unwrap();

    let mut compiled = resume(phone(), &snapshot).unwrap();
    assert!(compiled.is_writer());
    assert_eq!(compiled.epoch(), 2);
    let second = encode_envelope(&select(&mut compiled, 7).unwrap()).unwrap();

    let contract = pairs_continuity_contract().unwrap();
    let decoded = decode_contract(&contract).unwrap();
    assert_eq!(decoded.actions, vec![ACTION_SELECT, ACTION_RESOLVE]);
    assert_eq!(decoded.schema_fingerprint, SCHEMA.fingerprint().unwrap());
    negotiate_contract(&contract, &pairs_continuity_contract().unwrap()).unwrap();
    assert_ne!(ExecutionMode::Compiled, ExecutionMode::Interpreted);

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(second, read_hex(&root.join("fixtures/select7.rmu.hex")));
    assert_eq!(contract, read_hex(&root.join("fixtures/contract.rcb.hex")));

    let output = Command::new("node")
        .arg(root.join("conformance/interpret.mjs"))
        .current_dir(&root)
        .output()
        .unwrap_or_else(|err| panic!("node failed to start: {err}"));
    assert!(
        output.status.success(),
        "node status {:?}\nstderr {}\nstdout {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    expect_line(&mut lines, "mutation", &second);
    expect_line(&mut lines, "contract", &contract);
    assert_eq!(lines.next(), Some("equivalent"));
    assert_eq!(lines.next(), None);

    let mut interpreted = resume(phone(), &snapshot).unwrap();
    assert_eq!(
        interpreted.apply_remote(&second, state_ok).unwrap(),
        Apply::Applied
    );
    assert_eq!(interpreted.revision(), 2);
    assert_eq!(interpreted.epoch(), 2);
    assert_eq!(
        compiled.fingerprint().unwrap(),
        interpreted.fingerprint().unwrap()
    );
    assert_eq!(interpreted.values().get(&F_SCORE), Some(&Value::Int(1)));
    assert_eq!(completion_permille(&interpreted).unwrap(), 250);
    assert_eq!(select(&mut origin, 7).unwrap_err().code, "RMT-E5005");
    assert_eq!(select(&mut interpreted, 7).unwrap_err().code, "RMT-E5004");
    let joined = join(id_from_byte(3), &interpreted.encode_snapshot().unwrap()).unwrap();
    assert!(!joined.is_writer());
    assert_eq!(
        joined.fingerprint().unwrap(),
        interpreted.fingerprint().unwrap()
    );
}
