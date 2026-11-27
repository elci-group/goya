//! CPU retained framebuffer.
//!
//! A GPU retained scene is not implemented. Device-local residency is
//! [`crate::residency::PlaceError::VulkanBlocked`].

/// Packed `u32` pixels, row-major, `width` stride.
///
/// The buffer is process memory. Nothing here is a device allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedFrame {
    width: i32,
    height: i32,
    pixels: Vec<u32>,
}

impl RetainedFrame {
    /// Each axis is clamped at 0. A zero or negative axis yields an empty pixel buffer.
    pub fn new(width: i32, height: i32, clear: u32) -> Self {
        let width = width.max(0);
        let height = height.max(0);
        let len = (width as usize).saturating_mul(height as usize);
        Self {
            width,
            height,
            pixels: vec![clear; len],
        }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Clear the whole frame, then paint fills in order. Each fill is (x, y, w, h, colour). Clip to the frame. Non-positive w/h paints nothing.
    pub fn redraw(&mut self, clear: u32, fills: &[(i32, i32, i32, i32, u32)]) {
        self.pixels.fill(clear);
        for &(x, y, w, h, colour) in fills {
            paint(
                &mut self.pixels,
                self.width,
                self.height,
                (x, y, w, h),
                colour,
            );
        }
    }

    /// Clear and paint only inside damage (x, y, w, h). Pixels outside damage are unchanged. Fills are clipped to the intersection of their rect and the damage rect and the frame.
    pub fn update(
        &mut self,
        damage: (i32, i32, i32, i32),
        clear: u32,
        fills: &[(i32, i32, i32, i32, u32)],
    ) {
        paint(&mut self.pixels, self.width, self.height, damage, clear);
        for &(x, y, w, h, colour) in fills {
            let Some(region) = intersect((x, y, w, h), damage) else {
                continue;
            };
            paint(&mut self.pixels, self.width, self.height, region, colour);
        }
    }

    /// Clone self (the front-buffer copy) then update the clone. Returns the new slot.
    pub fn copy_forward(
        &self,
        damage: (i32, i32, i32, i32),
        clear: u32,
        fills: &[(i32, i32, i32, i32, u32)],
    ) -> Self {
        let mut next = self.clone();
        next.update(damage, clear, fills);
        next
    }
}

/// Inclusive origin, exclusive end. Non-positive spans cover nothing.
fn intersect(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    if aw <= 0 || ah <= 0 || bw <= 0 || bh <= 0 {
        return None;
    }
    let ax0 = ax as i64;
    let ay0 = ay as i64;
    let ax1 = ax0 + aw as i64;
    let ay1 = ay0 + ah as i64;
    let bx0 = bx as i64;
    let by0 = by as i64;
    let bx1 = bx0 + bw as i64;
    let by1 = by0 + bh as i64;
    let x0 = ax0.max(bx0);
    let y0 = ay0.max(by0);
    let x1 = ax1.min(bx1);
    let y1 = ay1.min(by1);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32))
}

fn paint(pixels: &mut [u32], width: i32, height: i32, rect: (i32, i32, i32, i32), colour: u32) {
    let (x, y, w, h) = rect;
    if w <= 0 || h <= 0 || width <= 0 || height <= 0 {
        return;
    }
    let x0 = (x as i64).max(0);
    let y0 = (y as i64).max(0);
    let x1 = (x as i64 + w as i64).min(width as i64);
    let y1 = (y as i64 + h as i64).min(height as i64);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let stride = width as usize;
    for row in y0..y1 {
        let start = row as usize * stride + x0 as usize;
        let end = row as usize * stride + x1 as usize;
        pixels[start..end].fill(colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(frame: &RetainedFrame, x: i32, y: i32) -> u32 {
        frame.pixels()[(y * frame.width() + x) as usize]
    }

    #[test]
    fn damage_keeps_outside_pixels_and_matches_full_redraw_inside() {
        let clear = 0x1111_1111;
        let old = (0, 0, 3, 2, 0xAA11_2233);
        let new_colour = (0, 0, 3, 2, 0xBB44_5566);
        let damage = (2, 1, 2, 2);

        let mut frame = RetainedFrame::new(8, 4, 0);
        frame.redraw(clear, &[old]);
        let redrawn = frame.clone();

        frame.update(damage, clear, &[new_colour]);

        let mut full = RetainedFrame::new(8, 4, 0);
        full.redraw(clear, &[new_colour]);

        for y in 0..4 {
            for x in 0..8 {
                let inside = (2..4).contains(&x) && (1..3).contains(&y);
                if inside {
                    assert_eq!(at(&frame, x, y), at(&full, x, y), "inside {x},{y}");
                } else {
                    assert_eq!(at(&frame, x, y), at(&redrawn, x, y), "outside {x},{y}");
                }
            }
        }

        let forwarded = redrawn.copy_forward(damage, clear, &[new_colour]);
        let mut cloned = redrawn.clone();
        cloned.update(damage, clear, &[new_colour]);
        assert_eq!(forwarded, cloned);
        assert_eq!(forwarded, frame);
    }

    #[test]
    fn damage_that_misses_the_frame_changes_nothing() {
        let mut frame = RetainedFrame::new(8, 4, 0x1111_1111);
        frame.redraw(0x1111_1111, &[(1, 1, 2, 2, 0xABCD_EF01)]);
        let before = frame.clone();
        frame.update((8, 0, 3, 3), 0x2222_2222, &[(0, 0, 8, 4, 0x3333_3333)]);
        assert_eq!(frame, before);
        frame.update((-4, -4, 2, 2), 0x2222_2222, &[(0, 0, 8, 4, 0x3333_3333)]);
        assert_eq!(frame, before);
    }

    #[test]
    fn non_positive_size_is_empty_and_clamped() {
        let frame = RetainedFrame::new(-3, 5, 0x1);
        assert_eq!(frame.width(), 0);
        assert_eq!(frame.height(), 5);
        assert!(frame.pixels().is_empty());
        let flat = RetainedFrame::new(4, -1, 0x1);
        assert_eq!(flat.width(), 4);
        assert_eq!(flat.height(), 0);
        assert!(flat.pixels().is_empty());
    }
}
