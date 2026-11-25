# romántico Phase 3 — live synchronisation

Status: two in-process replicas of Goya Pairs stay one session. The directive exit gate, one session on both a webOS television and an Android 9 phone, is not met. Date: 25 November 2026.

## Messages

After both HELLO frames, the channel carries three application kinds. Intent is 3, mutation is 4, and acknowledgement is 5. Hello remains 1 and snapshot remains 2. `open_app` does not accept those two. The kind byte sits in the authenticated header, so a flipped kind fails the tag.

An intent is an RCB/1 record: an action enum, a non-negative argument, and a 16-byte origin. The origin must be the follower channel's local id and the authority channel's peer. Any other origin is `RMT-E4006` and is not executed. A rejected intent is still acknowledged, so the two sequence counters stay paired. `deliver_intent` does not clear the follower's pending queue, and the queue is not part of the revision.

A mutation frame's plaintext is canonical `RMU1`. The receiver applies it. `Applied`, `Duplicate`, and `Stale` are acknowledged with that outcome. A gap is `RMT-E5003` and is not buffered. The acknowledgement is an RCB/1 record: the mutation id (16 zero bytes when the plaintext is not an envelope), the outcome, the follower revision after the attempt, and a code. The code is empty unless the outcome is rejected. A rejection with an empty code, or a success with a code, is `RMT-E2001`. An acknowledgement that names a different mutation is `RMT-E5002`.

A follower cannot commit. `commit_authority` on that replica is `RMT-E5005`. A frame sealed by the phone carries the phone as its sender. The authority rejects a frame whose sender is itself. Once the authority has seen an envelope id, a copy of those bytes applies as `Duplicate`.

The sync path takes the caller's invariant. It does not call Goya Pairs. The tests pass the Pairs invariant.

## One session

The television replica and the phone replica start from the same open snapshot. The phone queues `select(0)` without moving the revision or the fingerprint. Delivery of that intent commits it on the television and applies it on the phone. Both sit at revision 1 with card 0 selected. The television then selects card 7. That match is one transaction. Both sit at revision 2, score 1, completion 250, and the same fingerprint.

A second delivery of the first envelope is `Duplicate` and leaves the score at 0 until the match arrives. Delivering the match before the first select leaves the phone at revision 0 with `RMT-E5003`. Delivering the two envelopes in order then makes the fingerprints match. Selecting card 0 again is `RMT-E5004` and changes neither replica. Installing the revision-2 snapshot and delivering the first envelope again is `Stale` and does not rewind the score. Bytes that are not an envelope are `RMT-E2001`, and the next in-order mutation still applies.

## Cross-check

Node.js 18.19.1 rebuilds the first `select(0)` envelope. The operation list is one `Set` of field 3 to `[0]`. The mutation id is the first 16 bytes of SHA-256 over `GY-ROMANTICO/MUT/1`, the session, the origin, the epoch, the base revision, the revision, the schema version, and those operations. The script seals that plaintext as television-to-phone sequence 2, kind 4, with the Phase 2 key schedule and no snapshot between the hello and the mutation. It also rebuilds the phone's `select(0)` intent. The fixtures are `crates/goya-romantico/fixtures/select0.rmu.hex`, `crates/goya-romantico/fixtures/select0.rtr.hex`, and `crates/goya-romantico/fixtures/select0.intent.hex`. Rust and that script agree.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 61 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico -i getrandom` found no such package. Library code does not reference `std::net`. `std::fs` appears only in the conformance hex reader. The temporary target directory was removed after that run.

## What remains open

Two replicas on one Linux host agreed on one Goya Pairs session. A webOS television and an Android 9 phone did not. There is no socket, no WebSocket, and no TLS session. No certificate was verified, and none was ignored. `RCB/1` is still the interim codec. The 15 P0 device gates stay open. Authority handoff on this channel is recorded in `phase4.md`.
