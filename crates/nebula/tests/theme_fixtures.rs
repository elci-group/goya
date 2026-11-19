//! Theme reading against fixture copies of real COSMIC config.

use goya_chroma::{Polarity, Rgb8, WcagLevel};
use nebula::probe::theme_summary;
use nebula::theme::{
    geometry, load, load_active, map_to_chroma, read_mode, KeyProblem, ThemeError, ThemeSource,
    Variant,
};
use std::path::PathBuf;

fn fixture(name: &str) -> ThemeSource {
    ThemeSource::new(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
}

#[test]
fn real_dark_theme_reads_without_issues() {
    let loaded = load(&fixture("pop-2026-11-19"), Variant::Dark).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let t = &loaded.theme;
    assert_eq!(t.name.as_deref(), Some("cosmic-dark"));
    assert_eq!(t.is_dark, Some(true));
    assert_eq!(t.is_high_contrast, Some(false));
    assert_eq!(t.gaps, Some((0, 8)));
    assert_eq!(t.active_hint, Some(3));
    let r = t.corner_radii.as_ref().unwrap();
    assert_eq!(r.radius_xs, Some([4.0; 4]));
    assert_eq!(r.radius_xl, Some([160.0; 4]));
    assert_eq!(t.spacing.as_ref().unwrap().space_xxxl, Some(128));
    // The unknown `palette` key in the directory is not read.
    assert!(loaded.other_versions_present.is_empty());
}

#[test]
fn real_dark_theme_maps_to_chroma() {
    let loaded = load(&fixture("pop-2026-11-19"), Variant::Dark).unwrap();
    let m = map_to_chroma(&loaded);
    assert_eq!(m.polarity, Polarity::Dark);
    assert_eq!(m.wcag_target, WcagLevel::AA);
    assert!(m.palette_complete);
    assert!(m.unmapped.is_empty(), "{:?}", m.unmapped);
    let p = m.palette().unwrap();
    assert_eq!(p.bg, Rgb8::new(0x1B, 0x1B, 0x1B));
    assert_eq!(p.accent, Rgb8::new(0x63, 0xD0, 0xDF));
    assert_eq!(p.err, Rgb8::new(0xFD, 0xA1, 0xA0));
    assert_eq!(p.ok, Rgb8::new(0x92, 0xCF, 0x9C));
    assert_eq!(m.roles.warning, Some(Rgb8::new(0xF7, 0xE0, 0x62)));
    // Anchor is the accent in OKLCH; cyan sits near hue 200.
    let a = m.anchor.unwrap();
    assert!(a.h > 180.0 && a.h < 220.0, "{a:?}");
    // fg/bg is measured, and passes AA on this theme.
    let fg = m.contrast.iter().find(|c| c.pair == "fg/bg").unwrap();
    assert!(fg.meets_target && fg.ratio > 10.0);
}

#[test]
fn light_theme_and_mode() {
    let src = fixture("pop-2026-11-19");
    let mode = read_mode(&src).unwrap();
    assert_eq!(mode.selected, Variant::Dark);
    let light = load(&src, Variant::Light).unwrap();
    assert!(light.issues.is_empty());
    assert_eq!(light.theme.name.as_deref(), Some("cosmic-light"));
    assert_eq!(map_to_chroma(&light).polarity, Polarity::Light);
    let active = load_active(&src).unwrap();
    assert_eq!(active.loaded.variant, Variant::Dark);
    assert!(active.mode_error.is_none());
}

#[test]
fn bad_and_missing_keys_degrade_only_themselves() {
    let src = fixture("degraded");
    let loaded = load(&src, Variant::Dark).unwrap();
    let problem = |k: &str| {
        loaded
            .issues
            .iter()
            .find(|i| i.key == k)
            .map(|i| i.problem.clone())
    };
    assert!(matches!(problem("accent"), Some(KeyProblem::Unparsable(_))));
    assert_eq!(problem("background"), Some(KeyProblem::Missing));
    assert_eq!(problem("is_dark"), Some(KeyProblem::Missing));
    // primary carries an unknown field and still reads.
    assert_eq!(problem("primary"), None);
    assert!(loaded.theme.primary.is_some());
    assert_eq!(loaded.issues.len(), 3, "{:?}", loaded.issues);
    // Geometry is untouched by the colour failures.
    assert_eq!(geometry(&loaded).gaps, Some((0, 8)));

    let m = map_to_chroma(&loaded);
    assert_eq!(m.polarity, Polarity::Dark);
    assert!(m.polarity_basis.contains("degraded"));
    assert!(!m.palette_complete);
    assert!(m.palette().is_none());
    assert!(m.anchor.is_none());
    assert!(m.roles.ok.is_some() && m.roles.err.is_some());
    assert!(m.unmapped.iter().any(|u| u.starts_with("bg ")));
    assert!(m.unmapped.iter().any(|u| u.starts_with("accent ")));

    // A bad mode key falls back to Dark and says so.
    let mode = read_mode(&src).unwrap();
    assert_eq!(mode.selected, Variant::Dark);
    assert!(mode.basis.contains("unreadable"));
    assert!(matches!(mode.issues[0].problem, KeyProblem::Unparsable(_)));
}

#[test]
fn v2_only_config_is_refused_explicitly() {
    let src = fixture("v2-only");
    match load(&src, Variant::Dark) {
        Err(ThemeError::FormatNotPresent {
            found, supported, ..
        }) => {
            assert_eq!(found, ["v2"]);
            assert_eq!(supported, "v1");
        }
        other => panic!("expected FormatNotPresent, got {other:?}"),
    }
    let s = theme_summary(Some(&src), None);
    assert!(s.error.is_some());
    assert!(s.mode_error.is_some());
    assert!(s.chroma.is_none());
}

#[test]
fn missing_root_is_an_error_not_a_panic() {
    let s = theme_summary(Some(&fixture("does-not-exist")), None);
    assert!(matches!(
        s.error,
        Some(ThemeError::FormatNotPresent { ref found, .. }) if found.is_empty()
    ));
    let s = theme_summary(None, None);
    assert_eq!(s.error, Some(ThemeError::NoConfigRoot));
}
