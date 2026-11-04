//! Theme declarations: anchors, per-role relations, harmony and constraints.

use crate::color::Oklch;
use crate::relation::PropertyRelation;

/// Named colour-theory harmony policies (GY-CHR-001 §3). The default is
/// NeutralDominant: near-neutral surfaces with one coherent chromatic accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Harmony {
    Monochromatic,
    Analogous,
    Complementary,
    #[default]
    NeutralDominant,
}

/// WCAG contrast level a foreground role must reach against its surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WcagLevel {
    AA,  // 4.5:1
    AAA, // 7:1
}

impl WcagLevel {
    pub fn ratio(self) -> f64 {
        match self {
            WcagLevel::AA => 4.5,
            WcagLevel::AAA => 7.0,
        }
    }
}

/// Relations describing one role relative to its parent colour.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeRole {
    /// Lightness relative to the parent (anchor for surface, surface for
    /// foreground and border).
    pub lightness: PropertyRelation,
    /// Chroma relative to the parent.
    pub chroma: PropertyRelation,
    /// Hue offset in degrees relative to the parent (harmony adjustments).
    pub hue_shift: f64,
    /// Opacity applied at compositing time (1.0 = opaque).
    pub opacity: f64,
    /// Required contrast against the composited background, if any.
    pub contrast: Option<WcagLevel>,
    /// Brand-locked properties preserve hue and chroma across polarity.
    pub brand_lock: bool,
}

impl Default for ThemeRole {
    fn default() -> Self {
        ThemeRole {
            lightness: PropertyRelation::Absolute,
            chroma: PropertyRelation::Absolute,
            hue_shift: 0.0,
            opacity: 1.0,
            contrast: None,
            brand_lock: false,
        }
    }
}

/// A relational theme: one absolute anchor, everything else derived.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Absolute anchor colour in OKLCH.
    pub anchor: Oklch,
    pub harmony: Harmony,
    pub surface: ThemeRole,
    pub foreground: ThemeRole,
    pub accent: ThemeRole,
    pub border: ThemeRole,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_role_is_neutral_inheritance() {
        let r = ThemeRole::default();
        assert_eq!(r.lightness, PropertyRelation::Absolute);
        assert_eq!(r.opacity, 1.0);
        assert!(!r.brand_lock);
    }
}
