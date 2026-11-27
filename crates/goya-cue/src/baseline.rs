//! Baseline present-path pricing.
//!
//! Full, damage-copy, and damage-in-place are the three ways a frame is
//! published. [`account`] prices the bytes. [`time_paths`] performs those
//! memory operations and reports the median and p99.

use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentPath {
    Full,
    DamageCopy,
    DamageInPlace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathAccount {
    pub bytes_copied: u64,
    pub bytes_cleared: u64,
    pub pixels_raster_bound: u64,
}

/// Damage is `(x, y, width, height)` in pixels. `None` means the whole frame.
///
/// Clip damage to the frame. A non-positive frame yields a zero account.
/// [`PresentPath::Full`] ignores the damage rectangle and charges a clear plus
/// a raster of every pixel, and copies nothing. [`PresentPath::DamageCopy`]
/// charges one full-frame copy (`width * height * 4`), then a clear and a
/// raster of the clipped damage. [`PresentPath::DamageInPlace`] copies nothing
/// and clears plus rasters only the clipped damage.
pub fn account(
    path: PresentPath,
    width: u32,
    height: u32,
    damage: Option<(i32, i32, i32, i32)>,
) -> PathAccount {
    if width == 0 || height == 0 {
        return PathAccount::ZERO;
    }
    let frame = pixel_count(width, height);
    let painted = match path {
        PresentPath::Full => frame,
        PresentPath::DamageCopy | PresentPath::DamageInPlace => {
            let clip = clip_damage(width, height, damage);
            pixel_count(clip.w, clip.h)
        }
    };
    let bytes_copied = match path {
        PresentPath::DamageCopy => rgba_bytes(frame),
        PresentPath::Full | PresentPath::DamageInPlace => 0,
    };
    PathAccount {
        bytes_copied,
        bytes_cleared: rgba_bytes(painted),
        pixels_raster_bound: painted,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimedSample {
    pub median_ns: u128,
    pub p99_ns: u128,
    pub account: PathAccount,
}

/// Actually perform the memory operations `iterations` times (minimum 1).
///
/// Returns `[Full, DamageCopy, DamageInPlace]`.
/// Median is the middle of a sorted sample (odd count) or the upper middle (even).
/// p99 is the last sample after sorting when iterations < 100, else the
/// `ceil(0.99 * (n - 1))` index.
pub fn time_paths(
    width: u32,
    height: u32,
    damage: Option<(i32, i32, i32, i32)>,
    iterations: usize,
) -> [TimedSample; 3] {
    let iterations = iterations.max(1);
    let clip = clip_damage(width, height, damage);
    // Both buffers exist for the whole measurement. The timed loops only fill
    // and copy; they do not allocate.
    let mut front = alloc_frame(width, height);
    let mut back = vec![0u32; front.len()];
    for (i, px) in front.iter_mut().enumerate() {
        *px = (i as u32).wrapping_mul(0x45D9_F3B) ^ 0xA5A5_A5A5;
    }

    let full = sample(
        iterations,
        account(PresentPath::Full, width, height, damage),
        |stamp| {
            fill_all(&mut back, stamp);
        },
    );
    let copy = sample(
        iterations,
        account(PresentPath::DamageCopy, width, height, damage),
        |stamp| copy_then_fill(&front, &mut back, width, clip, stamp),
    );
    let inplace = sample(
        iterations,
        account(PresentPath::DamageInPlace, width, height, damage),
        |stamp| fill_clipped(&mut back, width, clip, stamp),
    );
    [full, copy, inplace]
}

impl PathAccount {
    const ZERO: Self = Self {
        bytes_copied: 0,
        bytes_cleared: 0,
        pixels_raster_bound: 0,
    };
}

#[derive(Clone, Copy, Debug)]
struct Clip {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Clip {
    const EMPTY: Self = Self {
        x: 0,
        y: 0,
        w: 0,
        h: 0,
    };
}

/// Intersection of the damage rectangle with `[0, width) × [0, height)`.
/// Extents are widened to `i64` so a negative origin cannot overflow `i32`.
fn clip_damage(width: u32, height: u32, damage: Option<(i32, i32, i32, i32)>) -> Clip {
    if width == 0 || height == 0 {
        return Clip::EMPTY;
    }
    let Some((x, y, dw, dh)) = damage else {
        return Clip {
            x: 0,
            y: 0,
            w: width,
            h: height,
        };
    };
    if dw <= 0 || dh <= 0 {
        return Clip::EMPTY;
    }
    let x0 = i64::from(x);
    let y0 = i64::from(y);
    let x1 = x0 + i64::from(dw);
    let y1 = y0 + i64::from(dh);
    let right_bound = i64::from(width);
    let bottom_bound = i64::from(height);
    let left = x0.clamp(0, right_bound);
    let right = x1.clamp(0, right_bound);
    let top = y0.clamp(0, bottom_bound);
    let bottom = y1.clamp(0, bottom_bound);
    if right <= left || bottom <= top {
        return Clip::EMPTY;
    }
    Clip {
        x: left as u32,
        y: top as u32,
        w: (right - left) as u32,
        h: (bottom - top) as u32,
    }
}

fn pixel_count(width: u32, height: u32) -> u64 {
    u64::from(width).saturating_mul(u64::from(height))
}

fn rgba_bytes(pixels: u64) -> u64 {
    pixels.saturating_mul(4)
}

fn alloc_frame(width: u32, height: u32) -> Vec<u32> {
    match (width as usize).checked_mul(height as usize) {
        Some(n) => vec![0u32; n],
        None => Vec::new(),
    }
}

fn fill_all(buf: &mut [u32], stamp: u32) {
    buf.fill(black_box(stamp ^ 0xFF11_2233));
    black_box(buf);
}

fn fill_clipped(buf: &mut [u32], stride: u32, clip: Clip, stamp: u32) {
    let color = black_box(stamp ^ 0xFF20_3040);
    if clip.w == 0 || clip.h == 0 || stride == 0 || !clip_in_buf(buf, stride, clip) {
        black_box(buf);
        return;
    }
    let stride = stride as usize;
    let x = clip.x as usize;
    let w = clip.w as usize;
    let y0 = clip.y as usize;
    let y1 = y0 + clip.h as usize;
    for row in y0..y1 {
        let start = row * stride + x;
        buf[start..start + w].fill(color);
    }
    black_box(buf);
}

fn clip_in_buf(buf: &[u32], stride: u32, clip: Clip) -> bool {
    let stride = stride as usize;
    let last_row = (clip.y as usize)
        .saturating_add(clip.h as usize)
        .saturating_sub(1);
    match last_row
        .checked_mul(stride)
        .and_then(|off| off.checked_add(clip.x as usize))
        .and_then(|off| off.checked_add(clip.w as usize))
    {
        Some(end) => end <= buf.len(),
        None => false,
    }
}

fn copy_then_fill(front: &[u32], back: &mut [u32], stride: u32, clip: Clip, stamp: u32) {
    let front = black_box(front);
    if back.len() == front.len() {
        back.copy_from_slice(front);
    }
    fill_clipped(back, stride, clip, stamp);
}

fn sample(iterations: usize, priced: PathAccount, mut body: impl FnMut(u32)) -> TimedSample {
    let mut elapsed = Vec::with_capacity(iterations);
    for i in 0..iterations {
        let started = Instant::now();
        body(i as u32);
        elapsed.push(started.elapsed().as_nanos());
    }
    elapsed.sort_unstable();
    TimedSample {
        median_ns: elapsed[iterations / 2],
        p99_ns: elapsed[p99_index(iterations)],
        account: priced,
    }
}

/// Last rank below 100 samples; otherwise `ceil(0.99 * (n - 1))`.
fn p99_index(n: usize) -> usize {
    if n < 100 {
        n - 1
    } else {
        let span = (n - 1) as u128;
        ((span * 99 + 99) / 100) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_hd_band_accounts() {
        let damage = Some((0, 0, 1920, 313));
        let full = account(PresentPath::Full, 1920, 1080, damage);
        assert_eq!(full.bytes_copied, 0);
        assert_eq!(full.bytes_cleared, 2_073_600 * 4);
        assert_eq!(full.pixels_raster_bound, 2_073_600);

        let copy = account(PresentPath::DamageCopy, 1920, 1080, damage);
        assert_eq!(copy.bytes_copied, 2_073_600 * 4);
        assert_eq!(copy.bytes_cleared, 1920 * 313 * 4);
        assert_eq!(copy.pixels_raster_bound, 1920 * 313);
        assert!(copy.bytes_copied + copy.bytes_cleared > full.bytes_cleared);

        let inplace = account(PresentPath::DamageInPlace, 1920, 1080, damage);
        assert_eq!(inplace.bytes_copied, 0);
        assert_eq!(inplace.bytes_cleared, 1920 * 313 * 4);
        assert!(inplace.bytes_cleared < full.bytes_cleared);
        assert_eq!(inplace.pixels_raster_bound, 1920 * 313);
    }

    #[test]
    fn negative_damage_is_clipped() {
        // Origin is outside the frame; only the overlapping 6×4 pixels count.
        let damage = Some((-4, -8, 10, 12));
        let inplace = account(PresentPath::DamageInPlace, 32, 32, damage);
        assert_eq!(inplace.bytes_copied, 0);
        assert_eq!(inplace.pixels_raster_bound, 24);
        assert_eq!(inplace.bytes_cleared, 96);

        let copy = account(PresentPath::DamageCopy, 32, 32, damage);
        assert_eq!(copy.bytes_copied, 32 * 32 * 4);
        assert_eq!(copy.bytes_cleared, 96);
        assert_eq!(copy.pixels_raster_bound, 24);

        let full = account(PresentPath::Full, 32, 32, damage);
        assert_eq!(full.pixels_raster_bound, 32 * 32);
        assert_eq!(full.bytes_copied, 0);

        let outside = account(PresentPath::DamageInPlace, 64, 64, Some((-10, -10, 5, 5)));
        assert_eq!(outside, PathAccount::ZERO);

        // i32 addition would overflow; clipping must not panic.
        let extreme = account(
            PresentPath::DamageCopy,
            32,
            32,
            Some((i32::MIN, i32::MIN, i32::MAX, i32::MAX)),
        );
        assert_eq!(extreme.bytes_copied, 32 * 32 * 4);
        assert_eq!(extreme.bytes_cleared, 0);
        assert_eq!(extreme.pixels_raster_bound, 0);

        let timed = time_paths(32, 32, damage, 1);
        assert_eq!(timed[0].account, full);
        assert_eq!(timed[1].account, copy);
        assert_eq!(timed[2].account, inplace);

        assert_eq!(
            account(PresentPath::Full, 0, 1080, damage),
            PathAccount::ZERO
        );
        assert_eq!(
            account(PresentPath::DamageCopy, 10, 0, None),
            PathAccount::ZERO
        );
        assert_eq!(
            account(PresentPath::DamageInPlace, 8, 8, None).pixels_raster_bound,
            64
        );
    }

    #[test]
    fn time_paths_matches_account() {
        let damage = Some((0, 0, 16, 16));
        let samples = time_paths(64, 64, damage, 11);
        let paths = [
            PresentPath::Full,
            PresentPath::DamageCopy,
            PresentPath::DamageInPlace,
        ];
        for (sample, path) in samples.into_iter().zip(paths) {
            assert!(sample.median_ns > 0, "{path:?} median {}", sample.median_ns);
            assert!(
                sample.p99_ns >= sample.median_ns,
                "{path:?} p99 {} median {}",
                sample.p99_ns,
                sample.median_ns
            );
            assert_eq!(sample.account, account(path, 64, 64, damage));
        }
    }

    #[test]
    fn rank_indexes_follow_the_sample_rules() {
        assert_eq!(p99_index(1), 0);
        assert_eq!(p99_index(11), 10);
        assert_eq!(p99_index(99), 98);
        assert_eq!(p99_index(100), 99);
        assert_eq!(p99_index(101), 99);
        assert_eq!(p99_index(200), 198);
        // Odd: middle. Even: upper middle. Both are `n / 2` after an ascending sort.
        assert_eq!(11 / 2, 5);
        assert_eq!(4 / 2, 2);
        assert_eq!(2 / 2, 1);
    }
}
