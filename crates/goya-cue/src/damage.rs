//! Conservative device-pixel covers of millipoint dirty spans.

use goya_change::PixelRect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MillipointRect {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

/// Conservative device-pixel cover of the inclusive millipoint span.
/// mp_per_px <= 0, or max < min on either axis, returns a zero rect at 0,0.
/// x0 = floor(min / mp), x1 = ceil(max / mp), width = x1 - x0, using i64 then saturate to i32.
pub fn millipoints_to_pixels(rect: MillipointRect, mp_per_px: i64) -> goya_change::PixelRect {
    if mp_per_px <= 0 || rect.max_x < rect.min_x || rect.max_y < rect.min_y {
        return PixelRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };
    }
    let (x, width) = axis_cover(rect.min_x, rect.max_x, mp_per_px);
    let (y, height) = axis_cover(rect.min_y, rect.max_y, mp_per_px);
    PixelRect {
        x,
        y,
        width,
        height,
    }
}

fn axis_cover(min: i32, max: i32, mp_per_px: i64) -> (i32, i32) {
    let min = i64::from(min);
    let max = i64::from(max);
    // Signed div_floor/div_ceil are still unstable. Divisor is positive here.
    let x0 = div_floor_pos(min, mp_per_px);
    let x1 = div_ceil_pos(max, mp_per_px);
    (sat_i32(x0), sat_i32(x1 - x0))
}

fn div_floor_pos(n: i64, d: i64) -> i64 {
    let q = n / d;
    if n % d != 0 && n < 0 {
        q - 1
    } else {
        q
    }
}

fn div_ceil_pos(n: i64, d: i64) -> i64 {
    let q = n / d;
    if n % d != 0 && n > 0 {
        q + 1
    } else {
        q
    }
}

fn sat_i32(v: i64) -> i32 {
    v.clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millipoints_span_covers_pixels() {
        let rect = MillipointRect {
            min_x: 52500,
            min_y: 0,
            max_x: 57500,
            max_y: 250,
        };
        assert_eq!(
            millipoints_to_pixels(rect, 250),
            PixelRect {
                x: 210,
                y: 0,
                width: 20,
                height: 1,
            }
        );
    }

    #[test]
    fn millipoints_zero_scale_is_zero_rect() {
        let rect = MillipointRect {
            min_x: 52500,
            min_y: 0,
            max_x: 57500,
            max_y: 250,
        };
        assert_eq!(
            millipoints_to_pixels(rect, 0),
            PixelRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            }
        );
    }

    #[test]
    fn converted_rect_uses_decide_retained() {
        let px = millipoints_to_pixels(
            MillipointRect {
                min_x: 52500,
                min_y: 0,
                max_x: 57500,
                max_y: 250,
            },
            250,
        );
        let decision = goya_change::decide_retained(
            &[px],
            &goya_change::CostModel::default(),
            goya_change::PresentCopy {
                frame_bytes: 0,
                copies_front: false,
            },
        );
        assert_eq!(
            decision,
            goya_change::decide(&[px], &goya_change::CostModel::default())
        );
    }
}
