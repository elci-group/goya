# romántico protocol — GY-ROMANTICO/0.1

Status: this describes the in-process crate. It is not an RFC and it is not a device certification. Date: 25 November 2026.

## Documents

`RCB1` is the canonical value encoding. Lengths are little-endian. Map keys and record fields are strictly increasing. Negative zero becomes positive zero. NaN and infinities are rejected. JSON is a diagnostic manifest only.

`RSN1` is a snapshot. The header is magic, session (16), schema version (`u32`), schema fingerprint (32), epoch (`u64`), revision (`u64`), authority (16), and body length (`u32`). The portable `RCB1` body follows. That header is 92 bytes.

`RMU1` is a mutation. The header is magic, session, mutation id, origin, epoch, base revision, revision, and schema version. Operations follow. The id is the first 16 bytes of SHA-256 over `GY-ROMANTICO/MUT/1`, the session, the origin, the epoch, the base, the revision, the schema version, and the canonical operations. The revision is the base plus one.

`RCK1` is a checkpoint. The layout is magic, version byte 1, kind (1 prepared, 2 granted), from, to, epoch, revision, fingerprint (32 zeros when prepared), snapshot length, and snapshot bytes.

`RTR1` is a transport frame. The header is 46 bytes: magic, sequence (`u64`), direction, kind, sender (16), and receiver (16). Ciphertext and a 16-byte tag follow. The header is the additional data.

`RJN1` is a caller-held journal. The layout is magic, base length, one `RSN1` or `RCK1`, tail count, then each tail length and `RMU1`. It is not an `RTR1` kind. The core does not open a file.

The continuity contract is an `RCB1` record of nine fields and has no separate magic: application id, protocol `0.1`, schema version, schema fingerprint, authority text `single_writer`, strictly increasing action ids, persistence 1 (caller-held), reconciliation 1 (reject a gap), and migrations 0. Execution mode is not a field.

## Transport kinds

1. HELLO
2. snapshot
3. intent
4. mutation
5. acknowledgement
6. prepare
7. grant
8. abort
9. capabilities: display, interactive input, touch, and large display

Kinds 1 and 2 are sealed only before `open_app`. `open_app` rejects them.

## Key

The key is HKDF-SHA256 (RFC 5869). The salt is the lexicographically lower 16-byte device id followed by the higher id. The input keying material is the accepted 16-byte pairing code. The info string is `GY-ROMANTICO/AEAD/1`. The cipher is ChaCha20-Poly1305 (RFC 8439). The nonce is 12 bytes: direction, three zero bytes, and the sequence as a little-endian `u64`. Direction is 0 when the sender id is lower than the receiver id, and 1 otherwise. The sequence starts at 1. The pairing code is not sent. This is not a key exchange. There is no socket, no TLS, and no certificate.

## HELLO

HELLO plaintext is an `RCB1` record of six fields: protocol, application id, schema version, schema fingerprint, authority model, and the supported feature list. The supported features, in order, are snapshot, mutation, handoff, and reconnection. `authenticated_transport` is required and is not in that list. Negotiation compares application id, protocol, schema version, fingerprint, and authority model.

## Discovery

`select_discovery` returns the manual endpoint and the pairing-code fallback. Claimed platform mechanisms are stored and not contacted.

## Errors

A code is nine characters: `RMT-E` and four digits. The detail is a fixed sentence.

- E1xxx schema shape
- E2xxx encoding and compatibility
- E4xxx consent, pairing, transport, and permission
- E5xxx authority, session, gap, and invariant
- E6xxx corrupt snapshot, checkpoint, or journal
- E7xxx capability
- E8xxx limits

## Limits

Nesting is 16. A collection is 4096 items. One blob is 64 KiB. One document is 1 MiB. The journal holds 64 envelopes. Pending intents are 32. Pairing failures lock at 5. The diagnostic trace holds 64 events. A full trace drops the oldest event. The trace is process-local. It is absent from the snapshot, the journal, the fingerprint, and the wire.

## Absent from the wire

Execution mode, action aliases, surface preferences, local fields, ephemeral fields, private fields, and the diagnostic trace are not encoded.

## Not in this crate

The crate opens no socket and no WebSocket. It does not speak mDNS. It does not check a certificate. It does not elect a second writer, and it does not fail over on a timeout.
