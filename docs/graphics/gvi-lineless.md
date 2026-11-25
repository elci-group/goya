# GVI-L — Style palette in `.gvi`

Status: **Draft**, inside GY-GVI/0. It chooses which style-pool section
an inventory carries. The runtime API is unchanged.
Implementation: `crates/goya-gvi/src/lineless.rs`.
Normative companion: [gvi.md](gvi.md) §4.3.

A GVI style pool is a list of GVA style records ([gva.md](gva.md) §4).
Records that share a fill or a stroke repeat those four colour bytes.
GVI-L stores each distinct colour once and replaces the colour fields
with indexes.

## 1. When it is used

The compiler encodes the same style list as both `STYL` and `LSTY`.
It emits `LSTY`, and sets `CERT` assumptions bit 4
(`ASSUME_LINELESS_STYLES`), only when the `LSTY` payload is strictly
shorter than the `STYL` payload. The comparison includes each section's
header and `LSTY`'s trailing zero padding. Equal length keeps `STYL`
and leaves bit 4 clear.

A palette of 65,535 colours or more is not encoded. `LSTY` is skipped
and the inventory uses `STYL`. Index `0xFFFF` means the colour is
absent, so a present colour never uses that index.

`open` accepts exactly one style section, and it must match the bit:

| Bit 4 | `LSTY` | `STYL` | Result |
|---|---|---|---|
| set | present | absent | decode `LSTY` |
| clear | absent | present | decode `STYL` |

Any other combination is E5009.

Decoded styles are the same pool either way: source styles first, then
styles introduced by `Restyle`, in first-use order. Projection rows
keep style ids. Lookup is unchanged.

## 2. `LSTY` layout

The payload is padded with zero bytes to a multiple of 16. Integers
are little-endian.

| Offset | Field |
|---|---|
| 0 | `u32` style count |
| 4 | `u16` colour count |
| 6 | 10 zero bytes |
| 16 | palette: `colour count` colours, 4 bytes each, strictly ascending and unique |
| 16 + 4·colours | one 12-byte record per style, in pool order |

**12-byte record.** Bytes 0–3 and 8–11 are the GVA style record's bytes
0–3 and 12–15: flags, fill rule, cap, join, and stroke width. Bytes
4–5 and 6–7 are `u16` palette indexes:

| Flag in byte 0 | Colour in the GVA record | Index |
|---|---|---|
| bit 0 set | fill, bytes 4–7 | palette index |
| bit 0 clear | no fill | `0xFFFF` |
| bit 1 set | stroke, bytes 8–11 | palette index |
| bit 1 clear | no stroke | `0xFFFF` |

The palette is that set of RGBA tuples, sorted in unsigned byte order
with red first, then green, blue and alpha. Duplicate colours appear
once.

## 3. Validation

These failures are E2006 (`LSTY: …`) unless noted:

- the payload is shorter than 16 bytes;
- the style count is above `GvaLimits.max_styles`, or any of the 10
  reserved bytes is non-zero;
- the palette and the records run past the payload, or the bytes after
  them are not the zero padding up to the next multiple of 16;
- the palette is not strictly ascending, including a repeated colour;
- a set flag names an index outside the palette;
- a clear flag carries an index other than `0xFFFF`;
- the 16-byte record rebuilt from the header, the indexed colours and
  the stroke width fails `decode_style_record` (E2005);
- re-encoding the decoded styles does not reproduce the payload
  (E2007).

The last check is the canonical form. Palette order, index width and
padding are fixed by the encoder above.
