# romántico Phase 8 — enterprise hardening

Status: the crate records a bounded diagnostic trace and rejects a bounded set of malformed documents with structured errors. The directive exit gate, a release candidate that satisfies the enterprise acceptance gates, is not met. Date: 25 November 2026.

## Diagnostics

The trace names outcomes the replica actually observes: session created, session joined, mutation committed, acknowledged, rejected, deduplicated, or stale, authority transfer started, completed, or failed, and snapshot applied or rejected. It holds 64 events. When it is full, the oldest event is dropped. That drop does not drop a committed mutation. The trace is absent from the snapshot, the journal, the fingerprint, and the wire. Installing a snapshot does not clear it. A torn journal fails before reload and does not grow it. A replica built again by recovery starts a new trace.

Names that need a socket, a discovery service, or a divergence detector are not emitted. Those include `session.left`, `device.discovered`, `device.authenticated`, `device.revoked`, `snapshot.sent`, `replica.diverged`, `replica.reconciled`, `transport.connected`, `transport.disconnected`, and `transport.recovered`. `mutation.stale` is recorded because a stale apply is success, not a rejection. A successful abort records `authority.transfer.failed` because the transfer did not complete. An abort after grant is `RMT-E5005` and records nothing further.

## Decoder probe

The probe is a test. It is not a coverage-guided campaign and it does not link libfuzzer. It builds a boolean, a record, a Pairs snapshot, an `RMU1`, the continuity contract, a capability record, a checkpoint, and an `RJN1`, then mutates each inside a fixed window. Every decoder returns `Ok` or an `RMT-E` code with a non-empty detail. A collection count of 4097 and a blob longer than 64 KiB are `RMT-E8001`. Sixteen nested arrays fail to encode. Garbage after both hellos is `RMT-E4005` and does not move the channel counter. Eight zero bytes do not change a replica's revision or snapshot, and the trace records `mutation.rejected`. The four bytes `RJN1` do not change the snapshot or the trace.

## Budgets

Latency and RAM were not measured. The enforced ceilings are nesting 16, a collection of 4096 items, a blob of 64 KiB, a document of 1 MiB, a journal of 64, 32 pending intents, 5 pairing failures, and a trace of 64. Idle RAM, package size, and LAN latency are not claimed.

## Cross-check

`CARGO_TARGET_DIR=/tmp/goya-romantico-target cargo test -p goya-romantico --offline --lib` passed 91 tests. `cargo clippy -p goya-romantico --offline --all-targets -- -D warnings` passed. `cargo tree -p goya-romantico -i getrandom` found no such package. Library code does not reference `std::net` or `std::fs`. `std::fs` appears only in the conformance hex reader. The temporary target directory was removed after that run.

The protocol note is `docs/romantico/protocol.md`. The threat note is `docs/romantico/threats.md`. Quinta was not integrated.

## What remains open

The 15 P0 device gates stay open. This slice did not measure latency or RAM, did not run a coverage-guided fuzzer, and did not integrate Quinta. There is no socket, no WebSocket, and no TLS session. No certificate was verified, and none was ignored. The core does not open a file. The release-candidate exit gate is not met.
