# romántico decisions — RMT-ADR-001 through RMT-ADR-012

Status: accepted for `GY-ROMANTICO/0.1` where marked. Date: 25 November 2026. These decisions bind the in-process crate. They do not close the device programme.

## RMT-ADR-001 — Semantic state ontology

**Context.** `GY-STATE/1` stores process-local scalars. The resolver publishes D1 revisions on a different rule and does not use that store. A portable session cannot be a `HashMap` dump.

**Decision.** romántico state is a schema of fields with a stable numeric id, one class, and one wire type. Classes are shared, local, derived, ephemeral, private, authoritative, and resumable. The source name is a unique alias. The schema fingerprint is SHA-256 over the id, the class, and the wire type. The alias is not an input, so renaming it keeps continuity. Only shared, authoritative, and resumable fields enter a snapshot. Derived values are functions of that snapshot. Goya Pairs keeps the deck, the selection, the score, the move count, the phase, and the pending reveal. Viewport, pointer, and secret stay on the device that wrote them. Their definitions still contribute to the schema fingerprint.

**Consequences.** `goya-romantico` does not depend on `goya-state`. A later host may mirror a snapshot into a D1 owner. It must not treat the `HashMap` as the wire form. Changing an id, a class, or a wire type is a different schema.

## RMT-ADR-002 — Canonical encoding

**Context.** The directive prefers deterministic CBOR after an ADR and a benchmark. The offline lock contains `sha2` and no CBOR implementation. No benchmark was run.

**Decision.** The interim codec is RCB/1: little-endian, length-delimited, strictly increasing map keys and record fields, UTF-8 text, finite floats, negative zero normalised to positive zero. SHA-256 over those bytes is the fingerprint. JSON is a diagnostic manifest only.

**Consequences.** RCB/1 is not frozen as the cross-implementation wire format. A CBOR candidate has to beat it on encoded size and decode cost before it replaces RCB/1. Unknown snapshot fields fail negotiation. They are not dropped. Phase 1 checked byte equivalence with an independent JavaScript encoder on Node.js 18.19.1, one Linux host, using the language a webOS application would run. That check is not a television-to-phone session, and it does not adopt CBOR.

## RMT-ADR-003 — Wire protocol

**Context.** The directive lists a full message family. Building that family on an unadmitted transport would describe bytes nothing can carry.

**Decision.** Two documents are specified and tested: `RSN1` snapshots and `RMU1` mutation envelopes. A mutation id is the first 16 bytes of SHA-256 over a domain separator and the canonical body. Revision must be `base + 1`. A duplicate id with the same bytes is a no-op. A hole is `RMT-E5003`; the replica does not buffer it. The caller fetches the journal tail or a snapshot. After both HELLO frames, an intent, that `RMU1` envelope, and an acknowledgement travel as `RTR1` application frames. The intent and the acknowledgement are RCB/1 records. They are not new document magics.

**Consequences.** `HELLO` is on the wire. Phase 3 adds intent (kind 3), mutation (kind 4), and acknowledgement (kind 5). Phase 4 adds prepare (kind 6), grant (kind 7), and abort (kind 8). The snapshot that finishes a grant stays kind 2. Phase 6 adds capabilities (kind 9). That record is four booleans: display, interactive input, touch, and large display. It is not a device fingerprint. `PAIR_*` and `HEARTBEAT` are not. Pairing remains a local state machine. Journal windows are bounded to 64 envelopes; an older replica receives `RMT-E6002` and must take a snapshot. A rejected mutation is still acknowledged, so the two directions keep the same counter. The acknowledgement names the mutation and the journal outcome. It does not buffer a hole.

## RMT-ADR-004 — Initial transport

**Context.** webOS is not a served host, so a WebSocket topology cannot be measured. A plaintext loopback marked "secure" would be the shortcut the directive forbids.

