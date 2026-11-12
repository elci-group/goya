# GY-D2-VM/1 — Journal-Replay State Transitions

`goya-executable::D2Program` executes a bounded state transition whose
declared effects are resolved only through `EffectJournal` replay. There is
no live-host effect path in this VM.

## Admission and execution

- Compilation verifies that `ProcessDecl` classifies as D2 and that the
  `ExecutionProof` process name and determinism class match the declaration.
- State and input keys are explicit through the same `StateContract`,
  `D1Snapshot`, and `InputSnapshot` types used by the D1 VM. Snapshot keys
  must exactly match the contract.
- Supported instructions are constants, state/input reads, checked integer
  addition, journal-backed effect requests, and state writes. An effect
  request embeds a capability label and request bytes in the canonical
  program. The label must be among the process's declared effects.
- Each request consumes the next journal entry with an exact capability and
  request match. Its result must decode as the instruction's expected type.
  The VM rejects execution if journal entries remain after the program.
- Integer results encode as a tag plus exactly eight little-endian bytes;
  text results encode as a tag, u32 little-endian length, and exact UTF-8
  payload; booleans encode as a tag plus exactly `0` or `1`.
- Program, snapshot, and outcome encodings are deterministic and hashed with
  SHA-256. The state output is returned as data; the VM does not mutate
  `goya-state::Store` or perform network, filesystem, clock, RNG, or other
  host I/O.
- `D2Program::decode` restores the canonical program bytes after checking the
  supplied SHA-256, format version, canonical encoding, and exact embedded
  proof. The caller must also supply the original `ProcessDecl`; compilation
  reclassifies it and verifies every effect capability against that
  declaration. Proof metadata alone does not carry capability grants.

## VEE replay worker

`goya-vee::request_from_d2_program` pre-executes the reference VM against the
supplied snapshots and `EffectJournal`, then emits a Python worker carrying
the same instructions and journal. The worker rechecks the journal sequence
and hash chain, requires exact capability and request-byte matches, decodes
only the declared result type, and requires all records to be consumed. It
performs no host effects and the VEE request grants no capabilities. The
adapter bounds the journal to 256 entries/64 KiB, combined embedded payload
to 32 KiB, source to 256 KiB, and worker output to 2 MiB. Focused local Python
tests compare the full output and hashes with the Rust D2 outcome.

The typed source bridge is `goya-lang::CompiledD2Interaction::to_vee_request`.
It verifies authored state/input/output scalar types and the caller-supplied
journal through the Rust VM wrapper before delegating to the bounded VEE
adapter. The wrapper exposes immutable declaration, proof, and capability
labels so callers can build and record an `EffectJournal`; it does not expose
the raw `D2Program`. The method only constructs a `SubmitRequest`. A caller
must explicitly submit it through `VeeClient`, and the request carries no VEE
capabilities or live effect path.

On 2026-11-04, the generated D2 replay worker was submitted through Goya's
`VeeClient` to the local VEE source on loopback with isolated temporary data,
config, and pattern-store paths, authentication disabled only for that
disposable instance, and mDNS disabled. The task reached `Completed`; the
worker replayed the supplied integer result 4, changed `count` from 8 to 12,
and advanced revision 5 to 6. Its program, input, and outcome hashes were
`334527df00d3bd99771c6b7d59cbb87efbee989a4569c7331f47cdc6d5cd185a`,
`d9914bb941607e8e603de0ecc9169b7ff46170d007a2ad8f5b2219e5b163904c`, and
`ed8fe5c14d5e53adbf968467dccf6babb90c013feb7f7bd16e9ff929662db2ea`.
The request granted no VEE capabilities. On that date Landlock setup
failed with `Operation not permitted`; VEE continued with its rlimit sandbox
layer only. A 2026-11-12 rebuild sets `no_new_privs` before
`landlock_restrict_self` and keeps the system interpreter executable; the
repeated D0 and authored D1/D2 loopback smokes then completed with Landlock
enforced on this host. That still depends on the deployed kernel and VEE
build. The D1 result and
full D0/D1/D2 smoke details are in
[goya-vee.md](goya-vee.md#local-d0d1d2-live-smoke). The D0 worker also passed
local submit/status/result validation; VEE retained its expected JSON line in
a text artifact preceded by sandbox diagnostics.

## Bounds and exclusions

- At most 4,096 instructions, 64 KiB per static effect request or typed
  result, 2 MiB encoded program, 1 MiB combined state/input snapshots, and
  1 MiB register data.
- Integer addition and revision increments fail on overflow. Effect results
  with wrong tags, lengths, UTF-8, or trailing bytes fail closed.
- The journal's unkeyed SHA-256 chain checks record linkage and content
  consistency; it does not authenticate a journal rewritten in full.
- This VM does not make effects live or deterministic by itself: a trusted
  recorder must provide results, and a caller must decide whether to apply
  the returned state transaction. This is a narrow D2 replay path, not a
  general-purpose effect scheduler or sandbox.
