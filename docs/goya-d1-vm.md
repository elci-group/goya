# GY-D1-VM/1 — Deterministic State Transitions

`goya-executable::D1Program` is a bounded state-transition VM for process
declarations classified as D1 by `goya-determinism`. Compilation requires an
explicit state snapshot contract and an `ExecutionProof` with the same
process name and D1 class. D1 declarations with host effects are rejected.

## Execution model

- A `StateContract` explicitly lists state keys that may be read, state keys
  that may be written, and deterministic input keys.
- Each execution receives immutable `D1Snapshot` and `InputSnapshot` values.
  The snapshot key sets must exactly match the contract's read/write and
  input sets, preventing undeclared environment reads.
- The current instruction set supports constants, state/input reads,
  checked integer addition, and state writes. Registers are local to one
  execution. There are no loops, clocks, RNG, system calls, or I/O.
- Execution returns a new state map and increments its revision once. It
  never mutates a store. The caller may apply the result transactionally.
- `D1Program::apply_to_store` is the optional store adapter: it reserves one
  transaction, captures the declared state keys with their revision, executes
  with explicit inputs, stages only declared writes, and commits once against
  a caller-supplied invariant. Busy stores, missing declared keys, VM errors,
  and rejected commits leave committed state unchanged. The store revision
  and state snapshot are captured atomically.
- Program encoding is stable: fixed opcodes, little-endian integers,
  length-prefixed strings, and sorted contract keys. SHA-256 hashes bind the
  program, input snapshot, and output state/registers. `replay` recomputes and
  checks the expected outcome hash.

## Bounds and exclusions

## Typed GY-LANG bridge

`goya-lang::CompiledInteraction::to_vee_request` checks the source-authored
state/input/output scalar types, then builds the existing bounded VEE worker
request from its private verified `D1Program`. It returns a `SubmitRequest`;
it does not connect to VEE or submit anything. This preserves the source type
contract without exposing the raw VM program. The resulting worker still
applies the D1 VM's exact key-set, arithmetic, and outcome-hash checks.

- At most 256 distinct contract/snapshot keys, 4,096 instructions, 64 KiB per
  text value, 1 MiB combined state/input snapshots, 2 MiB encoded program,
  and 1 MiB register data per execution.
- Integer addition and revision increments fail on overflow.
- This is a local deterministic transition model. It does not integrate with
  `goya-state::Store`, persist snapshots, authenticate proof signatures, or
  implement all of Rust or a general-purpose language. D2 effects are handled
  by the separate journal primitive and are not executed by this VM.
- SHA-256 outcomes are unkeyed integrity identifiers, not attestations from a
  trusted signer. A caller that accepts stored outcome hashes must establish
  trust in their source separately.
