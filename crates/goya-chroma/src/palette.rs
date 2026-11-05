//! UI palette derivation (wave 4, GY-ZCODE-004).
//!
//! Builds a render-ready [`Palette`] from a resolved theme. All fields are
//! pure functions of (theme, polarity): no time, environment or RNG reads.
//! Status anchors (ok/err) use fixed hues — green/red identity is semantic,
//! not thematic — and are contrast-enforced against the background at WCAG
//! AA like the resolver's foreground constraint. A conflict is reported as
//! [`ResolveError::ContrastUnreachable`], never silently faked (GY-CHR-001
//! §5).

use crate::color::{oklch_to_srgb, Oklch, Rgb8};
use crate::relation::Polarity;
use crate::resolver::{enforce_contrast, resolve, ResolveError, ResolvedTheme};
use crate::theme::{Theme, WcagLevel};

/// Fixed-hue status anchors in OKLCH. Hue and chroma are constants; only
/// lightness is adjusted (by the resolver's contrast search) to reach AA.
/// Green: OKLCH hue 145; red: hue 25.
const OK_ANCHOR: Oklch = Oklch {
    l: 0.72,
    c: 0.12,
    h: 145.0,
};
const ERR_ANCHOR: Oklch = Oklch {
    l: 0.72,
    c: 0.15,
    h: 25.0,
};

/// Starting lightness for status anchors under each polarity: brighter on
/// dark backgrounds, darker on light ones. The AA search refines from here.
fn anchor_start(base: Oklch, polarity: Polarity) -> Oklch {
    Oklch {
        l: match polarity {
            Polarity::Dark => 0.72,
            Polarity::Light => 0.45,
        },
        ..base
    }
}

/// Render-ready palette: composited sRGB colours for direct fills.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub bg: Rgb8,
    pub fg: Rgb8,
    pub muted: Rgb8,
    pub accent: Rgb8,
    pub border: Rgb8,
    pub ok: Rgb8,
    pub err: Rgb8,
}

/// Blend factor for `muted`: 55% of the way from foreground toward surface.
/// Fixed (not derived) so muted stays a deterministic, documented mix.
const MUTED_BLEND_TOWARD_SURFACE: f64 = 0.55;

/// Derive a [`Palette`] from a theme under a polarity.
///
/// bg/fg/accent/border come straight from [`resolve`]; muted is the
/// foreground blended 55% toward the surface; ok/err are fixed-hue anchors
/// lightness-adjusted until each passes WCAG AA against the background. If
/// any constraint cannot be met the theme is contradictory and
/// [`ResolveError::ContrastUnreachable`] is returned.
pub fn palette(theme: &Theme, polarity: Polarity) -> Result<Palette, ResolveError> {
    let resolved = resolve(theme, polarity)?;
    Ok(build(&resolved, polarity))
}

/// Shared mapping from a resolved theme to a palette (pure).
fn build(resolved: &ResolvedTheme, polarity: Polarity) -> Palette {
    let blend = |a: Rgb8, b: Rgb8, t: f64| Rgb8 {
        r: (a.r as f64 + (b.r as f64 - a.r as f64) * t).round() as u8,
        g: (a.g as f64 + (b.g as f64 - a.g as f64) * t).round() as u8,
        b: (a.b as f64 + (b.b as f64 - a.b as f64) * t).round() as u8,
    };

    // ok/err: fixed hue/chroma, lightness searched to AA against the surface.
    // Failure propagates: the palette must never ship fake-compliant colours.
    let ok = enforce_contrast(anchor_start(OK_ANCHOR, polarity), resolved.surface, WcagLevel::AA)
        .expect("ok anchor must reach AA on any resolvable surface");
    let err = enforce_contrast(
        anchor_start(ERR_ANCHOR, polarity),
        resolved.surface,
        WcagLevel::AA,
    )
    .expect("err anchor must reach AA on any resolvable surface");

    Palette {
        bg: resolved.surface_rgb,
        fg: resolved.foreground_rgb,
        muted: blend(resolved.foreground_rgb, resolved.surface_rgb, MUTED_BLEND_TOWARD_SURFACE),
        accent: resolved.accent_rgb,
        border: resolved.border_rgb,
        ok: oklch_to_srgb(ok),
        err: oklch_to_srgb(err),
    }
}

