# Goya VEE adapter

`goya-vee` submits JSON to `POST /vee/submit`, matching the local VEE request
contract in `vico-vee/src/types/api.rs`. Requests carry `agent_id`, `language`,
`source_code`, `capabilities`, and an optional budget with VEE's five optional
limits. Authentication is optional and uses `Authorization: Bearer <token>`.
VEE requires the `submit` scope when authentication is enabled.

Submit responses provide `success`, `execution_id`, and `status`, with
optional `message` and `error`. The client parses JSON structure and escaping,
rejects duplicate keys and malformed types, and maps `success: false` to an
application rejection.

## Execution status

The local VEE implementation at
`/home/sal/vico-desktop/src-tauri/vico-vee/src/server.rs` registers
`POST /vee/status`. It accepts `{"execution_id":"..."}` and returns
`{"success":true,"data":ExecutionResult}` or
`{"success":false,"error":"Execution not found"}`. `ExecutionResult` is
defined in `src/types/result.rs`; it contains the execution ID, project ID,
status, phase, artifacts, validation, confidence and resource metrics, error
log, and timestamps. Its status and phase enums are the exact PascalCase
variants represented by `VeeExecutionState` and `VeeExecutionPhase` in this
crate. Artifacts and validation remain typed JSON values because their schemas
are polymorphic.

`VeeClient::status` submits that request and returns a typed snapshot. Response
parsing is dependency-free and bounded to the existing 4 MiB HTTP body limit,
64 JSON nesting levels, and 100,000 JSON values. It rejects duplicate object
keys, invalid escapes/surrogates, malformed numbers, and unknown status/phase
names. The local HTTP fixture verifies the request and typed result mapping.
No live server was used or confirmed; deployed API behavior should still be
checked against the local source contract.

## Compiled Goya programs

`request_from_compiled_program` accepts only artifacts whose proof class is
D0 and whose bytecode passes the reference VM's version, hash, and structure
checks. It produces a deterministic `python` request containing a bounded
instruction tuple and a small interpreter. The interpreter builds the same
tree shape as the Rust reference VM and prints compact JSON to stdout, with
spacer values represented by their IEEE-754 bit patterns. The bridge caps the
instruction count, aggregate text/label bytes, generated source size, and
agent ID length. It sets an empty capability list and a small fixed budget.

The generated source imports only Python's standard-library `json` module,
and performs no file, network, subprocess, or shell operations. Goya bytecode
is not WebAssembly, and the adapter never labels it as WASM. The local Python
test compares its stdout with the Rust reference VM's tree serialization.
The D0 VEE path is also covered by the opt-in live smoke below. Its JSON is
returned in the completed execution's text artifact; VEE may prefix sandbox
diagnostics in that same stdout field, so consumers should locate/parse the
worker output line rather than assume stdout contains only that JSON. Callers
may continue to use `SubmitRequest` directly for independently authored
supported-language source.

`request_from_d1_program` accepts a `D1Program` handle produced by the bounded
canonical decoder (or compiler), plus explicit `D1Snapshot` and
`InputSnapshot` values. It first runs the Rust VM to validate the exact schema
and execution, then generates a Python worker implementing only the validated
constant, state-read, input-read, checked-add, and state-write operations.
The worker repeats exact schema and size checks, emits typed state/register
values and the same program/input/outcome hashes as the Rust VM, and requests
no VEE capabilities. A local Python execution test compares its complete JSON
output against the Rust outcome. This uses the existing VEE `language=python`
submit contract; a local live service accepted the worker and retained its
output as recorded below.

`request_from_d2_program` accepts a validated `D2Program`, explicit state and
input snapshots, and the caller-supplied `EffectJournal`. Rust executes the
reference VM first, requiring exact ordered capability/request matches and
complete journal consumption. The generated Python worker independently
checks the journal's sequence and SHA-256 chain, replays only those exact
recorded results, checks typed result encodings, and rejects unused entries.
It imports only `hashlib` and `json`, has no host effect path, and requests no
VEE capabilities. The adapter caps the journal at 256 entries/64 KiB, the
combined data payload at 32 KiB, and the generated source at 256 KiB. A local
Python execution test compares the complete typed output and hashes with the
Rust D2 outcome; a local live service accepted the worker and retained its
output as recorded below.

