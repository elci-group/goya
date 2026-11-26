# GY-FAIR/2 — Foreign Application Intermediate Representation, revision 2

Status: **Candidate.** The owner approved the draft on 26 November 2026, and it
was implemented the same day in `crates/goya-fair`
(`FAIR_CONTRACT = "GY-FAIR/2"`). It supersedes [GY-FAIR/1](fair-1.md), which
stays readable (§4). Extraction-only authority is unchanged: FAIR→Goya
reconstruction is still withheld (GY-ENT-001 Phase 4).

Origin: owner decision 1 in [DELECTABLE-0](../../delectable/DELECTABLE-0.md)
folds Delectable's evidence graph (DEG) and intermediate representation (DIR)
into FAIR rather than creating new formats. Every change below answers
something FAIR/1 could not hold when `goya-disparate-electron` ran on the
DEL-B01 fixture.

## 1. What FAIR/1 could not hold

| Gap | Evidence (DEL-B01, extractor v0.1.0) |
| --- | --- |
| No edges. Elements could not point at each other. | The two sides of `counter:add` were linked only by an equal `channel` value. A component named its parent inside an untyped payload. |
| Kinds missing for common constructs. | 7 of 39 elements were `unknown` with `NO_FAIR_KIND`: 3 module imports, 2 functions, 1 bridge call, 1 package manifest. |
| The contract promised a field the code lacked. | fair-1.md §Provenance requires "framework detections with detection evidence, entry points". `FairDocument` had no field for either. |
| No written reason-code register. | fair-1.md required codes "from a versioned register". No register existed, and validation checked only the form. |
| No certainty separate from progression. | DEG §8.3 separates how a fact is known from how far conversion has gone. FAIR had only the second. |

## 2. Changes

1. **Relationships.** `FairDocument.relationships: Vec<FairRelationship>`.
   Each one has `id`, `subject`, `relation`, `object`, `basis`, `evidence`
   and `provenance`.
   * `relation` comes from a closed vocabulary: `contains`, `invokes`,
     `handles`, `exposes`, `reads`, `mutates`, `loads` and `styles`.
   * `basis` says how the relationship is known. It is `extracted` when one
     source site shows it, `derived` when it follows deterministically from
     other elements or relationships, and `observed` when it was recorded
     during replay (Phase 3 only).
   * `evidence` lists the element or relationship IDs that a `derived`
     relationship rests on. It is required for `derived` and may be empty
     otherwise.
2. **Kinds.** `module_import`, `logic_unit` (a function or method kept so that
   unclassified logic is never lost), `bridge_call` and `package_manifest`.
   Every FAIR/1 kind keeps its name.
3. **Project record.** `FairDocument.project` holds `entry_points` and
   `framework_detections`.
   * An entry point has a `path`, which must be in the manifest, a `role`,
     and `evidence` (element IDs).
   * A framework detection has a `framework`, a `version` where the source
     states one, and `evidence`.
4. **Register.** [reason-codes.md](reason-codes.md), mirrored by
   `goya_fair::REASON_CODES`. A test keeps the two equal. Under FAIR/2,
   validation refuses any code that is not registered.
5. **Certainty.** Certainty is carried by `basis` on relationships. Elements
   keep the frozen seven-state progression. FAIR/2 has **no `inferred`
   basis**. Whether LLM-proposed assertions may appear at all is still owner
   decision 3 in DELECTABLE-0. Until that is decided, hypotheses live outside
   FAIR.

## 3. Validation rules added

| Code | Rule |
| --- | --- |
| `DANGLING_RELATIONSHIP` | `subject` and `object` must be element IDs. |
| `DUPLICATE_RELATIONSHIP_ID`, `EMPTY_RELATIONSHIP_ID` | Relationship IDs are non-empty and unique. |
| `MISSING_EVIDENCE` | A `derived` relationship, an entry point and a framework detection each name evidence. |
| `DANGLING_EVIDENCE` | Evidence IDs exist. A relationship may not cite itself. Project evidence must be element IDs. |
| `ENTRY_POINT_NOT_IN_MANIFEST`, `EMPTY_ROLE`, `EMPTY_FRAMEWORK` | The project record refers to real files and names its roles. |
| `UNREGISTERED_REASON_CODE` | FAIR/2 documents use registered codes only. |
| `FAIR2_FEATURE_IN_FAIR1` | A FAIR/1 document may not use relationships, the project record or FAIR/2 kinds. |

Canonical output sorts relationships by ID, entry points by (path, role),
detections by framework, and every `evidence` list. An empty relationship
list and an empty project record are omitted.

## 4. FAIR/1 compatibility

* FAIR/1 documents still deserialise and validate under FAIR/1 rules. Reason
  codes are checked for form only.
* Their canonical bytes are **unchanged**. The FAIR/1 fixture round-trips
  byte for byte (`fair1_fixture_round_trips_byte_for_byte`).
* `FairDocument::upgrade` converts a FAIR/1 document to FAIR/2. It fails if a
  reason code is outside the register.
* A FAIR/1 reader still on `main` or `arachnid/v0.1` rejects a FAIR/2 document
  at `validate` with `CONTRACT_MISMATCH`. It does not silently drop the
  relationships.

## 5. Decided during implementation

These refine the approved draft without changing its intent:

* **`evidence` on relationships.** The draft defined `derived` as following
  "from other relationships" without saying where those are recorded. FAIR/2
  records the element or relationship IDs explicitly, so that every
  assertion stays traceable (DEG §8.1).
* **Empty fields are omitted** rather than serialised as `[]` or `{}`. This
  is what keeps FAIR/1 bytes stable.
* **The FAIR/1 examples are registered.** The register includes the three
  codes fair-1.md gives as examples (`DYNAMIC_REQUIRE_UNRESOLVED`,
  `NATIVE_MODULE_OPAQUE`, `NO_GOYA_CAPABILITY_CHILD_PROCESS`), so
  documents that follow fair-1.md's own examples upgrade cleanly.

## 6. Not in FAIR/2

* **No separate DIR.** The FAIR→`.gscene` boundary is still a future contract.
* **No graph database.** Relationships are a list in one canonical JSON
  document, as elements are.
* **No `inferred` basis** (§2 item 5).
