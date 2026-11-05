//! GY-CHROMA/1 — Chromatic relativism engine.
//!
//! Colour is relational, not a bag of hex values. A theme declares one
//! absolute anchor plus parent-relative transformations in the perceptual
//! OKLCH space; the resolver derives a coherent hierarchy deterministically
//! and enforces accessibility constraints on the *composited* pairs.
//!
//! Contract: resolution is a pure function of (theme, polarity). No time,
//! environment or RNG dependence. See docs/contracts.md.
//!
//! On top of role resolution, [`palette`] derives a render-ready UI palette
//! (bg/fg/muted/accent/border plus fixed-hue ok/err status anchors) with AA
//! contrast enforced on every foreground-like field against the background.

mod color;
mod palette;
mod relation;
mod resolver;
mod theme;

pub use color::{oklch_to_srgb, srgb_to_oklch, Oklch, Rgb8};
pub use palette::{aa_ratio, palette, Palette};
pub use relation::{Polarity, PropertyRelation, Role};
pub use resolver::{hex_anchor, resolve, ResolveError, ResolvedTheme};
pub use theme::{Harmony, Theme, ThemeRole, WcagLevel};

/// Contract identifier implemented by this crate.
pub const CHROMA_CONTRACT: &str = "GY-CHROMA/1";

/// Grey-line pivot in OKLab lightness (0..1). Colours at this lightness are
/// perceptually neutral pivots; polarity reflects relationships around it.
pub const GREY_LINE: f64 = 0.6;

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme {
            anchor: srgb_to_oklch(Rgb8::new(0x32, 0x67, 0xC8)),
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

    #[test]
    fn oklch_round_trip_within_tolerance() {
        let original = Rgb8::new(0x32, 0x67, 0xC8);
        let c = srgb_to_oklch(original);
        let back = oklch_to_srgb(c);
        for (a, b) in [original.r, original.g, original.b]
            .iter()
            .zip([back.r, back.g, back.b])
        {
            assert!(
                (*a as i32 - b as i32).abs() <= 1,
                "round trip drifted: {original:?} -> {back:?}"
            );
        }
    }

    #[test]
    fn resolution_is_deterministic() {
        let t = theme();
        assert_eq!(
            resolve(&t, Polarity::Dark).unwrap(),
            resolve(&t, Polarity::Dark).unwrap()
        );
    }

    #[test]
    fn polarity_reflects_lightness_around_grey_line() {
        let t = theme();
        let dark = resolve(&t, Polarity::Dark).unwrap();
        let light = resolve(&t, Polarity::Light).unwrap();
        // Surface lightness relationship inverts across the grey line.
        assert!(
            dark.surface.l < GREY_LINE,
            "dark surface sits below grey line"
        );
        assert!(
            light.surface.l > GREY_LINE,
            "light surface sits above grey line"
        );
    }

    #[test]
    fn brand_lock_preserves_hue_across_polarity() {
        let t = theme();
        let dark = resolve(&t, Polarity::Dark).unwrap();
        let light = resolve(&t, Polarity::Light).unwrap();
        assert_eq!(dark.accent.h, light.accent.h, "locked hue must not change");
        assert_eq!(
            dark.accent.c, light.accent.c,
            "locked chroma must not change"
        );
    }

    #[test]
    fn contrast_constraint_enforced_on_composited_pair() {
        let t = theme();
        for polarity in [Polarity::Dark, Polarity::Light] {
            let theme = resolve(&t, polarity).unwrap();
            let ratio = resolver::contrast_ratio(theme.foreground, theme.surface);
            assert!(
                ratio >= 4.5,
                "{polarity:?} foreground/surface contrast {ratio:.2} below WCAG AA"
            );
        }
    }

    #[test]
    fn opposite_polarities_differ_but_share_hue_identity() {
        let t = theme();
        let dark = resolve(&t, Polarity::Dark).unwrap();
        let light = resolve(&t, Polarity::Light).unwrap();
        assert_ne!(dark.surface, light.surface);
        // Hue identity is preserved: polarity is not an RGB complement.
        assert!(
            (dark.surface.h - light.surface.h).abs() < 1.0,
            "surface hue should be preserved across polarity"
        );
    }
}
