//! Shared geometry types for the spatial transformation stage
//! (directive §9).
//!
//! Every stage below emission speaks one unit: CSS reference pixels
//! (1px = 1/96in, matching the CSS Absolute Lengths table). Emission
//! converts the resolved pixel boxes into the scene model's proportional
//! fractions; nothing downstream of [`dimensional`] invents its own units.

pub mod calc;
pub mod dimensional;
pub mod layout_systems;
pub mod proportional;

/// An absolute length in CSS reference pixels.
pub type Px = f64;

/// A CSS length: absolute pixels or a percentage of a context-dependent
/// reference (width, parent width, font size, ...).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f64),
    Percent(f64),
}

/// A resolved element box in CSS px. `x`/`y` are relative to the parent's
/// content-box origin; the document root is relative to the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: Px,
    pub y: Px,
    pub width: Px,
    pub height: Px,
}

/// A box split into its edges: content box plus optional padding and
/// border, so layout systems can compute content extents once.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub top: Px,
    pub right: Px,
    pub bottom: Px,
    pub left: Px,
}

impl Edges {
    /// Total horizontal space consumed by the edges.
    pub fn horizontal(&self) -> Px {
        self.left + self.right
    }

    /// Total vertical space consumed by the edges.
    pub fn vertical(&self) -> Px {
        self.top + self.bottom
    }
}

/// The per-element context length resolution needs: the element's own
/// font size, the root font size and the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Context {
    pub font_size: f64,
    pub root_font_size: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
}
