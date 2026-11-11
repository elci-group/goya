# ZCode monitor GUI parity increment

Status: Candidate. This specification covers resize-aware native layout,
transcript viewport sizing, and keyboard selection in the Goya ZCode monitor.

### Renderer coordinate contract

Scene layout coordinates are points converted to millipoints by `goya-lang`
(1000 mp per scene point). The Wayland raster maps 250 mp to one surface
pixel. Therefore this host adapter expresses each intended UI pixel as 0.25
scene points, and every published frame rect must satisfy `rect / 250 ==`
the corresponding pixel geometry. Viewport calculations and transcript row
counts remain in pixels. Pointer input, accessibility hit regions, and the
IME candidate rect use the resulting frame in surface-local pixel geometry.
Per-node scene text sizes are passed to the native shaper while preserving
existing foreground overrides.

## Product behavior

- The native window begins at 400×320 pixels and can be resized. Each
  configured size is the source of truth for the content layout.
- The header stays at the top, the composer and quit control stay in the
  footer, and transcript rows fill the space between them. The composer and
  quit control retain fixed-height geometry and margins. Very small windows
  clamp the transcript area to zero rows without negative or overlapping
  rectangles.
- The number of visible transcript rows is derived from the laid-out
  transcript viewport. `SessionView` owns transcript history and scroll
  position; it supplies exactly that many rows, leading-padded when there is
  less content and clamping scrollback when the viewport changes.
- One current, renderer-scaled `Frame` is shared by pointer hit testing, rendering, accessible
  hit regions, and the IME candidate rectangle. A resize rebuilds that frame
  and republishes accessibility geometry even when transcript content is not
  dirty.
- Shift+Left/Right extends the grapheme-safe composer selection. Home/End
  preserve their existing navigation behavior; selection state remains in the
  shared `TextBuffer` and is sent as byte offsets to text-input-v3.
- Text remains clipped by the composer node. The Wayland renderer provides
  shaped text measurement and caret APIs, but this monitor does not yet use
  them for mouse-to-caret mapping or selection painting. Candidate placement
  remains a clamped six-pixel-per-grapheme estimate.
- Native text-input-v3 batches preserve the preedit byte cursor/selection
  endpoints and delete-surrounding byte counts until `done`. The composer
  applies each completed batch in the protocol's required order and routes
  deletes through grapheme-safe `TextBuffer` edits.

## State and event ownership

`GoyaWindow` owns the configured pixel size and supplies `Resized(width,
height)`. The host loop owns the current viewport dimensions and derives one
layout frame from them. The frame is transient derived geometry, not a second
layout state store.

```text
Wayland configure ── Resized(w,h) ──> zcode-native viewport
                                         │
                                         ├─ layout_monitor(scene, viewport) ──> Frame
                                         │                                    ├─ hit test
                                         │                                    ├─ render
                                         │                                    ├─ a11y rectangles
                                         │                                    └─ IME candidate rectangle
                                         └─ visible row count ──> SessionView.rows_for_window(n)
```

`SessionView` remains the only owner of transcript data and scroll position.
The monitor host owns composer focus/input routing. `TextBuffer` remains the
single owner of the composer cursor and selection.

Text-input-v3 updates have one protocol batch owner in `TextInputState`. Each
protocol callback stages fields; `done` emits one ordered event batch. The
composer applies that batch synchronously before publishing its next
surrounding-text state:

```text
text_input_v3 callbacks ──> TextInputState pending batch
                                  │ done
                                  ▼
                         clear old preedit
                                  ↓
                         delete surrounding bytes
                                  ↓
                         apply commit strings
                                  ↓
                         install new preedit + byte range
```

## Native pixel surface

`GoyaWindow::present_rgba(rgba, width, height)` accepts tightly packed,
top-down, straight-alpha RGBA8 bytes. Width and height must match the current
configured surface exactly; the API does not scale or letterbox. Validation
checks exact byte length, checked size arithmetic, and a 16 megapixel cap
before any SHM write. The renderer alpha-composites against the
last primitive-frame background (black before the first primitive frame),
then copies opaque native-format words into a released shared-memory slot and
uses the existing attach/commit path. A frame for a stale resize is rejected
with a size mismatch error and the producer can retry after receiving the new
configured dimensions.

## Acceptance scenarios

- Headless resize acceptance lays out at the initial size, a wider/taller
  size, and minimum dimensions. It checks footer anchoring, bounded row count,
  nonnegative/clipped geometry, and verifies root width/height in actual
  surface pixels, footer and transcript bounds, pointer hits at surface pixel
  coordinates, a11y rectangles in the same pixel units, and IME candidate
  geometry inside the viewport.
- A resize-only event changes geometry and a11y publication count without
  requiring a protocol or composer update. Dirty redraw and scene watch
  behavior remain event-driven.
- A synthetic transcript viewed with different row counts returns the newest
  rows at the bottom, pads short history at the top, and preserves/clamps the
  scroll-from-bottom offset across viewport changes.
- Headless input acceptance selects a grapheme range with modified arrows,
  inserts text through the existing buffer path, and confirms text-input
  surrounding state reports cursor and anchor separately.
- Pixel-surface tests cover alpha blending, exact length, dimension mismatch,
  overflow, the maximum pixel count, and rejection before SHM writes. A live
  compositor/browser presentation check remains an operator acceptance item.
- Enter clears the composer only after synchronous admission into the existing
  agent command channel. Disabled sends, missing sessions, and busy/full
  admission restore the same text, cursor/selection, and history draft; a
  later CLI acknowledgement remains owned by the agent lifecycle.

## Known platform limits

The adapter preserves preedit cursor endpoints and delivers
delete-surrounding requests, but tests remain compositor-free. Whether a
specific desktop/input method sends useful CJK composition traffic remains
operator acceptance. The monitor also does not perform font-shaped
mouse-to-caret mapping or paint selection ranges; the six-pixel candidate
position remains a documented estimate.

Extended IME cursor/deletion events are opt-in per window. Existing clients retain legacy preedit events; the native successor opts into the richer contract.
