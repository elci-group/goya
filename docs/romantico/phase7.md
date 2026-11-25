# romántico Phase 7 — compiler and resolver expansion

Status: a compiled reducer and an independent interpreter on one Linux host produced the same portable state. The directive exit gate, interoperability between a compiled application and an interpreted `.gscene`, is not met. The resolver was not linked. No scene syntax was added. Date: 25 November 2026.

## Contract

The continuity contract is an RCB/1 record of nine fields: application id, protocol, schema version, schema fingerprint, authority model, action ids, persistence policy, reconciliation policy, and migration count. Action names are aliases and are not encoded. Execution mode is not a field. A tenth field is `RMT-E2001`. A migration count other than zero is `RMT-E2002`. Persistence policy 1 means the caller holds the journal. Reconciliation policy 1 rejects a gap. HELLO is unchanged, and so is schema negotiation.

The same bytes are produced beside a compiled host label and an interpreted host label. The record carries no target name.

## One session, two reducers

The compiled television selects card 0 and grants the writer to the phone. The epoch becomes 2. The revision stays 1. The television can no longer select (`RMT-E5005`). The phone resumes as the writer.

The interpreted reducer then selects card 7. It does not call the compiled `select` for that card. The envelope names the phone, epoch 2, base revision 1, and revision 2. The operations clear the selection, record the matched pair `[0, 7]`, and set the score, the move count, the phase, and the reveal. The compiled `select(7)` produces the same `RMU1`. Applying that envelope leaves both replicas at revision 2, score 1, completion 250, and the same fingerprint. Selecting card 7 again is `RMT-E5004`. A third replica joins the snapshot and is not the writer.

## Cross-check

Node.js 18.19.1 runs `conformance/interpret.mjs`. The fixtures are `crates/goya-romantico/fixtures/select7.rmu.hex` and `crates/goya-romantico/fixtures/contract.rcb.hex`. The script prints `mutation`, then `contract`, then `equivalent`.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 83 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico -i getrandom` found no such package. Library code does not reference `std::net` or `std::fs`. `std::fs` appears only in the conformance hex reader. The temporary target directory was removed after that run.

## What remains open

Two reducers on one Linux host agreed on one Goya Pairs session. An interpreted `.gscene` did not join a compiled application. `GY-RESOLVER/0.1` does not consume this contract, and its D1 values still cannot represent the Pairs lists. There is no `romantico` keyword. webOS, Android, Maja, Sash, Emperor, and Neptune did not run it. There is no socket, no WebSocket, and no TLS session. No certificate was verified, and none was ignored. The core does not open a file. `RCB/1` is still the interim codec. Unplanned failover is not implemented. The 15 P0 device gates stay open. The diagnostic trace and the decoder probe are recorded in `phase8.md`.
