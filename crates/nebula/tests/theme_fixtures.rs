//! Theme reading against fixture copies of real COSMIC config.

use goya_chroma::{Polarity, Rgb8, WcagLevel};
use nebula::probe::theme_summary;
use nebula::theme::{
    geometry, load, load_active, map_to_chroma, read_mode, KeyProblem, ThemeError, ThemeSource,
    Variant, MAX_THEME_FILE_BYTES,
};
use std::path::PathBuf;

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

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
fn oversized_theme_key_is_bounded_and_degrades_only_that_key() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "nebula-oversized-theme-{}-{stamp}",
        std::process::id()
    ));
    let dir = root.join("cosmic/com.system76.CosmicTheme.Dark/v1");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("background"), vec![b' '; MAX_THEME_FILE_BYTES + 1]).unwrap();
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pop-2026-11-19/cosmic/com.system76.CosmicTheme.Dark/v1/accent"),
        dir.join("accent"),
    )
    .unwrap();

    let loaded = load(&ThemeSource::new(&root), Variant::Dark).unwrap();
    let oversized = loaded.issues.iter().find(|issue| issue.key == "background");
    assert!(matches!(
        oversized.map(|issue| &issue.problem),
        Some(KeyProblem::TooLarge { limit_bytes }) if *limit_bytes == MAX_THEME_FILE_BYTES
    ));
    assert!(loaded.theme.background.is_none());
    assert!(
        loaded.theme.accent.is_some(),
        "a separate valid key still loads"
    );
    assert!(!loaded.issues.iter().any(|issue| issue.key == "accent"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn active_digest_uses_the_bytes_parsed_including_mode() {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "nebula-theme-snapshot-{}-{stamp}",
        std::process::id()
    ));
    let mode_dir = root.join("cosmic/com.system76.CosmicTheme.Mode/v1");
    std::fs::create_dir_all(&mode_dir).unwrap();
    let mode_path = mode_dir.join("is_dark");
    std::fs::write(&mode_path, b"true").unwrap();

    let accent_fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pop-2026-11-19/cosmic/com.system76.CosmicTheme.Dark/v1/accent");
    for variant in ["Dark", "Light"] {
        let dir = root.join(format!("cosmic/com.system76.CosmicTheme.{variant}/v1"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&accent_fixture, dir.join("accent")).unwrap();
    }

    let active = load_active(&ThemeSource::new(&root)).unwrap();
    assert_eq!(active.loaded.variant, Variant::Dark);
    let mode_digest = active
        .source_files
        .iter()
        .find(|file| file.key.ends_with("CosmicTheme.Mode/v1/is_dark"))
        .expect("mode input is included in the active digest");
    assert_eq!(mode_digest.byte_len, 4);
    assert_eq!(mode_digest.sha256, sha256_hex(b"true"));
    assert!(active
        .source_files
        .iter()
        .any(|file| file.key.ends_with("CosmicTheme.Dark/v1/accent")));
    assert!(!active
        .source_files
        .iter()
        .any(|file| file.key.ends_with("CosmicTheme.Light/v1/accent")));

    let parsed_digest = active.files_sha256.clone();
    std::fs::write(&mode_path, b"false").unwrap();
    assert_eq!(active.files_sha256, parsed_digest);
    let reloaded = load_active(&ThemeSource::new(&root)).unwrap();
    assert_eq!(reloaded.loaded.variant, Variant::Light);
    assert_ne!(reloaded.files_sha256, parsed_digest);
    let reloaded_mode = reloaded
        .source_files
        .iter()
        .find(|file| file.key.ends_with("CosmicTheme.Mode/v1/is_dark"))
        .unwrap();
    assert_eq!(reloaded_mode.sha256, sha256_hex(b"false"));

    std::fs::remove_dir_all(root).unwrap();
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