## Verification scope

Unit tests verify request JSON, endpoint path, bearer header, success/error
response handling, and the D0 conversion using a local one-shot HTTP fixture.
The fixture is not represented as an end-to-end VEE integration.

### Local D0/D1/D2 live smoke

The D0 live smoke is opt-in: run the unit test
`d0_live_submit_status_and_result_when_enabled` with
`GOYA_VEE_LIVE_SMOKE=1`. It defaults to `127.0.0.1:18997`; optional
`GOYA_VEE_HOST` and `GOYA_VEE_PORT` overrides are accepted only when the host
is a loopback literal. On 2026-11-04, a disposable local VEE instance accepted
the generated capability-free D0 worker, returned `Completed`, and retained
the reference-VM-equivalent JSON line
`{"kind":"text","node":1,"content":"D0 live result"}` in its stdout
text artifact. VEE prefixed Landlock warnings in that artifact and labeled it
`Plain`, because the diagnostics preceded the JSON. D0 result semantics are
now covered alongside D1/D2; sandbox isolation still fell back to rlimits
only in this environment.

On 2026-11-04, the locally checked out VEE source at
`/home/sal/vico-desktop/src-tauri/vico-vee` was built with its validated
`baby` recipe and run on `127.0.0.1:18997`. The smoke used fresh temporary
data, config, and pattern-store paths; disabled authentication only for this
disposable loopback server; and disabled mDNS. Goya's `VeeClient` submitted
the generated D1 and D2 Python worker sources and polled each status to
`Completed`.

- D1 task `vee-551a9da9-cae1-4d08-a43f-e8b2acd44a73`: state `count` changed
  from 7 to 10, revision 4 to 5. Worker hashes: program
  `1cde920996ab175d27f8f5b777a934338e1c88ae8f429d529f4dece1eb4a72a6`, input
  `1abb9ca811942180a3acc28e8b09f6abca7c41f7737bd6f65bf5a4fdbc3a21c3`,
  outcome `161fa724289ac51ce02ff074425d037562fff3d7e178bb3ebe7a7136ecfea495`.
- D2 task `vee-5a88e05d-8fff-4b5e-8ea6-dee35b999b7a`: replayed its supplied
  journal result (4), changing `count` from 8 to 12, revision 5 to 6. The
  request had an empty VEE capability list. Worker hashes: program
  `334527df00d3bd99771c6b7d59cbb87efbee989a4569c7331f47cdc6d5cd185a`, input
  `d9914bb941607e8e603de0ecc9169b7ff46170d007a2ad8f5b2219e5b163904c`,
  outcome `ed8fe5c14d5e53adbf968467dccf6babb90c013feb7f7bd16e9ff929662db2ea`.

Both returned worker payloads contained the expected typed state/register
values and hashes. In this environment VEE logged that Landlock setup failed
with `Operation not permitted` and continued with the rlimit sandbox layer
only. This confirms the local submit/status/result path and worker behavior
under that environment; it does not establish production Landlock or other
production sandbox coverage, nor validate remote deployment configuration.

The typed authored-source path has a separate opt-in acceptance in
`goya-lang`:

```sh
GOYA_VEE_COMPILED_LIVE_SMOKE=1 \
GOYA_VEE_HOST=127.0.0.1 GOYA_VEE_PORT=18997 \
cargo test -p goya-lang compiled_source_live_vee_submit_when_enabled -- --nocapture
```

On 2026-11-04 this test compiled the authored D1 and static-journal D2 source
fixtures, generated requests through the typed wrappers, submitted both to a
disposable loopback VEE instance, and verified their stdout artifacts contained
the expected next state (`count` 12 for D1, `count` 4 for D2). The D2 request
used only the caller-provided journal and granted no VEE capabilities. The
instance used isolated temporary data/config/pattern-store paths, disabled
authentication only for loopback, and disabled mDNS. Landlock setup returned
`Operation not permitted`, so this test verifies worker behavior under VEE's
rlimit fallback only; it does not establish production sandbox coverage.
