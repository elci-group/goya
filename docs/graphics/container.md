# GY-GFXC/1 — Shared graphics container

Status: **Ratified 2026-11-24** (GOYA-GFX-001 Phase 0). The owner accepted
decisions D1–D4 as drafted. D5 (0.x versioning) was left to the default,
which is the rule drafted in §6.
Binary spec version: 0.0.1. The contract number (`/1`) tracks the ratified
semantics. The header version tracks the byte layout.
Reference implementation: `crates/goya-graphics-container`.
Golden fixture: `fixtures/graphics/container/sample.gva`.

All four graphics formats (`.gru`, `.gva`, `.gvi`, `.spc`) use this
envelope. The container defines framing, integrity, versioning and limits.
Format meaning lives in each format's own spec. Phase 0 has not drafted those
yet.

The key words MUST, MUST NOT, SHOULD and MAY are normative.

## 1. Encoding

- All integers are little-endian.
- A file is laid out as `header (80 bytes) ‖ directory (count × 72 bytes) ‖ sections`.
- There is exactly one valid layout per logical content (§4). Equal inputs
  MUST produce byte-identical files.

## 2. Header (80 bytes)

| Offset | Field | Type | Rule |
|---|---|---|---|
| 0 | Magic | 4 bytes | `GRU\0`, `GVA\0`, `GVI\0` or `SPC\0`. A decoder MUST reject any other format's magic (E1002/E1003). |
| 4 | Major | u16 | See §6. |
| 6 | Minor | u16 | See §6. |
| 8 | Patch | u16 | See §6. |
| 10 | Header length | u16 | MUST be 80 (E1005). |
| 12 | Feature flags | u32 | Bits 0–15 are *required* features: a reader MUST reject a set bit it does not know (E1006). Bits 16–31 are *advisory*: a reader MAY ignore them. No bits are defined in 0.0.x. |
| 16 | Total length | u64 | MUST equal the file length. Larger means truncated (E1001); smaller means trailing data (E1008). |
| 24 | Directory offset | u64 | MUST be 80 (E1009). |
| 32 | Section count | u32 | MUST NOT exceed the reader's limit (E4002). |
| 36 | Profile | u32 | `0` = Baseline (§7). A reader MUST reject unknown profiles (E1007). |
| 40 | Envelope digest | 32 bytes | §5. |
| 72 | Reserved | 8 bytes | MUST be zero (E1013). |

## 3. Directory entry (72 bytes)

| Offset | Field | Type | Rule |
|---|---|---|---|
| 0 | Tag | 4 bytes | Each byte is `A–Z` or `0–9` (E1014). |
| 4 | Flags | u32 | Bit 0 = MANDATORY. Other bits MUST be zero (E1013). |
| 8 | Offset | u64 | MUST equal the canonical offset (§4; E1009). |
| 16 | Stored length | u64 | Length of the bytes in the file. |
| 24 | Decoded length | u64 | For codec 0, MUST equal the stored length (E1009). |
| 32 | Codec | u16 | `0` = stored. No other codec is defined in 0.0.x. |
| 34 | Alignment | u8 | log2 of the alignment, from 3 to 12 (8 B to 4 KiB; E1015). |
| 35 | Reserved | u8 | MUST be zero. |
| 36 | Section version | u16 | Owned by the format spec. |
| 38 | Reserved | u16 | MUST be zero. |
| 40 | Section digest | 32 bytes | SHA-256 of the section's stored bytes. |

Entries MUST be in strictly ascending byte order of their tags, with no
duplicates (E1010).

A reader receives the set of tags it understands:

- An unknown section with MANDATORY set MUST cause rejection (E1011).
- An unknown optional section MAY be skipped. A format spec MUST NOT give
  an optional section any power to change required rendering semantics.
- A MANDATORY section whose codec the reader cannot decode MUST cause
  rejection (E1012). An optional one MAY be skipped.

## 4. Canonical layout

The cursor starts at `80 + 72 × count`. For each entry, in directory order:

1. `offset = align_up(cursor, 2^alignment)`
2. `cursor = offset + stored_length`

The total length MUST equal the final cursor, so the file ends exactly at
the end of the last section. All bytes between sections (padding) MUST be
zero (E1018).

A decoder MUST do this arithmetic with checked operations. Overflow is
E1019, and a section ending past the file is E1001. These checks let the
format reject gaps, overlaps, hidden bytes, reordering and garbage padding.

## 5. Integrity

