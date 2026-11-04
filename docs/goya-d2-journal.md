# GY-D2 Journal/1 — Deterministic Host-Effect Records

`goya-executable::EffectJournal` provides bounded recording and replay for
caller-mediated D2 host effects. The module stores request and result bytes,
labels each record with a granted capability, and links entries with a
SHA-256 hash chain. It performs no host I/O: live code must obtain an effect
result elsewhere and pass the bytes to `record`; replay compares the exact
capability and request and returns the previously stored result.

## Admission and bounds

- A journal is created only for a `goya-determinism` declaration classified
  as D2 whose `ExecutionProof` matches its process name and determinism class.
- Every grant must name a declared effect (`network`, `filesystem`, `clock`,
  `random`, or `global_mutable`). Runtime records must use a granted label.
- Labels are lowercase ASCII using letters, digits, `.`, `_`, `:`, and `-`,
  with a 128-byte limit.
- Defaults cap a journal at 10,000 entries, 64 KiB per request/result, and
  8 MiB of encoded entry data. Configurable limits cannot exceed hard caps of
  100,000 entries, 1 MiB per field, and 32 MiB total.
- Sequence numbers begin at zero. Each entry commits to its sequence,
  capability, request, result, and previous SHA-256 digest. Restore verifies
  the complete chain and all bounds before replay is available.
- Replay is sequential. An out-of-order or changed request fails without
  advancing the cursor; `finish` rejects a replay that leaves entries unused.

## Security and scope

The hash chain detects inconsistent links and accidental changes when
replaying a stored journal. It is **unkeyed**: it does not authenticate an
origin, and an attacker who can rewrite the entire journal can recompute the
chain. Signed manifests, trusted storage, and external effect execution are
outside this contract.

This is a D2 journaling primitive, not general D2 bytecode execution. The
current bytecode reference VM executes pure D0 modules only. The journal does
not schedule effects, invoke capabilities, sandbox a host, or make an
otherwise nondeterministic effect deterministic; callers must define those
policies and provide the observed request/result bytes.