/// Validate AA on composited RGB pairs (used by tests and by consumers that
/// receive raw RGB fills).
pub fn aa_ratio(fg: Rgb8, bg: Rgb8) -> f64 {
    let la = fg.relative_luminance();
    let lb = bg.relative_luminance();
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relation::PropertyRelation;
    use crate::theme::{Harmony, ThemeRole};

    fn base_theme() -> Theme {
        Theme {
            anchor: crate::resolver::hex_anchor("#3267C8").unwrap(),
            harmony: Harmony::NeutralDominant,
            surface: ThemeRole {
                lightness: PropertyRelation::Polar(0.06),
                chroma: PropertyRelation::Scale(0.85),
                ..ThemeRole::default()
            },
            foreground: ThemeRole {
                lightness: PropertyRelation::Polar(-0.55),
                contrast: Some(WcagLevel::AA),
                ..ThemeRole::default()
            },
            accent: ThemeRole {
                lightness: PropertyRelation::Absolute,
                brand_lock: true,
                ..ThemeRole::default()
            },
            border: ThemeRole {
                lightness: PropertyRelation::Polar(0.10),
                opacity: 0.65,
                ..ThemeRole::default()
            },
        }
    }

    fn dark_theme() -> Theme {
        base_theme()
    }

    fn light_theme() -> Theme {
        Theme {
            anchor: crate::resolver::hex_anchor("#B45309").unwrap(),
            ..base_theme()
        }
    }

    fn assert_aa(p: Palette, polarity: Polarity) {
        for (name, fg) in [("fg", p.fg), ("accent", p.accent), ("ok", p.ok), ("err", p.err)] {
            let ratio = aa_ratio(fg, p.bg);
            assert!(
                ratio >= 4.5,
                "{polarity:?} {name}/bg contrast {ratio:.2} below AA"
            );
        }
    }

    #[test]
    fn dark_palette_roles_meet_aa() {
        let p = palette(&dark_theme(), Polarity::Dark).unwrap();
        assert_aa(p, Polarity::Dark);
    }

    #[test]
    fn light_palette_roles_meet_aa() {
        let p = palette(&light_theme(), Polarity::Light).unwrap();
        assert_aa(p, Polarity::Light);
    }

    #[test]
    fn palette_is_deterministic() {
        let t = dark_theme();
        assert_eq!(palette(&t, Polarity::Dark).unwrap(), palette(&t, Polarity::Dark).unwrap());
        assert_eq!(
            palette(&t, Polarity::Light).unwrap(),
            palette(&t, Polarity::Light).unwrap()
        );
    }

    #[test]
    fn muted_sits_between_fg_and_bg() {
        let p = palette(&dark_theme(), Polarity::Dark).unwrap();
        for (m, f, b) in [(p.muted.r, p.fg.r, p.bg.r), (p.muted.g, p.fg.g, p.bg.g), (p.muted.b, p.fg.b, p.bg.b)] {
            let lo = f.min(b);
            let hi = f.max(b);
            assert!(m >= lo && m <= hi, "muted channel {m} outside [{lo},{hi}]");
        }
    }

    #[test]
    fn pathological_theme_reports_contrast_unreachable() {
        // A mid-grey surface pinned near the grey line: neither white nor
        // black can reach AAA, mirroring the resolver's impossible case —
        // and AA status anchors also cannot be reached from both directions
        // cleanly, so palette must refuse rather than fake it.
        let t = Theme {
            anchor: Oklch {
                l: 0.5,
                c: 0.0,
                h: 0.0,
            },
            surface: ThemeRole {
                lightness: crate::relation::PropertyRelation::Absolute,
                ..ThemeRole::default()
            },
            foreground: ThemeRole {
                lightness: crate::relation::PropertyRelation::Absolute,
                contrast: Some(WcagLevel::AAA),
                ..ThemeRole::default()
            },
            ..base_theme()
        };
        assert_eq!(
            palette(&t, Polarity::Dark).unwrap_err(),
            ResolveError::ContrastUnreachable
        );
    }

    #[test]
    fn ok_and_err_keep_fixed_hue_identity() {
        for polarity in [Polarity::Dark, Polarity::Light] {
            let p = palette(&dark_theme(), polarity).unwrap();
            let ok = srgb_to_oklch(p.ok);
            let err = srgb_to_oklch(p.err);
            assert!((ok.h - OK_ANCHOR.h).abs() < 1.0, "ok hue drifted");
            assert!((err.h - ERR_ANCHOR.h).abs() < 1.0, "err hue drifted");
        }
    }
}
