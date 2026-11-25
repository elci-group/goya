# romántico Phase 1 — portable state foundation

Status: the snapshot-equivalence gate for this tree is met on one host. Date: 25 November 2026. The directive's device P0 checklist stays open.

## Exit gate

Two encoders produced the same bytes for the same literals:

- `goya-romantico`, the Rust crate
- `crates/goya-romantico/conformance/rcb.mjs`, run by Node.js 18.19.1

The JavaScript file does not call the Rust crate and does not read the Rust encoder's output. Checked-in hex under `crates/goya-romantico/fixtures/` is a third copy. `javascript_encoder_matches_rust_snapshots` requires the crate, the fixture, and the Node script to agree. The script decodes its own bytes and checks the model before it prints `equivalent`.

The documents are:

| Document | Contents |
| --- | --- |
| `types.rcb.hex` | bool false, `i64::MIN`, float 1.5, text `ñ`, enum 7, a map with keys `a` and `b`, optional ref 9, optional none, bytes `01 02 03`, timestamp −1 |
| `pairs-open.rcb.hex` / `pairs-open.rsn.hex` | Goya Pairs board `[0, 1, 2, 3, 3, 2, 1, 0]`, empty match list, revision 0, epoch 1 |
| `pairs-matched.rcb.hex` / `pairs-matched.rsn.hex` | the same board after selecting cards 0 and 7: matched `[0, 7]`, one move, score 1, revision 2 |

Restoring the matched `RSN1` snapshot leaves three pairs and a completion of 250. The `RSN1` header is 92 bytes. The 32 bytes at offset 24 are the schema fingerprint. That digest is SHA-256 of the schema document. It is not a digest of the portable body.

## Semantic identity

A fingerprint covers the stable field id, the class discriminant, and the wire type. The source name is a unique alias and is not hashed. `renaming_the_alias_keeps_the_fingerprint` renames `count` to `total` and keeps the digest, then changes the wire type from int to text and gets a different digest.

Wire-type discriminants: bool 1, int 2, float 3, text 4, enum 5, int list 6, bytes 7, timestamp 8.

The Pairs schema fingerprint covers all 11 fields, including viewport, pointer, and secret. Those three stay out of the snapshot body. Local, ephemeral, and private bytes are absent from both the open and the matched documents.

## Evidence

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 37 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. The temporary target directory was removed after that run. Library code does not reference `std::net` or `std::fs`. The conformance test reads the fixtures and starts `node`.

## What remains open

This gate is two implementations on one Linux host, in the language a webOS application would use. A television and an Android 9 phone have not run the crate. RCB/1 stays an interim codec until a CBOR candidate wins a benchmark. The authenticated channel that followed this gate is recorded in `phase2.md`. The 15 P0 device gates in the directive are still open. RMT-T001 and RMT-T002, as device runs, were not executed. Their logical stand-in is the fixture comparison above.
