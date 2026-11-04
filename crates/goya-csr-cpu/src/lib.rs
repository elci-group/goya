//! GY-CSR-CPU — multithreaded tile rasteriser for the Goya render list.
//!
//! Integrates the GY-CSR/1 scheduler (`goya-csr`): the dirty region is
//! partitioned into spatial territories per the selected policy, and scoped
//! workers rasterise their own territory's tiles in parallel. Work stealing
//! is simulated by round-robin draining of territories inside the scope so
//! an idle worker picks up the next unfinished territory.
//!
//! Correctness contract (directive acceptance #1): pixel output is identical
//! regardless of worker count. Every tile is drawn by iterating ALL
//! primitives in list order and clipping each to the tile, so overlapping
//! primitive order never depends on threads. Tiles are spatially disjoint,
//! so workers never write the same pixel.

use goya_change::PixelRect;
use goya_csr::{partition, select_policy, Policy};
use goya_runtime::Primitive;

/// Contract identifier implemented by this crate.
pub const CSR_CPU_CONTRACT: &str = "GY-CSR-CPU/1";

/// Default tile edge in device pixels.
pub const DEFAULT_TILE: u32 = 32;

/// Base fill colour for `Panel` primitives (opaque ARGB8888).
const PANEL_COLOR: u32 = 0xFF_33_66_99;
/// Base fill colour for `Button` primitives.
const BUTTON_COLOR: u32 = 0xFF_22_AA_66;
/// Base fill colour for `Text` extents.
const TEXT_COLOR: u32 = 0xFF_DD_DD_DD;
/// Stripe overlay applied to Button/Text extents so coverage is testable.
const STRIPE_COLOR: u32 = 0xFF_FF_E0_00;

/// Raw view of the pixel framebuffer shared across scoped workers.
///
/// Soundness: GY-CSR partitioning guarantees tile sets across territories
/// are disjoint, and each tile write is confined to the tile's pixel span,
/// so workers never write the same `u32`. Every pixel is written by at most
/// one worker with a single store (the XOR stripe folds into that write),
/// so there is no cross-thread read-modify-write race.
struct FrameBuf {
    ptr: *mut u32,
    width: i32,
    height: i32,
}

// SAFETY: see struct docs — writes are provably disjoint per pixel because
// territories hold disjoint tile sets and each tile draw clips to that tile.
unsafe impl Send for FrameBuf {}
unsafe impl Sync for FrameBuf {}

impl FrameBuf {
    fn new(pixels: &mut [u32], width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let len = width as usize * height as usize;
        if pixels.len() < len {
            return None;
        }
        Some(FrameBuf {
            ptr: pixels.as_mut_ptr(),
            width,
            height,
        })
    }

    /// Write one pixel if it lies inside the framebuffer. Clip-safe.
    fn blit_pixel(&self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }
        // SAFETY: bounds checked immediately above; index < width*height.
        unsafe { *self.ptr.add(y as usize * self.width as usize + x as usize) = color };
    }
}

/// Rasterise `prims` into `pixels` (ARGB8888, row-major, `width` x `height`)
/// restricted to `dirty`, using the GY-CSR/1 scheduler with up to `workers`
/// scoped threads. Output is identical to `raster_serial` for any
/// `workers >= 1`.
///
/// # Panics
/// Panics if `pixels` is smaller than `width * height` or the dimensions
/// are non-positive.
pub fn raster_tiles(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    prims: &[Primitive],
    dirty: PixelRect,
    tile: u32,
    workers: u32,
) {
    let workers = workers.max(1);
    let tile = tile.max(1);
    let buf = FrameBuf::new(pixels, width, height)
        .unwrap_or_else(|| panic!("raster_tiles: framebuffer smaller than {width}x{height}"));
    let dirty = clamp_rect(dirty, width, height);
    if dirty.width <= 0 || dirty.height <= 0 {
        return;
    }
    let policy = select_policy(dirty.width as u64 * dirty.height as u64, tile, workers);
    let territories = partition(dirty, tile, policy, workers);

    if territories.len() <= 1 || policy == Policy::Serial {
        for territory in &territories {
            for t in &territory.tiles {
                draw_tile(&buf, t, prims);
            }
        }
        return;
    }

    // Work stealing is simulated by draining territories round-robin: each
    // scoped worker takes the next unfinished territory via an atomic cursor,
    // so a heavy territory is never stranded on one thread.
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..territories.len().min(workers as usize) {
            scope.spawn(|| loop {
                let idx = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(territory) = territories.get(idx) else {
                    break;
                };
                for t in &territory.tiles {
                    draw_tile(&buf, t, prims);
                }
            });
        }
    });
}

