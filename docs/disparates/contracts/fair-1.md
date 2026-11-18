# GY-FAIR/1 — Foreign Application Intermediate Representation

Directive: GY-ENT-001 (Disparates programme, revision of 2026-11-18). Date: 2026-11-18.
Status: Candidate. Extraction-only authority; no reconstruction is authorised under this revision.

## Purpose

FAIR is the foundation of the Disparates programme. It is the common intermediate
representation produced by semantic extraction of foreign applications and consumed by
representability analysis, coverage reporting and — under a later, separately authorised
revision — Goya reconstruction.

FAIR exists independently of successful migration. A FAIR document is a structured,
reproducible architectural inventory of a foreign application even when no element is
representable in Goya.

## Provides

- `crates/goya-fair` — schema, validation and deterministic serialisation.
  `pub const FAIR_CONTRACT: &str = "GY-FAIR/1"`.
- `FairDocument` — top-level document: schema version, source provenance, extractor
  identity, and an element list.
- `FairElement` — one extracted foreign construct: stable element ID, element kind,
  source provenance, representability status, and an open semantics payload.
- `Representability` — the seven-state coverage classification (see below).
- `validate(&FairDocument) -> Result<(), Vec<ValidationError>>` — structural validation.
- `canonical_json(&FairDocument) -> String` — deterministic serialisation.

## Consumes

None. `goya-fair` is serde-only and depends on no other Goya crate. FAIR does not
import, reference or embed `goya-ir` types. `goya-ir` remains free of foreign framework
semantics; the FAIR→Goya boundary is a future contract and is not defined here.

## Element kinds

Element kinds are an open, versioned vocabulary. Known kinds (component, hook, event
handler, ipc channel, preload bridge, os_api_call, child_process, filesystem_operation,
window_lifecycle, native_module, dynamic_module_resolution, state_store, style_rule,
…) are enumerated in the schema. An extractor that observes a construct outside the
vocabulary must emit `kind: "unknown"` with the observed spelling recorded in the
semantics payload. Inventing an unversioned kind is prohibited; the vocabulary grows by
contract revision only.

## Representability states

Representability is an evidence progression, not a progress percentage. The states are
ordered and every element carries exactly one:

| State                 | Meaning                                              |
|-----------------------|------------------------------------------------------|
| `extracted`           | Foreign construct identified and recorded            |
| `representable`       | Existing Goya semantics can express it               |
| `translated`          | Native Goya source generated                         |
| `verified`            | Behavioural equivalence demonstrated                 |
| `remediation_required`| Explicit intervention necessary                      |
| `unrepresentable`     | Required Goya capability absent                      |
| `undetermined`        | Insufficient evidence to establish semantics         |

Rules:

- An element may not be marked `translated` without generated Goya source recorded
  against it. An element may not be marked `verified` on the basis of compilation or
  translation alone. Verification requires replay evidence produced under a future
  equivalence contract; this revision cannot produce `verified`.
- Every state other than `extracted` and `undetermined` requires a machine-readable
  reason code. `undetermined` requires one too. Reason codes are uppercase snake case
  from a versioned register (for example `DYNAMIC_REQUIRE_UNRESOLVED`,
  `NO_GOYA_CAPABILITY_CHILD_PROCESS`, `NATIVE_MODULE_OPAQUE`); free-form detail goes in
  the `detail` field, not the code.
- Unresolved constructs must be preserved, never dropped. A dynamic `require()` whose
  destination cannot be resolved is recorded as
  `kind: dynamic_module_resolution`, `resolution: unresolved`, representability
  `undetermined`. **FAIR must preserve ignorance**: an extractor records what it does
  not understand rather than omitting it, so later extractor improvements are honest
  revisions, not silent rewrites.

## Provenance

Every element carries `file` (workspace-relative path in the foreign project) and
`line` (1-based). A byte span (`span_start`, `span_end`, end-exclusive UTF-8) is
required where the extractor has it and must be absent where it does not; fabricated
spans are prohibited. Origins are attributed to the most precise available enclosing
location, and fallback origins are marked, following the GSSH/frontend-handoff
convention.

Document-level provenance records the foreign project identity: framework detections
with detection evidence, entry points, and the extractor version. FAIR documents are
bound to foreign source bytes by a SHA-256 source manifest; a FAIR document without a
source manifest is invalid.

## Determinism

- Serialisation is canonical: fixed field order, sorted map keys, elements sorted by
  ID, no trailing whitespace, UTF-8, trailing newline. Byte-identical extraction from
  byte-identical input and extractor version is a hard requirement.
- Element IDs are extractor-assigned, stable and unique within a document; the
  assignment rule is the extractor's contract and is recorded in document provenance.
- Extraction must be fully deterministic and offline. No LLM inference participates in
  extraction, validation or serialisation; LLM assistance is confined to later
  remediation-proposal stages and is always subordinate to deterministic verification.
- Foreign application code is never executed during extraction. Extraction is static.

## Coverage reporting

Coverage reports are derived exclusively from element-level representability states.
Aggregate percentages may be displayed but are never the unit of truth, are never
persisted as authoritative, and are always accompanied by the per-element report. A
migration may not be declared successful on the basis of a compiling result;
`verified` states with replay evidence are the only success basis, and no such
evidence exists under this revision.

## Invariants

- `goya-fair` contains no `goya-ir` types and no foreign framework runtime code.
- Validation rejects: duplicate element IDs, missing reason codes where required,
  missing source manifests, fabricated spans, non-canonical field values.
- Canonical serialisation of the same document is byte-stable across runs and platforms.
- No element is silently omitted: extraction completeness is evidenced by the coverage
  report's `undetermined` and `unrepresentable` populations, which must be present
  where they exist.

## Stability

Candidate. The element-kind vocabulary and reason-code register may grow by contract
revision; existing codes and kinds are never renamed or renumbered. The seven-state
representability enumeration is frozen at v1. Reconstruction authority (FAIR→Goya) is
explicitly withheld pending demonstrated representational feasibility on a reference
corpus.
