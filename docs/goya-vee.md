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
The VEE submission path is prepared, but no live VEE server was used or
confirmed for this change, and the interpretation of Python stdout/result
fields by the remote VEE task API still needs confirmation. Callers may
continue to use `SubmitRequest` directly for independently authored
supported-language source.

## Verification scope

Unit tests verify request JSON, endpoint path, bearer header, success/error
response handling, and the D0 conversion using a local one-shot HTTP fixture.
They do not exercise a running VEE server; no live server was used or
confirmed for this change. The fixture is not represented as an end-to-end
VEE integration.