**Decision.** `admit_transport` accepts only a reliable adapter that reports authenticated encryption and a message bound inside the 1 MiB canonical budget. The admitted adapter is `Channel`: HKDF-SHA256 (RFC 5869) over the accepted pairing secret, then ChaCha20-Poly1305 (RFC 8439). Frames are `RTR1`. Counters start at 1, each direction has its own nonce space, and a gap, replay, or failed tag is `RMT-E4005`. `select_discovery` keeps the manual endpoint and the pairing-code fallback. Claimed mDNS, Bluetooth, platform discovery, and rendezvous are not contacted. The crate opens no socket.

**Consequences.** This channel is authenticated encryption between two in-process peers. It is not a WebSocket, not TLS, and not a device link. Certificate verification is not disabled: the adapter has no certificates. A later socket adapter has to verify them or refuse to start. Live webOS–Android connectivity is still blocked on a measured host.

## RMT-ADR-005 — Trust and pairing

**Context.** Presence on a network must not grant state. A homemade key exchange is prohibited. The cipher and the KDF are RFC 8439 and RFC 5869, keyed from the out-of-band pairing secret.

**Decision.** `Pairing::offer` takes 16 caller-supplied bytes. The core does not read a random device. Accept requires consent, a matching code, an unexpired caller tick, and fewer than five failures. Comparison of equal-length codes is byte-wise without an early return. Locks and expiry fail closed.

**Consequences.** Generating the 16 bytes is an adapter duty, using the platform CSPRNG. The core still does not read a random device. After consent, those bytes are the HKDF input keying material and are not placed on the frame. They need to be a 16-byte secret, not a short PIN. A paired flag without `Channel::establish` is not a secure channel. Device pairing, RMT-005 on real hardware, stays open.

## RMT-ADR-006 — Authority transfer and fencing

**Context.** The first release needs one writer. Timeout-based failover splits a two-device session.

**Decision.** The initial model is transferable single-writer. Epochs start at 1. Prepare suspends commits. Grant moves the epoch up by one and clears the origin's mutate right before the successor activates. Activation requires the post-grant snapshot. A `Prepared` checkpoint keeps the origin as the only writer. A `Granted` checkpoint names only the successor. If the snapshot disagrees with the record, restore returns `RMT-E5008` and creates no writer. Epoch-mismatched mutations return `RMT-E5001`.

**Consequences.** Unplanned failover is not implemented. Prepare, grant, and abort cross the channel. The post-grant snapshot is a separate frame. Losing it suspends both writers. Recovery is a later delivery of that snapshot, or a reload of the granted `RCK1` checkpoint. The origin's mutate right stays cleared. That fence is inside the process. It does not terminate an operating-system process. A lost post-grant snapshot suspends writes. That is the intended failure.

## RMT-ADR-007 — Persistence and durability

**Context.** The directive separates volatile, locally durable, and remotely acknowledged state. This environment has no shared disk between a television and a phone.

**Decision.** Checkpoints are `RCK1` byte strings. A durable journal is an `RJN1` byte string: one `RSN1` or `RCK1` base plus an ordered `RMU1` tail of at most 64. Compaction replaces the base and drops the covered tail. Truncation fails with `RMT-E6001` and does not install. A gapped or non-extending tail fails with `RMT-E6002` and does not install. Reload applies the tail all-or-nothing. A snapshot install replaces portable state only after the body decodes and matches the schema fingerprint. A channel acknowledgement means the peer replica applied the envelope, recognised a duplicate or a stale revision, or rejected it. It does not mean a device flushed storage.

**Consequences.** Crash recovery is tested by reloading those bytes in a new `Replica`. The byte string is not a filesystem write. The core does not open files. Power loss on real storage is untested. Do not describe a mutation as durable beyond the process that holds the journal. The acknowledgement is that process answering across the channel, still inside one address space. A torn journal leaves the live replica unchanged.

## RMT-ADR-008 — Offline behaviour

**Context.** A disconnected follower must not present a queued intent as a committed move.

