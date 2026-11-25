# romántico Phase 5 — persistence and resilience

Status: two replicas on one Linux host reload the same session from a caller-held journal. The directive exit gate, recovery without semantic divergence on a device, is not met. A television did not lose power. A phone did not reconnect over a network. Date: 25 November 2026.

## Journal

`RJN1` is a byte string the caller holds. It is not an `RTR1` frame and it is not a file. The base is one `RSN1` snapshot or one `RCK1` checkpoint. The tail is at most 64 canonical `RMU1` envelopes. Each envelope extends the previous revision and names the same session. Truncation or trailing bytes are `RMT-E6001` and install nothing. A gap is `RMT-E6002` and installs nothing. The 65th tail entry is `RMT-E6002` until compaction.

Compaction replaces the base with a snapshot whose revision lies on the journal and drops the tail entries that snapshot already covers. A foreign session is `RMT-E5002` and leaves the log unchanged. A checkpoint whose record disagrees with the inner snapshot is `RMT-E5008` and is not stored.

## Recovery

`recover` builds a new replica. An `RSN1` base admits the device named as authority. An `RCK1` base admits only the writer named by the checkpoint. The tail applies all-or-nothing. An invariant failure is `RMT-E5004` and returns no replica. Local, ephemeral, private, and pending values are not in the bundle.

`reload` decodes before it touches the live replica. `catch_up` applies on a clone and swaps only after success. A replica behind the base installs the snapshot, then replays the tail. The same install runs when the revision matches the base but the epoch or authority does not. That is the post-grant snapshot. The install does not change permissions, so a live follower does not become the writer. A replica inside the tail replays only the suffix. A replica ahead of the tip is refused.

`reconnect` runs that check on a clone before it seals anything. The snapshot frame stays kind 2. Each mutation stays kind 4. At the tip, reconnect sends nothing. The journal authority must be the writer channel (`RMT-E4006` otherwise).

## The session continues

The television selects card 0 and card 7. The phone has neither. Reconnect leaves both at revision 2, score 1, completion 250, and the same fingerprint. A second reconnect does not move the channel counters. After a grant, the journal base is the granted checkpoint and the tail is the phone's card 7. Reloading admits only the phone as the writer. The television reloads the same fingerprint and cannot commit. A live phone that still has the pre-grant epoch installs the snapshot and stays a follower.

A score of 99 committed under a permissive invariant is refused under the Pairs invariant. The live follower keeps revision 0, its pending intent, and its viewport. A spliced pair of revision-1 envelopes is `RMT-E6002` and does not install.

## Cross-check

Node.js 18.19.1 rebuilds the idle snapshot and the first `select(0)` envelope and wraps them as `RJN1`. The fixture is `crates/goya-romantico/fixtures/select0.rjn.hex`. Rust and `conformance/journal.mjs` agree. Recovering that bundle restores a writer at revision 1 with the same fingerprint.

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 74 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico -i getrandom` found no such package. Library code does not reference `std::net` or `std::fs`. `std::fs` appears only in the conformance hex reader. The temporary target directory was removed after that run.

## What remains open

Two replicas on one Linux host reloaded the same Goya Pairs session from bytes. A television did not lose power, and a phone did not reconnect across a network. There is no socket, no WebSocket, and no TLS session. No certificate was verified, and none was ignored. The core does not open a file. `RCB/1` is still the interim codec. Unplanned failover is not implemented. The 15 P0 device gates stay open. Surface roles on this channel are recorded in `phase6.md`.
