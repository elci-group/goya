# ADR-IE-001 — Freeze Iberia/España shared contract v0.1 (`crates/ie-contract`)

Status: Accepted (G1 freeze) | Date: 2026-11-18 | Owner: S0 contracts | Supersedes: none

## Context
The PAMPER anthology (Iberia/España baseline 0.0.1) proposed a shared protocol
and conformance contract (IE-X01). G0 inventory recorded that no contract
crate existed and that all names/tolerances were proposals. The owner
approved freezing contract v0.1 at G1. Per IE-E01, no implementation work
(S1/S3/S4) may begin against anything other than these frozen mock
interfaces.

## Decision
Add `crates/ie-contract` (no implementation dependencies, stdlib only) as the
single source of the shared contract:

- **Typed error model** (IE-A01): UNSUPPORTED_OP, INVALID_TENSOR, POLICY_DENIED,
  UNTRUSTED_PEER, MODEL_MISSING, RESOURCE_EXHAUSTED, DEADLINE_EXCEEDED,
  CANCELLED, PEER_LOST, VERSION_MISMATCH, INTERNAL. Wire codes are stable;
  new variants require a contract revision.
- **Framing**: 4-byte little-endian length prefix, MAX_MESSAGE_BYTES = 16 MiB.
  Oversized declarations are rejected before any allocation.
- **Tensors**: static shapes, FP32/INT8 only, little-endian row-major,
  MAX_RANK 8, MAX_TENSOR_BYTES 64 MiB; checked shape-product arithmetic;
  payload length validated against the descriptor before retention.
- **Messages**: CapabilityDescriptor (endpoint ID, epoch, expiry), Request
  (model SHA-256, request ID, remaining-duration deadline, lease, policy
  digest), Response (actual backend, epoch, measured timings, typed error).
- **Endpoint lifecycle**: Discovered → Authenticated → Ready (only Ready
  admits jobs; Degraded requires policy approval).
- **Golden oracle**: FP32 reference graph fixtures with abs tol 1e-5 /
  rel tol 1e-4 (reference fixture only), frozen in
  `fixtures/golden_t01_matmul.golden`; conformance seeds T01–T03 in
  `tests/conformance.rs`.

## Consequences
- Downstream swarms (S1 CPU backend, S3 España transport, S4 Goya gate)
  implement against this crate only; changes require a new ADR and
  compatibility review before integration (IE-E01 work-packet protocol).
- Wire canonical encoding beyond framing (field-level encoding of
  Request/Response) is intentionally **not** frozen yet; the framing and
  error-code layer is. Full canonical encoding + test vectors must exist
  before wire freeze of transports (IE-X01).
- The G0 record (isolated extraction at `/home/sal/zipwork_iberia/g0/`)
  stands as the pre-approval inventory evidence.