/// Single-threaded reference rasteriser: byte-for-byte identical output to
/// `raster_tiles` at any worker count (used by the equivalence tests).
pub fn raster_serial(
    pixels: &mut [u32],
    width: i32,
    height: i32,
    prims: &[Primitive],
    dirty: PixelRect,
    tile: u32,
) {
    let tile = tile.max(1);
    let buf = FrameBuf::new(pixels, width, height)
        .unwrap_or_else(|| panic!("raster_serial: framebuffer smaller than {width}x{height}"));
    let dirty = clamp_rect(dirty, width, height);
    if dirty.width <= 0 || dirty.height <= 0 {
        return;
    }
    let policy = select_policy(dirty.width as u64 * dirty.height as u64, tile, 1);
    let territories = partition(dirty, tile, policy, 1);
    for territory in &territories {
        for t in &territory.tiles {
            draw_tile(&buf, t, prims);
        }
    }
}

/// Draw one tile: iterate primitives in list order, clip each to the tile
/// (and the framebuffer), fill or stripe the intersection. Order-independence
/// across worker counts follows from this fixed per-tile primitive order.
fn draw_tile(buf: &FrameBuf, tile: &goya_csr::Tile, prims: &[Primitive]) {
    let tile_rect = PixelRect {
        x: tile.x,
        y: tile.y,
        width: tile.size as i32,
        height: tile.size as i32,
    };
    for (idx, prim) in prims.iter().enumerate() {
        let (device, color, stripe) = prim_geometry(prim, idx);
        let Some(clip) = intersect(&device, &tile_rect) else {
            continue;
        };
        let Some(clip) = intersect(&clip, &buf.bounds()) else {
            continue;
        };
        if stripe {
            stripe_rect(buf, &clip, color);
        } else {
            fill_rect(buf, &clip, color);
        }
    }
}

/// Resolve a render primitive (millipoint layout rect) to a device-pixel
/// rect plus its draw colour and whether it gets the test stripe pattern.
/// The device rect is deterministic: millipoints floored to whole pixels.
fn prim_geometry(prim: &Primitive, idx: usize) -> (PixelRect, u32, bool) {
    let tint = (idx as u32)
        .wrapping_mul(0x1F_1F_1F)
        .wrapping_add(0x00_04_04_04);
    let (rect, base, stripe) = match prim {
        Primitive::Panel(r) => (r, PANEL_COLOR, false),
        Primitive::Button(r, _) => (r, BUTTON_COLOR, true),
        Primitive::Text(r, _) => (r, TEXT_COLOR, true),
    };
    let to_px = |v: i64| -> i32 { (v / 1000).clamp(0, i64::from(i32::MAX)) as i32 };
    let device = PixelRect {
        x: to_px(rect.x),
        y: to_px(rect.y),
        width: to_px(rect.width),
        height: to_px(rect.height),
    };
    (device, base ^ tint, stripe)
}

/// Fill `rect` with a solid colour, clipped by `blit_pixel`.
fn fill_rect(buf: &FrameBuf, rect: &PixelRect, color: u32) {
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            buf.blit_pixel(x, y, color);
        }
    }
}

/// Stripe `rect` with a coordinate-driven checkerboard of `color` so a test
/// can verify per-pixel coverage without glyph shaping. Purely a function of
/// device coordinates, hence worker-count independent.
fn stripe_rect(buf: &FrameBuf, rect: &PixelRect, color: u32) {
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let c = if (x ^ y) & 4 == 0 {
                color
            } else {
                STRIPE_COLOR
            };
            buf.blit_pixel(x, y, c);
        }
    }
}

impl FrameBuf {
    fn bounds(&self) -> PixelRect {
        PixelRect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        }
    }
}