- **Envelope digest** = SHA-256 over `header ‖ directory`, computed with
  the digest field (offsets 40–71) set to zero. A mismatch is E1016.
- **Section digest** = SHA-256 of each section's stored bytes, held in the
  directory. A mismatch is E1017.

The envelope digest covers every section digest and the layout rules fix
the padding, so the envelope digest commits to every byte of the file. It
is the file's content identity. The two digests serve two access modes:

- **Partial or memory-mapped access:** verify the envelope (O(directory
  size)), then verify each section only when it is first used.
- **Full verification:** verify the envelope, every section and every
  padding byte.

A digest is integrity evidence only. It MUST NOT be treated as proof of
authenticity. Authenticity needs a separate signature or signed package
manifest (GY-ARTIFACT/1 or successor).

## 6. Versioning

- Writers emit 0.0.1.
- While the major version is 0, a reader MUST reject any file whose
  `major.minor` differs from its own (E1004). Any patch value is accepted.
- Patch releases MUST NOT change the layout or the meaning of any field.
- From 1.0 onward, the intended rule is: same major, and minor ≤ the
  reader's minor. New minor versions may add optional sections, advisory
  features or profiles. This rule will be ratified at 1.0.

## 7. Baseline profile (id 0)

Ratified with this spec (D3). It sets these numeric conventions for the
formats' section contents:

- **Coordinates:** signed integer millipoints, encoded as
  `round_half_even(1000 × x)`. This is the same unit as GY-LAYOUT/1
  (1 pt = 1000 mp). Geometry is in the unit's own local space.
  Placement in a scene stays parent-relative (`position_p`/`extent_p`).
- **Colour:** sRGB-encoded, 8 bits per channel. Alpha is straight (not
  premultiplied) at rest. Premultiplication, linearisation and blending
  belong to the backend, which must declare them.
- **Transforms:** row-vector affine `[x y 1]·M`, the same convention as SVG
  `matrix(a,b,c,d,e,f)`: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
  The linear part `a–d` is Q16.16 fixed point and `e, f` are in
  millipoints. Composition runs from instance to parent.
- **Floating point:** no float is stored in Baseline sections. A
  higher-precision profile (for CAD, scientific or large spatial work) will
  get its own id.

## 8. Resource limits

A decoder MUST check limits before it allocates or reads section bytes. A
writer MUST refuse to produce a file that the same limits would reject.

| Limit | Default | Error |
|---|---|---|
| File length | 256 MiB | E4001 |
| Section count | 64 | E4002 |
| Stored or decoded section length | 64 MiB | E4003 |

Targets MAY tighten or loosen these through their capability manifest.

## 9. Error codes

| Code | Meaning |
|---|---|
| E1001 | Truncated: shorter than the header, the declared length, or a section's extent |
| E1002 | Bad magic |
| E1003 | Valid magic for a different format |
| E1004 | Unsupported version |
| E1005 | Header length is not 80 |
| E1006 | Unknown required feature bit |
| E1007 | Unknown profile |
| E1008 | Trailing bytes past the declared length |
| E1009 | Non-canonical layout: directory offset, section offset, trailing bytes, or stored ≠ decoded for codec 0 |
| E1010 | Directory not strictly ascending, or a duplicate tag |
| E1011 | Unknown mandatory section |
| E1012 | Unsupported codec on a mandatory section, or on a section being fetched |
| E1013 | Reserved bits or bytes are not zero |
| E1014 | Invalid tag |
| E1015 | Alignment outside 2^3 to 2^12 |
| E1016 | Envelope digest mismatch |
| E1017 | Section digest mismatch |
| E1018 | Padding is not zero |
| E1019 | Arithmetic overflow in offset or length |
| E4001 | File over the length limit |
| E4002 | Too many sections |
| E4003 | Section over the length limit |

Every diagnostic carries its code, the affected section tag when there is
one, a detail message and a remediation hint. The originating `.gscene`
location and artifact identity are added by the compiler layer that calls
the container.

## 10. Conformance

The reference crate's tests are this spec's conformance suite:

- round trip
- order-independent determinism
- golden bytes
- distinct magic per format
- each header and directory violation, mapped to its code
- every truncation length
- every single-byte flip, which must be detected by `open` or by `verify_all`
- lazy per-section verification
- padding checks
- 20,000 rounds of deterministic mutation fuzzing, with no panics

An independent implementation MUST reproduce
`fixtures/graphics/container/sample.gva` byte for byte, from the inputs in
`tests.rs::sample`.
