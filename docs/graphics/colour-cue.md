# Colour Cueing

Status: **execution policy**, not a new `.gvi` section. GVI-L remains the
on-disk palette (`LSTY`). `open` still expands that palette into the style
pool. Colour Cueing decides what paint does with a colour after that.

Implementation: `crates/goya-cue`. Hardware admission:
`goya.graphics.colour_cue.placement.v1` in `goya-caprichios`.

## What is priced

`present_internal` already has three paths. `account` names them:

| Path | When | Bytes |
|---|---|---|
| Full | `damage` is `None` | Clear and raster every pixel. Copy nothing. |
| DamageCopy | Damage is set and the new slot is not the front buffer | Copy the whole front slot, then clear and raster the clipped damage. |
| DamageInPlace | Damage is set and the presenter paints the front slot | Clear and raster only the clipped damage. |

The preferred Wayland slot is a released buffer other than the front, so a
damaged frame takes DamageCopy whenever another slot is free. A 1920×1080
copy moves 2,073,600 × 4 bytes before the damage rectangle is painted.
`decide_retained` in `goya-change` charges that copy on the local side and
charges a full-frame write on the full side. Local wins only when its cost
is strictly lower. `decide` is unchanged.

The calculator live loop asks `present_damage` with the window's current
RGBA8 size. `copies_front` is true when the next present will memcpy a
slot other than the front. An in-place paint of the released front slot
sets it false, and can keep a rectangle that the copy would reject. A key that maps to a button is priced as
that button's press, together with the display. Envelope bounds are the
conservative pixel cover of each half-open millipoint rect. Presses in one
turn share one union. A rectangle with no positive area is a full frame,
because a zero clip would copy the front buffer and then paint nothing.
A resize or a hot-swap passes no damage, and a press in that same turn
does not shrink the full frame. The swap then recompiles the envelopes
from the new layout, so the next press prices that geometry. When every
Wayland slot is still owned, `present_dirty` returns success without
committing. The calculator keeps that damage and tries again after
`pump`. A present error drops the frame. Emperor still passes
`damage: None`, as GY-EMP-RENDER/1 requires for the counter.

## Index until the fill

`ChromaticRef::Index` names a palette entry. `resolve_at_fill` is the
expansion to RGBA8. `0xFFFF` and an out-of-range index resolve to nothing.
`select_paint` keeps a direct colour unless the indexed cost is strictly
smaller. A tie stays direct, matching GVI-L.

`fills_at_paint` is the raster boundary. It resolves each reference once
into the `0x00RRGGBB` word `present_dirty` already takes. `cue_fills` on
the Wayland host is that call. An indexed fill and the same direct word
rasterise to the same pixels. `present_damage` turns `decide_retained`
into the `Option` damage argument: `None` is a full frame. The calculator
passes that value through.

## Placement

`place(DeviceLocal)` returns `VulkanBlocked`. No device buffer is allocated.
`admit` refuses a discrete-GPU palette for the same reason, whether or not
an index looked cheaper. Tile damage and platform theme do not depend on
that comparison. Apple Silicon admission is process-visible shared memory.
It does not call Metal.

`RetainedFrame` is a CPU framebuffer. `update` repaints a damage rectangle.
`copy_forward` clones the front slot, then updates. A retained GPU scene is
the same `VulkanBlocked` refusal.

## Measured on this machine

`account` for 1920×1080 with damage `(0, 0, 1920, 313)`:

| Path | Copied | Cleared | Pixels in the raster bound |
|---|---:|---:|---:|
| Full | 0 | 8,294,400 | 2,073,600 |
| DamageCopy | 8,294,400 | 2,403,840 | 600,960 |
| DamageInPlace | 0 | 2,403,840 | 600,960 |

DamageCopy writes 10,698,240 bytes, which is more than the full clear.
`decide_retained` with `CostModel::default()` therefore chooses a full
frame for that band when `copies_front` is set. A 200×60 rectangle on a
400×680 window still chooses local with the copy. The same rectangle on
1920×1080 chooses local when the presenter paints in place.

`time_paths` on a 64×64 frame, 11 iterations, damage 16×16, one debug run:
median 11,262 ns full, 1,647 ns copy, 1,163 ns in place. p99 was 12,257 ns,
2,539 ns, and 1,191 ns. That buffer is 16 KB. It checks the harness. It is
not a frame-time result.

## Phases

| Phase | Where it lives |
|---|---|
| CC-0 | `baseline::account`, `baseline::time_paths` |
| CC-1 | `index::Palette`, `resolve_at_fill` |
| CC-2 | `goya_change::decide_retained`, `millipoints_to_pixels`, calculator |
| CC-3 | `select_paint` |
| CC-4 | `place` |
| CC-5 | Caprichios contract `goya.graphics.colour_cue.placement.v1` |
| CC-6 | `RetainedFrame` |
