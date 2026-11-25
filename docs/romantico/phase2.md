# romántico Phase 2 — authenticated session transport

Status: negotiation, pairing, discovery fallback, and encrypted frames work in one process. The directive exit gate, authenticated webOS–Android connectivity, is not met. Date: 25 November 2026.

## Channel

`Channel::establish` requires a pairing that has already been accepted with consent. The 16-byte code is not written into the frame. HKDF-SHA256 (RFC 5869) turns it into a 32-byte key. The salt is the two device ids in ascending byte order. The info string is `GY-ROMANTICO/AEAD/1`. The cipher is ChaCha20-Poly1305 (RFC 8439) from `chacha20poly1305` 0.10.1 with the `getrandom` feature left off, plus `hkdf` 0.12.4. The core does not read the operating-system random source.

A frame is magic `RTR1`, a little-endian sequence, a direction byte, a message type, the sender, the receiver, then ciphertext and the 16-byte tag. The header is additional authenticated data. Direction `0` belongs to the lower device id. Counters start at 1. A gap, a replay, a wrong peer, a wrong code, or a flipped tag is `RMT-E4005`. No partial snapshot is installed.

The first payload each way is a HELLO: protocol, application id, schema version, schema fingerprint, authority model, and the supported feature list, encoded as RCB/1. A mismatch is `RMT-E2002`. A snapshot cannot be sealed until both hellos have been accepted. `admit_transport` accepts `Channel::capabilities()` because the frames are encrypted and ordered. A plaintext capability report is still refused.

## Discovery

`select_discovery` returns the manual endpoint, with pairing-code discovery kept as the fallback. A report that claims multicast, inbound sockets, background execution, Bluetooth, platform-native discovery, or a rendezvous service is stored and not used. Those adapters are not implemented. The crate opens no socket.

## Cross-check

Node.js 18.19.1, through `node:crypto`, and the Rust channel produce the same hello frame and the same sealed open Goya Pairs snapshot. The fixtures are `crates/goya-romantico/fixtures/hello.rtr.hex` and `crates/goya-romantico/fixtures/snapshot.rtr.hex`. The JavaScript script decrypts both, checks that the board is `[0, 1, 2, 3, 3, 2, 1, 0]`, and checks that a tampered tag does not authenticate.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 48 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico` does not contain `getrandom`. Library code does not reference `std::net` or `std::fs`. The temporary target directory was removed after that run.

## What remains open

Two peers on one Linux host agreed on ciphertext. A webOS television and an Android 9 phone did not connect. There is no WebSocket and no TLS session. No certificate was verified, and none was ignored. The 15 P0 device gates stay open. Live delivery of an intent and a mutation over this channel is recorded in `phase3.md`.
