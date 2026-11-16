# GY-EMP-RENDER/1

Contract identifier: `GY-EMP-RENDER/1`. Programme GY-EMP-001, revision 1.0.0. Date 16 November 2026.

The renderer contract will eventually name a software backend and a Vulkan backend, and it will select between them from measured workload, not from a blanket preference for the GPU. That selection is not implemented.

Repository status: the software path below is specified for this cycle in `crates/emperor-runtime`, calling the existing `goya-wayland` host. Vulkan is Blocked. EMP-M1 is not certified.

## Software path

EMP-M1 SHALL draw only by calling `GoyaWindow::present_dirty`. It SHALL NOT call a Vulkan API. It SHALL NOT be the only future way to render, but it is the only way this cycle renders.

`present_dirty` arguments for the counter:

- `primitives`: the slice described below.
- `fills`: one entry per primitive. The focused control's primitives use `Some(0xFFD6E4F5)`. Every other entry is `None`.
- `fgs`: one `None` per primitive.
- `fg`: `[20, 20, 28]`.
- `bg`: `[235, 235, 240]`.
- `glyph_px`: `8`.
- `damage`: `None`, meaning a full frame when a frame is presented.

The window is created with `GoyaWindow::create`. The title is the client element's text, which is `Count` on the fixture. The requested size is the admitted DIP size, 320 by 160.

Primitives use `goya_runtime::Primitive` and `goya_layout::Rect`. Rectangles are millipoints. Multiply each DIP edge by 250. `sash::admit::MP_PER_DIP` and `goya_wayland::MP_PER_PX` are both 250. One integer DIP equals one pixel at that scale and at buffer scale 1. That equality is the current rule. It is not a fractional-scale rule.

Paint, in order:

1. Client: `Primitive::Panel` of the client rectangle, then `Primitive::Text` of the same rectangle and the authored client text.
2. Edit: `Primitive::Text` of the edit rectangle and the live edit string.
3. Button: `Primitive::Button` of the button rectangle and the authored button text.
4. Result: `Primitive::Text` of the result rectangle and the session result string.

Initial focus is the edit, from `KeyController::open`. The focus cycle is the edit and the button. The client and the result are not focused by Tab. Only the focused control takes the fill. On the first frame that is the edit `Text` primitive.

The host already composites shared-memory buffers, rejects a frame whose dimensions differ from the configured surface, and can shape text for other shells. EMP-M1 does not retarget those host features at a second scene.

## Frame scheduling

An inactive, unchanged window SHALL NOT continuously redraw.

The runtime starts dirty. It polls, handles events, and calls `present_dirty` only when dirty. After a successful present it clears dirty and increments the presented count. It then pumps one round. It does not present again until an event changes state or size.

The first frame may present, because dirty starts true. A smoke run presents at least one frame and returns with Wayland status `opened`. A live run, when not dirty, sleeps 16 milliseconds and does not present. Smoke and live are not qualified release modes until a real display records them.

Oracle mode does not present. `presented` is 0 and the Wayland status is `not-requested`.

## Resize

A `Resized` event marks the window dirty and does not move the admitted DIP hit rectangles. Sash clips controls to the client and does not reflow them. Emperor uses the same rectangles for hit testing after a resize. Layout is not solved again. Fractional scaling is not applied.

The host stores a positive configure size, drops shared-memory buffers, and rejects a present whose buffer size does not match the surface. The runtime must present a buffer that matches the configured surface. Hit testing still uses the authored DIP rectangles. Moving or scaling those rectangles to fill the new size would be a reflow, and this contract forbids it.

## What is blocked

Vulkan is Blocked. No retained GPU scene, no device-loss path, and no backend negotiation exist in this cycle. Headless software drawing inside the oracle is not a second renderer. It is the absence of a present call.

Fractional scaling is Blocked. HDR is Blocked. Wide gamut is Blocked. SDR colours in this cycle are the fixed foreground, background, and focus fill above. `goya-chroma` remains the theme authority for the programme, and this cycle does not read it. Theme switching is unmet.

Animation, VSync feedback, reduced-motion preferences, background throttling, and energy budgets are unmet. The 16 millisecond sleep is an idle wait in the live loop, not a measured frame-time result. The provisional 60 Hz targets in GY-EMP-001 are not measurements.

Glyph caching, incremental invalidation beyond the dirty flag, and damage rectangles other than `None` are unmet. Text measurement, caret geometry, and hit testing of glyphs are not part of EMP-M1. The edit is a whole string, not a shaped caret.
