# romántico Phase 4 — authority handoff

Status: the single writer moves from one in-process replica to the other. The directive exit gate, transfer execution and terminate the originating application, is not met. No process is killed. Date: 25 November 2026.

## Messages

After both HELLO frames, three more application kinds use the same channel. Prepare is 6, grant is 7, and abort is 8. The snapshot that finishes the grant stays kind 2. Prepare, grant, and abort are RCB/1 records. They are not new document magics.

Prepare names the session, the origin, the successor, the current epoch, and the revision. It suspends the origin's commits (`RMT-E5006`). Grant names those parties, the next epoch, the same revision, and the fingerprint of the portable body. Grant clears the origin's mutate right before the successor activates. The epoch is not rolled back when the following snapshot is lost. Abort after a grant is `RMT-E5005`.

A snapshot taken before the grant does not activate (`RMT-E5008`). The post-grant snapshot does. Until that snapshot is accepted, neither replica can commit. Delivering the post-grant snapshot activates the successor. A granted `RCK1` checkpoint reloads only the successor as the writer. A device that is not the channel peer is `RMT-E4006` and is not prepared.

## The session continues

The television selects card 0 and the phone applies that envelope. The handoff then runs. The phone activates at epoch 2 and revision 1 with the same fingerprint. The phone selects card 7. Both replicas show score 1, completion 250, and the same fingerprint. A later `select` on the television is `RMT-E5005`. Replaying the epoch-1 envelope on the phone is `RMT-E5001` and leaves the score in place. Aborting a prepare, before the grant, restores the television's commit right and leaves the epoch at 1.

## Cross-check

Node.js 18.19.1 rebuilds the idle prepare and the idle grant. The grant fingerprint is SHA-256 of the open portable body. The grant frame is television-to-phone sequence 3, kind 7, after the hello and the prepare. The fixtures are `crates/goya-romantico/fixtures/prepare.rcb.hex`, `crates/goya-romantico/fixtures/grant.rcb.hex`, and `crates/goya-romantico/fixtures/grant.rtr.hex`. Rust and that script agree.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 65 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico -i getrandom` found no such package. Library code does not reference `std::net`. `std::fs` appears only in the conformance hex reader. The temporary target directory was removed after that run.

## What remains open

Two replicas on one Linux host transferred the writer and kept the Goya Pairs session. A webOS television did not exit, and an Android 9 phone did not take over a running game. There is no socket, no WebSocket, and no TLS session. No certificate was verified, and none was ignored. The caller-held journal is recorded in `phase5.md`. The core still does not open a file. `RCB/1` is still the interim codec. Unplanned failover is not implemented. The 15 P0 device gates stay open.