/// Clamp `r` to a `width` x `height` framebuffer; empty result if fully outside.
fn clamp_rect(r: PixelRect, width: i32, height: i32) -> PixelRect {
    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = (r.x + r.width).min(width);
    let y1 = (r.y + r.height).min(height);
    PixelRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

/// Intersection of two pixel rects; `None` when disjoint or empty.
fn intersect(a: &PixelRect, b: &PixelRect) -> Option<PixelRect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    if x1 <= x0 || y1 <= y0 {
        None
    } else {
        Some(PixelRect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }
}

/// Deterministic 64-bit FNV-1a hash over a byte string (kept public for
/// downstream golden-image tests).
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// Hash of the whole framebuffer — the equivalence tests' bit-exact digest.
pub fn hash_pixels(pixels: &[u32]) -> u64 {
    let bytes: Vec<u8> = pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
    fnv1a(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_layout::Rect;

    const W: i32 = 256;
    const H: i32 = 256;

    fn rect(x: i64, y: i64, w: i64, h: i64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    /// Overlapping primitive set covering the whole region (millipoints).
    fn scene() -> Vec<Primitive> {
        vec![
            Primitive::Panel(rect(0, 0, 256_000, 256_000)),
            Primitive::Button(rect(32_000, 32_000, 128_000, 128_000), String::from("Ok")),
            Primitive::Text(rect(96_000, 96_000, 192_000, 192_000), String::from("hi")),
            Primitive::Panel(rect(160_000, 40_000, 64_000, 64_000)),
            Primitive::Button(rect(40_000, 160_000, 96_000, 96_000), String::from("Go")),
        ]
    }

    fn full_dirty() -> PixelRect {
        PixelRect {
            x: 0,
            y: 0,
            width: W,
            height: H,
        }
    }

    fn run_parallel(workers: u32) -> Vec<u32> {
        let mut px = vec![0xFF_00_00_00_u32; (W * H) as usize];
        raster_tiles(&mut px, W, H, &scene(), full_dirty(), 32, workers);
        px
    }

    #[test]
    fn one_and_four_workers_are_pixel_identical() {
        let a = run_parallel(1);
        let b = run_parallel(4);
        assert_eq!(a, b, "worker count changed pixel output");
        // And every intermediate budget too.
        for w in [2_u32, 3, 6, 16] {
            assert_eq!(run_parallel(w), a, "mismatch at {w} workers");
        }
    }

    #[test]
    fn output_is_bit_exact_across_runs() {
        let a = hash_pixels(&run_parallel(4));
        let b = hash_pixels(&run_parallel(4));
        let c = hash_pixels(&run_parallel(4));
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_ne!(a, 0, "sanity: scene must draw something");
    }

    #[test]
    fn serial_fallback_equals_parallel() {
        let mut ser = vec![0x11_22_33_44_u32; (W * H) as usize];
        raster_serial(&mut ser, W, H, &scene(), full_dirty(), 32);
        assert_eq!(ser, run_parallel(1));
        assert_eq!(ser, run_parallel(4));
        assert_eq!(hash_pixels(&ser), hash_pixels(&run_parallel(3)));
    }

    #[test]
    fn pixels_outside_dirty_rect_are_untouched() {
        let sentinel = 0xAB_CD_EF_12_u32;
        let mut px = vec![sentinel; (W * H) as usize];
        let dirty = PixelRect {
            x: 64,
            y: 64,
            width: 96,
            height: 96,
        };
        raster_tiles(&mut px, W, H, &scene(), dirty, 32, 4);
        for y in 0..H {
            for x in 0..W {
                let inside = x >= dirty.x
                    && x < dirty.x + dirty.width
                    && y >= dirty.y
                    && y < dirty.y + dirty.height;
                let v = px[(y as usize) * (W as usize) + (x as usize)];
                if !inside {
                    assert_eq!(
                        v, sentinel,
                        "pixel ({x},{y}) outside dirty rect was written"
                    );
                }
            }
        }
        // Something inside was actually drawn (scene covers the dirty rect).
        assert_ne!(
            px[(100 * W as usize) + 100],
            sentinel,
            "dirty rect interior should be redrawn"
        );
    }

    #[test]
    fn all_worker_budgets_agree_with_serial_on_full_region() {
        let mut base = vec![0_u32; (W * H) as usize];
        raster_serial(&mut base, W, H, &scene(), full_dirty(), 32);
        for workers in [1_u32, 2, 4] {
            let mut px = vec![0_u32; (W * H) as usize];
            raster_tiles(&mut px, W, H, &scene(), full_dirty(), 32, workers);
            assert_eq!(px, base, "divergence at {workers} workers");
        }
    }

    #[test]
    fn dirty_rect_offset_and_partial_tiles_agree_with_serial() {
        // Non tile-aligned dirty rect exercises clipping on every edge.
        let dirty = PixelRect {
            x: 37,
            y: 51,
            width: 133,
            height: 87,
        };
        let mut ser = vec![0_u32; (W * H) as usize];
        raster_serial(&mut ser, W, H, &scene(), dirty, 32);
        let mut par = vec![0_u32; (W * H) as usize];
        raster_tiles(&mut par, W, H, &scene(), dirty, 32, 4);
        assert_eq!(ser, par);
    }

    #[test]
    fn parallel_scales_or_at_least_completes_large_region() {
        // 1024x1024, 200 overlapping rects. Timing is environment-sensitive,
        // so the hard assertion is completion + correctness; the 2x slowdown
        // bound is checked only when both timings look trustworthy.
        let (w, h) = (1024_i32, 1024_i32);
        let prims: Vec<Primitive> = (0..200)
            .map(|i| {
                let x = i64::from(i % 20) * 50;
                let y = i64::from(i / 20) * 50;
                if i % 3 == 0 {
                    Primitive::Panel(rect(x * 1000, y * 1000, 60_000, 60_000))
                } else {
                    Primitive::Button(rect(x * 1000, y * 1000, 80_000, 80_000), String::from("b"))
                }
            })
            .collect();
        let dirty = PixelRect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        let mut ser = vec![0_u32; (w * h) as usize];
        raster_serial(&mut ser, w, h, &prims, dirty, 32);
        let mut par = vec![0_u32; (w * h) as usize];
        raster_tiles(&mut par, w, h, &prims, dirty, 32, 4);
        assert_eq!(ser, par, "large-scene parallel output diverged");

        let t0 = std::time::Instant::now();
        let mut par2 = vec![0_u32; (w * h) as usize];
        raster_tiles(&mut par2, w, h, &prims, dirty, 32, 4);
        let par_time = t0.elapsed();
        let t1 = std::time::Instant::now();
        let mut ser2 = vec![0_u32; (w * h) as usize];
        raster_serial(&mut ser2, w, h, &prims, dirty, 32);
        let ser_time = t1.elapsed();
        assert_eq!(par2, ser2);
        // Soft perf guard: only enforce when measurements look trustworthy.
        if ser_time.as_millis() > 5 && par_time.as_millis() > 0 {
            assert!(
                par_time.as_millis() < ser_time.as_millis() * 2,
                "parallel ({par_time:?}) more than 2x slower than serial ({ser_time:?})"
            );
        }
    }

    #[test]
    fn empty_and_degenerate_inputs_are_safe() {
        let mut px = vec![0_u32; (W * H) as usize];
        raster_tiles(&mut px, W, H, &[], full_dirty(), 32, 4);
        raster_tiles(
            &mut px,
            W,
            H,
            &scene(),
            PixelRect {
                x: 5,
                y: 5,
                width: 0,
                height: 0,
            },
            32,
            4,
        );
        raster_tiles(
            &mut px,
            W,
            H,
            &scene(),
            PixelRect {
                x: -9999,
                y: -9999,
                width: 10,
                height: 10,
            },
            32,
            4,
        );
        // Off-screen primitives clip without panicking.
        let off = vec![Primitive::Panel(rect(-50_000, -50_000, 20_000, 20_000))];
        raster_tiles(&mut px, W, H, &off, full_dirty(), 32, 4);
    }

    #[test]
    fn coverage_is_verifiable_via_stripe_pattern() {
        let mut px = vec![0_u32; (W * H) as usize];
        let prims = vec![Primitive::Button(
            rect(0, 0, 64_000, 64_000),
            String::from("X"),
        )];
        raster_tiles(&mut px, W, H, &prims, full_dirty(), 32, 4);
        // Inside the button: striped cells in either stripe colour.
        assert_ne!(px[0], 0);
        // (0,0) and (5,5) share checker parity ((x^y)&4 == 0 both ways).
        assert_eq!(px[0], px[(5 * W as usize) + 5], "checker parity must match");
        // (5,0) flips parity ((5^0)&4 == 4) so the colour must alternate.
        assert_ne!(px[0], px[5], "checker must alternate colour");
        // Just outside the 64x64 button: untouched.
        assert_eq!(px[(70 * W as usize) + 70], 0);
    }
}
