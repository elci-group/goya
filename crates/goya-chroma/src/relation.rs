//! Chromatic primitives: relations, polarity and roles (GY-CHR-001 §3).

use std::fmt;

/// Which side of the grey-line lightness relationship a theme resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Dark,
    Light,
}

impl Polarity {
    /// Signed polarity factor `p` in the grey-line reflection
    /// `L' = Lg - p·(L - Lg)`. Dark inverts relationships (-1), Light
    /// preserves them (+1). See GY-CHR-001 §2.
    pub fn sign(self) -> f64 {
        match self {
            Polarity::Dark => -1.0,
            Polarity::Light => 1.0,
        }
    }
}

impl fmt::Display for Polarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Polarity::Dark => "dark",
            Polarity::Light => "light",
        })
    }
}

/// Semantic purpose of a colour in the hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Surface,
    Foreground,
    Accent,
    Border,
}

/// A parent-relative transformation of one chromatic property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropertyRelation {
    /// Inherit the parent value unchanged.
    Absolute,
    /// Signed offset in OKLab lightness units, preserved across polarity.
    Delta(f64),
    /// Multiplicative scale (chroma, opacity).
    Scale(f64),
    /// Signed lightness relationship whose sign flips with theme polarity:
    /// `polar(+x)` raises in light mode and lowers in dark mode.
    Polar(f64),
}

impl PropertyRelation {
    /// Resolve this relation against a parent value under `polarity`.
    pub fn resolve(self, parent: f64, polarity: Polarity) -> f64 {
        match self {
            PropertyRelation::Absolute => parent,
            PropertyRelation::Delta(d) => parent + d,
            PropertyRelation::Scale(s) => parent * s,
            PropertyRelation::Polar(d) => parent + d * polarity.sign(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polar_flips_with_polarity() {
        let rel = PropertyRelation::Polar(0.1);
        assert_eq!(rel.resolve(0.5, Polarity::Light), 0.6);
        assert_eq!(rel.resolve(0.5, Polarity::Dark), 0.4);
    }

    #[test]
    fn delta_is_polarity_independent() {
        let rel = PropertyRelation::Delta(0.1);
        assert_eq!(rel.resolve(0.5, Polarity::Light), 0.6);
        assert_eq!(rel.resolve(0.5, Polarity::Dark), 0.6);
    }
}
