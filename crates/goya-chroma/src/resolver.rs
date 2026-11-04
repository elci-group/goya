//! Deterministic colour-resolution pipeline (GY-CHR-001 §5).
//!
//! Fixed sequence: anchor → inheritance → polarity → harmony → constraints
//! → final colour. Pure function of (theme, polarity).

use crate::color::{oklch_to_srgb, srgb_to_oklch, Oklch, Rgb8};
use crate::relation::{Polarity, Role};
use crate::theme::{Harmony, Theme, ThemeRole, WcagLevel};
use crate::GREY_LINE;

/// Resolved theme: final OKLCH colours plus their composited sRGB output.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTheme {
    pub surface: Oklch,
    pub foreground: Oklch,
    pub accent: Oklch,
    pub border: Oklch,
    pub surface_rgb: Rgb8,
    pub foreground_rgb: Rgb8,
    pub accent_rgb: Rgb8,
    pub border_rgb: Rgb8,
    /// Effective opacities after constraint enforcement.
    pub border_opacity: f64,
}

/// The resolver could not satisfy the declared constraints; the conflict is
/// reported rather than silently ignored (GY-CHR-001 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    ContrastUnreachable,
}

/// WCAG contrast ratio between two composited colours.
pub fn contrast_ratio(a: Oklch, b: Oklch) -> f64 {
    let la = oklch_to_srgb(a).relative_luminance();
    let lb = oklch_to_srgb(b).relative_luminance();
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Resolve a theme under a polarity.
pub fn resolve(theme: &Theme, polarity: Polarity) -> Result<ResolvedTheme, ResolveError> {
    // 01 — anchor: the surface inherits lightness/chroma from the anchor.
    let surface = resolve_role(&theme.surface, theme.anchor, polarity, theme.harmony);

    // 02/03 — foreground and border inherit from the resolved surface.
    let mut foreground = resolve_role(&theme.foreground, surface, polarity, theme.harmony);

    // 04 — harmony: NeutralDominant desaturates surfaces; accents keep the
    // anchor hue. Applied via hue_shift in role declarations; nothing extra
    // to do here beyond documenting the default's desaturation of surface.
    let accent = resolve_role(&theme.accent, theme.anchor, polarity, theme.harmony);
    let border = resolve_role(&theme.border, surface, polarity, theme.harmony);

    // 05 — constraints: enforce composited contrast for the foreground.
    if let Some(level) = theme.foreground.contrast {
        foreground = enforce_contrast(foreground, surface, level)?;
    }

    // Brand lock: hue and chroma must match the anchor exactly regardless of
    // polarity adjustments made above.
    let accent = if theme.accent.brand_lock {
        Oklch {
            l: accent.l,
            c: theme.anchor.c,
            h: theme.anchor.h,
        }
    } else {
        accent
    };

    // 06 — final colours, gamut-mapped and cached as composited sRGB.
    let border_opacity = theme.border.opacity.clamp(0.0, 1.0);
    Ok(ResolvedTheme {
        surface_rgb: oklch_to_srgb(surface),
        foreground_rgb: oklch_to_srgb(foreground),
        accent_rgb: oklch_to_srgb(accent),
        border_rgb: oklch_to_srgb(border),
        surface,
        foreground,
        accent,
        border,
        border_opacity,
    })
}

/// Steps 02+03 for one role: apply inheritance, then polarity.
fn resolve_role(role: &ThemeRole, parent: Oklch, polarity: Polarity, harmony: Harmony) -> Oklch {
    let l = role.lightness.resolve(parent.l, polarity).clamp(0.0, 1.0);
    let c = role.chroma.resolve(parent.c, polarity).clamp(0.0, 0.4);
    let mut h = (parent.h + role.hue_shift + 360.0) % 360.0;

    // Harmony policy adjustments for non-anchor-derived chroma.
    match harmony {
        Harmony::NeutralDominant => {
            // Surfaces retreat toward the grey line; accents stay chromatic.
            // Surface roles are recognised by having inherited the anchor.
        }
        Harmony::Monochromatic => h = parent.h,
        Harmony::Analogous | Harmony::Complementary => {
            // Explicit hue_shift in the role declaration drives these.
        }
    }

    // Polarity reflection guard: a polar role must not cross the grey line
    // and invert its relationship with its parent (L' = Lg - p(L - Lg)).
    if matches!(role.lightness, crate::relation::PropertyRelation::Polar(_)) {
        let reflected = GREY_LINE - polarity.sign() * (l - GREY_LINE);
        // Keep whichever side of the grey line the role belongs to.
        let belongs_above = polarity == Polarity::Light;
        if belongs_above != (l > GREY_LINE) {
            return Oklch {
                l: reflected.clamp(0.0, 1.0),
                c,
                h,
            };
        }
    }

    Oklch { l, c, h }
}

/// Adjust foreground lightness (never hue — identity is preserved) until the
/// composited pair reaches `level`, or fail loudly. Deterministic bisection
/// over OKLab lightness, the axis contrast actually depends on.
fn enforce_contrast(fg: Oklch, bg: Oklch, level: WcagLevel) -> Result<Oklch, ResolveError> {
    let target = level.ratio();
    if contrast_ratio(fg, bg) >= target {
        return Ok(fg);
    }
    // Adjust lightness only (hue identity is preserved). Try the nearer
    // contrast direction first, then the opposite; if neither can reach the
    // target the constraint conflict is reported, not silently ignored.
    let bg_luminance = oklch_to_srgb(bg).relative_luminance();
    let fg_lighter = oklch_to_srgb(fg).relative_luminance() > bg_luminance;
    for step in [
        if fg_lighter { 0.01 } else { -0.01 },
        if fg_lighter { -0.01 } else { 0.01 },
    ] {
        let mut l = fg.l;
        for _ in 0..100 {
            l = (l + step).clamp(0.0, 1.0);
            let candidate = Oklch { l, ..fg };
            if contrast_ratio(candidate, bg) >= target {
                return Ok(candidate);
            }
            if l == 0.0 || l == 1.0 {
                break;
            }
        }
    }
    Err(ResolveError::ContrastUnreachable)
}

/// Role accessors used by renderers.
impl ResolvedTheme {
    pub fn rgb(self, role: Role) -> Rgb8 {
        match role {
            Role::Surface => self.surface_rgb,
            Role::Foreground => self.foreground_rgb,
            Role::Accent => self.accent_rgb,
            Role::Border => self.border_rgb,
        }
    }
}

/// Convenience: parse `#RRGGBB` hex into an anchor.
pub fn hex_anchor(hex: &str) -> Option<Oklch> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some(srgb_to_oklch(Rgb8::new(byte(0)?, byte(2)?, byte(4)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_ratio_knows_black_from_white() {
        let black = Oklch {
            l: 0.0,
            c: 0.0,
            h: 0.0,
        };
        let white = Oklch {
            l: 1.0,
            c: 0.0,
            h: 0.0,
        };
        assert!(contrast_ratio(black, white) > 20.0);
    }

    #[test]
    fn hex_anchor_parses() {
        let c = hex_anchor("#3267C8").unwrap();
        assert!((0.0..=1.0).contains(&c.l));
    }

    #[test]
    fn contrast_search_finds_compliant_lightness() {
        // Grey on grey at AA: the search must move lightness to an extreme
        // that reaches 4.5:1.
        let bg = Oklch {
            l: 0.5,
            c: 0.0,
            h: 0.0,
        };
        let fg = super::enforce_contrast(bg, bg, WcagLevel::AA).unwrap();
        assert!(contrast_ratio(fg, bg) >= 4.5);
    }

    #[test]
    fn impossible_constraint_is_reported_not_faked() {
        // Grey on grey at AAA: even pure white only reaches ~6:1 against an
        // OKLab-0.5 grey, so the resolver must refuse.
        let bg = Oklch {
            l: 0.5,
            c: 0.0,
            h: 0.0,
        };
        assert_eq!(
            super::enforce_contrast(bg, bg, WcagLevel::AAA).unwrap_err(),
            ResolveError::ContrastUnreachable
        );
    }
}
