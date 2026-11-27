//! Proportional geometry conversion (directive §9.1).
//!
//! The native scene model positions children as fractions of the parent's
//! extent (the scene's `position_p`/`extent_p`), so a resolved pixel box
//! maps to proportional constraints by dividing against the parent box.
//! Conversion is pure arithmetic: identical inputs produce bit-identical
//! fractions on every host.

use super::Rect;

/// A box expressed as fractions of a reference extent. `x`/`y` are
/// fractions of the parent's width/height; `width`/`height` likewise.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ProportionalRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Convert a resolved pixel box (child coordinates relative to the
/// parent's origin) into proportional constraints. A zero parent extent
/// maps every fraction on that axis to zero rather than dividing by zero.
pub fn to_proportional(rect: &Rect, parent: &Rect) -> ProportionalRect {
    let width = if parent.width > 0.0 {
        rect.width / parent.width
    } else {
        0.0
    };
    let height = if parent.height > 0.0 {
        rect.height / parent.height
    } else {
        0.0
    };
    let x = if parent.width > 0.0 {
        rect.x / parent.width
    } else {
        0.0
    };
    let y = if parent.height > 0.0 {
        rect.y / parent.height
    } else {
        0.0
    };
    ProportionalRect {
        x,
        y,
        width,
        height,
    }
}

/// Inverse of [`to_proportional`]: scale fractions back to pixels for a
/// given parent box. Used to verify the round trip and to place elements
/// whose constraints were authored proportionally.
pub fn to_pixels(proportional: &ProportionalRect, parent: &Rect) -> Rect {
    Rect {
        x: proportional.x * parent.width,
        y: proportional.y * parent.height,
        width: proportional.width * parent.width,
        height: proportional.height * parent.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parent() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        }
    }

    #[test]
    fn conversion_divides_by_parent_extent() {
        let rect = Rect {
            x: 200.0,
            y: 150.0,
            width: 400.0,
            height: 300.0,
        };
        let proportional = to_proportional(&rect, &parent());
        assert_eq!(
            proportional,
            ProportionalRect {
                x: 0.25,
                y: 0.25,
                width: 0.5,
                height: 0.5
            }
        );
    }

    #[test]
    fn zero_parent_extent_does_not_divide() {
        let empty = Rect::default();
        let rect = Rect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        assert_eq!(to_proportional(&rect, &empty), ProportionalRect::default());
    }

    #[test]
    fn round_trip_recovers_the_box() {
        let rect = Rect {
            x: 33.0,
            y: 7.0,
            width: 128.0,
            height: 64.0,
        };
        let proportional = to_proportional(&rect, &parent());
        let recovered = to_pixels(&proportional, &parent());
        assert_eq!(recovered, rect);
    }

    #[test]
    fn nested_origin_offset_composes() {
        // A child of a child: coordinates are always parent-relative, so
        // converting against the immediate parent is enough at each level.
        let outer = Rect {
            x: 100.0,
            y: 50.0,
            width: 400.0,
            height: 200.0,
        };
        let inner = Rect {
            x: 50.0,
            y: 50.0,
            width: 200.0,
            height: 100.0,
        };
        let proportional = to_proportional(&inner, &outer);
        assert_eq!(proportional.x, 0.125);
        assert_eq!(proportional.y, 0.25);
        assert_eq!(proportional.width, 0.5);
        assert_eq!(proportional.height, 0.5);
    }
}
