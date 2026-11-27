//! Fixed positioning (directive §9).
//!
//! A fixed-position box is positioned like an absolutely positioned one,
//! but its containing block is always the viewport, independent of any
//! positioned ancestor and of scrolling (a single static snapshot per
//! layout pass; no scroll offsets exist in this model). The driver
//! routes fixed elements through [`super::absolute`] with the viewport
//! as the containing block, so this module only derives that block.

use crate::geometry::{Context, Rect};

/// The viewport as a containing block: origin at the document root,
/// spanning the layout viewport.
pub(super) fn viewport_containing(context: Context) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width: context.viewport_width,
        height: context.viewport_height,
    }
}