**Decision.** Followers may queue at most 32 intents when they hold `submit_intent`. The queue is excluded from the fingerprint and the revision. The authority commits or rejects. Catch-up replays the journal. The queue remains pending until the host acknowledges it. A read-only surface cannot enqueue (`RMT-E4006`).

**Consequences.** There is no speculative score. Last-write-wins is not a reconciliation policy. Authoritative invariants reject a partial score update. A follower intent can cross the channel. Delivery does not clear the queue. The host still acknowledges the queued intent itself. The channel acknowledgement is journal acceptance, not disk durability.

## RMT-ADR-009 — Compiler and Resolver integration

**Context.** The directive's grammar is illustrative. The scene language has a separate contract for control flow, and the resolver's publication rule is `GY-D1-APP/1`.

**Decision.** The parser, the D1 machine, and the resolver stay unchanged. The shared contract is an RCB/1 record of the application id, protocol `0.1`, schema version, schema fingerprint, `single_writer`, the numeric action ids, the caller-held persistence policy, the reject-gap reconciliation policy, and a migration count of zero. Action names are aliases and are not encoded. Execution mode is a host label. It is not in that record, the schema fingerprint, HELLO, or a mutation id. A compiled reducer and an independent interpreter exchange the same `RMU1`. HELLO is unchanged.

**Consequences.** RMT-001 stays open. A `romantico` keyword needs its own language-contract revision before any grammar patch. RMT-T016 was checked as two reducers on one Linux host. An interpreted `.gscene` did not join a compiled application. `GY-RESOLVER/0.1` does not consume the contract. The directive exit gate is not met.

## RMT-ADR-010 — Schema evolution

**Context.** Silent drops of unknown authoritative fields break continuity.

**Decision.** Negotiation requires the same application id, protocol `0.1`, schema version, fingerprint, and `single_writer` authority model. No migration is registered. A mismatch is `RMT-E2002`.

**Consequences.** Renaming the source alias does not change the fingerprint and needs no migration. Changing a field id, class, or wire type does. Additive fields and value conversions remain future registrations. A newer version number does not imply a migration.

## RMT-ADR-011 — Distributed interface roles

**Context.** Pairs on a television and a phone should not be three copies of one widget tree.

**Decision.** `assign_surface` grants mutate and transfer only to `Primary`. `Controller` gets read and submit. `Auxiliary` gets read. A missing required display or input is `RMT-E7001`. On the channel that refusal is checked before the frame is sealed, so nothing is sent. Touch and a large display are preferences. A missing preference still assigns and records that the preference was not met, so a remote control can still be a controller. `adopt_surface` refuses to give mutate or transfer to a replica that is not the authority, and it refuses to lower the active writer to a controller or a reader. Both are `RMT-E4006`. Kind 9 carries display, interactive input, touch, and large display. It carries no resolution, refresh rate, sensor, or operating-system name. A controller commits a card only by intent. Those intent bytes do not name the modality.

**Consequences.** The role is a permission set on the existing session. This slice does not draw a television board or a phone control surface. The directive exit gate is not met.

## RMT-ADR-012 — Resource limits

**Context.** Continuity must not imply an unbounded runtime. The directive's latency and RAM budgets are unmeasured.

**Decision.** Enforced limits in this crate: nesting 16, collections 4096 items, a single blob 64 KiB, a document 1 MiB, journal 64, pending intents 32, pairing failures 5, diagnostic trace 64. Past the journal window the replica demands a snapshot instead of growing without bound. When the trace is full, the oldest event is dropped. The trace is not durable and is not part of the snapshot. Fuzzing in this slice is a bounded deterministic probe in the test suite, not a coverage-guided campaign.

**Consequences.** Idle RAM, package size, and LAN latency are not claimed. Latency and RAM were not measured. The next measurement has to compare a host with and without the crate linked, on the same target, and report any external session host separately.
