# Goya EDC D1 replay journal

`goya-determinism::ReplayJournal` is a small deterministic state-transition
primitive for the D1 (state-deterministic) workstream. A caller supplies an
initial opaque byte snapshot and then records transitions as explicit
`(sequence, before, after)` snapshots. Sequence numbers start at zero and must
be contiguous; each `before` snapshot must equal the preceding state.

`replay` checks the supplied initial snapshot, checks the ordered transitions,
and returns the final snapshot. `replay_equal` additionally checks that final
snapshot against an expected byte string. `canonical_bytes` uses the
`GY-EDC-D1-REPLAY/1\0` domain tag, little-endian u64 sequence/count/length
fields, and length-prefixed opaque state bytes. `hash` applies FNV-1a 64 to
those bytes for stable identity/checksum use; it is not cryptographic.

This is a bounded primitive, not general EDC D1 bytecode support. It stores
caller-provided snapshots and verifies their continuity; it does not run a
program, define a state schema, capture process memory, or establish that the
caller generated the transitions honestly. The current bytecode VM still
admits D0 only. D2 effect scheduling, external effect capture/replay, and
cryptographic journal authentication are outside this API. No clocks, random
sources, or I/O are consulted by the journal itself.
