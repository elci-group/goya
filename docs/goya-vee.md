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

`request_from_compiled_program` accepts only artifacts whose proof class is
D0 and whose bytecode passes the reference VM's version, hash, and structure
checks. It produces a deterministic `python` request containing a Python
assignment to `GOYA_EDC_INSTRUCTIONS`: a tuple of inert instruction data.
Strings are escaped as Python literals, and numeric values (including spacer
IEEE-754 bits) are emitted as integers. The bridge caps the instruction count,
aggregate text/label bytes, generated source size, and agent ID length. It sets
an empty capability list and a small fixed budget.

This is a data export, not a Python implementation of Goya behavior. VEE's
Python runner will receive data declarations only; this does not cause the
Goya UI bytecode to execute or render. Goya bytecode is not WebAssembly, and
the adapter never labels it as WASM. No shell, subprocess, or host capability
is requested by the generated source. Callers may continue to use
`SubmitRequest` directly for independently authored supported-language
source.

## Verification scope

Unit tests verify request JSON, endpoint path, bearer header, success/error
response handling, and the D0 conversion using a local one-shot HTTP fixture.
They do not exercise a running VEE server; no live server was used or
confirmed for this change. The fixture is not represented as an end-to-end
VEE integration.
