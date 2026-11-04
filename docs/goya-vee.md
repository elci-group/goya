# Goya VEE adapter

`goya-vee` submits JSON to `POST /vee/submit`, matching the local VEE request
contract in `vico-vee/src/types/api.rs`. Requests carry `agent_id`, `language`,
`source_code`, `capabilities`, and an optional budget with VEE's five optional
limits. Authentication is optional and uses `Authorization: Bearer <token>`.
VEE requires the `submit` scope when authentication is enabled.

The response contract provides `success`, `execution_id`, and `status`, with
optional `message` and `error`. The std-only client retains permissive optional
field scanning and the raw response body because it does not use a JSON parser;
it returns an application rejection when `success` is explicitly `false`.

## Compiled Goya programs

The adapter accepts caller-provided source text and a VEE language label. It
does not convert `CompiledProgram` into a VEE artifact. Goya bytecode is not
WebAssembly, so sending its bytes with `language: "wasm"` would be a false
format claim and is unsupported. Executing compiled Goya programs through VEE
requires a real bytecode-to-supported-language compiler or a Goya runtime
integrated as a VEE worker. Neither conversion nor worker exists in this
checkout. Until one is implemented, callers must submit source code in a
language VEE actually supports; this adapter makes no execution claim for
Goya bytecode.

## Verification scope

Unit tests verify request JSON, endpoint path, bearer header, and success/error
response handling using a local one-shot HTTP fixture. They do not exercise a
running VEE server. A live server is not required by the adapter crate tests;
the fixture is not represented as an end-to-end VEE integration.
