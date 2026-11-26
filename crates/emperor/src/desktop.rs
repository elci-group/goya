//! Desktop profiles: `generic` (EMP-M1, the default) and `cosmic` (Nebula).
//!
//! A desktop profile is presentation only. It never changes admission, the
//! native IR, or the oracle. `cosmic` resolves the active COSMIC theme here,
//! in the compiler, through `nebula::theme`, and bakes the colours into a
//! generated `presentation.rs`. The linked App reads no theme file and calls
//! no service, so its capability manifest is unchanged and live theme
//! changes are not followed. If the theme cannot be read, or the roles the
//! counter paints with are missing, the build falls back to the generic
//! constants and records why.

use crate::{sha256_hex, Fault};
use emperor_runtime::{Paint, Presentation};
use goya_chroma::{aa_ratio, Rgb8};
use nebula::theme::{self, ThemeSource};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const GENERIC: &str = "generic";
pub const COSMIC: &str = "cosmic";

/// Schema of the theme record in the build report.
pub const THEME_RECORD_SCHEMA: &str = "emperor.cosmic-theme/1";

/// How [`theme_digest`] is computed.
pub const DIGEST_METHOD: &str = "emperor.cosmic-theme-digest/1: sha256 over the domain line, then Mode/v1/is_dark, then each nebula::theme::KEYS entry of <Variant>/v1, each as label NUL (\"present\" NUL byte-length NUL bytes | \"absent\") LF. Paths are not hashed.";

const MODE_COMPONENT: &str = "com.system76.CosmicTheme.Mode";

/// Validate a `--desktop` value.
pub fn parse(value: &str) -> Result<&'static str, Fault> {
    match value {
        GENERIC => Ok(GENERIC),
        COSMIC => Ok(COSMIC),
        other => Err(Fault::new(
            2,
            "EMP-USAGE",
            format!("unknown desktop profile {other}; expected generic or cosmic"),
        )),
    }
}

/// `cosmic` is a Wayland profile. The X11 host under COSMIC is Xwayland.
pub fn check_window(desktop: &str, window: &str) -> Result<(), Fault> {
    if desktop == COSMIC && window != "wayland" {
        return Err(Fault::new(
            2,
            "EMP-USAGE",
            format!(
                "--desktop cosmic requires --window wayland; {window} under COSMIC is Xwayland, not COSMIC"
            ),
        ));
    }
    Ok(())
}

/// Colours for one `cosmic` build and the record of where they came from.
#[derive(Debug, Clone)]
pub struct CosmicBake {
    pub paint: Paint,
    /// True when the COSMIC theme colours were applied; false on fallback.
    pub applied: bool,
    pub record: Value,
}

fn rgb_hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn argb(rgb: Rgb8) -> u32 {
    0xFF00_0000 | (u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b)
}

fn triple(rgb: Rgb8) -> [u8; 3] {
    [rgb.r, rgb.g, rgb.b]
}

fn fill_hex(value: Option<u32>) -> Value {
    match value {
        Some(v) => json!(format!("#{:06X}", v & 0x00FF_FFFF)),
        None => Value::Null,
    }
}

/// The colours a paint uses, as `#RRGGBB` strings (`null` keeps the host default).
pub fn paint_json(paint: &Paint) -> Value {
    json!({
        "fg": rgb_hex(paint.fg),
        "bg": rgb_hex(paint.bg),
        "client_fill": fill_hex(paint.client_fill),
        "field_fill": fill_hex(paint.field_fill),
        "focus_fill": fill_hex(Some(paint.focus_fill)),
        "focus_fg": fill_hex(paint.focus_fg),
    })
}

/// The presentation block of a build report.
pub fn presentation_json(look: &Presentation) -> Value {
    let resize = match look.resize {
        emperor_runtime::ResizePolicy::Fixed => json!({"policy": "fixed"}),
        emperor_runtime::ResizePolicy::Proportional {
            min_scale_milli,
            max_scale_milli,
            preserve_aspect,
            anchor,
        } => json!({
            "policy": "proportional",
            "min_scale_milli": min_scale_milli,
            "max_scale_milli": max_scale_milli,
            "preserve_aspect": preserve_aspect,
            "anchor": format!("{anchor:?}"),
            "tiled_hint_rule": "all four tiled, none, maximized or fullscreen pass None (no anchoring information); only a partial set of tiled edges is passed to place",
            "pointer": "surface position -> Placement::to_design of the frame on screen -> floor to DIP -> goya_parity::pointer::hit_map",
        }),
    };
    json!({
        "desktop": look.desktop.name(),
        "paint": paint_json(&look.paint),
        "resize": resize,
    })
}

/// Digest of the theme files the bake read. See [`DIGEST_METHOD`].
pub fn theme_digest(config_root: &Path, variant_dir: &Path, variant_label: &str) -> String {
    let mut bytes = b"emperor.cosmic-theme-digest/1\n".to_vec();
    let mode = config_root
        .join("cosmic")
        .join(MODE_COMPONENT)
        .join(theme::COSMIC_THEME_FORMAT)
        .join("is_dark");
    let mut entries = vec![("Mode/v1/is_dark".to_owned(), mode)];
    for key in theme::KEYS {
        entries.push((format!("{variant_label}/v1/{key}"), variant_dir.join(key)));
    }
    for (label, path) in entries {
        bytes.extend_from_slice(label.as_bytes());
        bytes.push(0);
        match std::fs::read(&path) {
            Ok(content) => {
                bytes.extend_from_slice(b"present\0");
                bytes.extend_from_slice(content.len().to_string().as_bytes());
                bytes.push(0);
                bytes.extend_from_slice(&content);
            }
            Err(_) => bytes.extend_from_slice(b"absent"),
        }
        bytes.push(b'\n');
    }
    sha256_hex(&bytes)
}

fn ratio(a: Rgb8, b: Rgb8) -> f64 {
    (aa_ratio(a, b) * 100.0).round() / 100.0
}

/// Resolve the active COSMIC theme under `source` into a paint.
///
/// `None` means no config root could be found. Required roles are `bg`
/// (`background.base`), `fg` (`background.on`) and `accent` (`accent.base`);
/// when any is missing every colour falls back to the generic constants.
pub fn bake_cosmic(source: Option<&ThemeSource>) -> CosmicBake {
    let mut record = Map::new();
    record.insert("schema".into(), json!(THEME_RECORD_SCHEMA));
    record.insert("nebula_version".into(), json!(nebula::NEBULA_VERSION));
    record.insert("format".into(), json!(theme::COSMIC_THEME_FORMAT));
    record.insert(
        "live_theme_changes".into(),
        json!("not followed: colours are resolved by the compiler and baked into the binary; the App reads no theme file and calls no service"),
    );
    record.insert("digest_method".into(), json!(DIGEST_METHOD));
    let fallback = |mut record: Map<String, Value>, reason: String| {
        let paint = Paint::GENERIC;
        record.insert("applied".into(), json!(false));
        record.insert("fallback_reason".into(), json!(reason));
        record.insert("colours".into(), paint_json(&paint));
        record.insert(
            "colour_sources".into(),
            json!("generic EMP-M1 constants (fallback)"),
        );
        CosmicBake {
            paint,
            applied: false,
            record: Value::Object(record),
        }
    };
    let Some(source) = source else {
        record.insert("config_root".into(), Value::Null);
        return fallback(
            record,
            "no config root: XDG_CONFIG_HOME is not absolute and HOME is unset".to_owned(),
        );
    };
    record.insert(
        "config_root".into(),
        json!(source.config_root.display().to_string()),
    );
    let active = match theme::load_active(source) {
        Ok(active) => active,
        Err(error) => {
            record.insert("digest_sha256".into(), Value::Null);
            return fallback(record, format!("theme unreadable: {error}"));
        }
    };
    let loaded = &active.loaded;
    let variant_label = match loaded.variant {
        theme::Variant::Dark => "Dark",
        theme::Variant::Light => "Light",
    };
    record.insert("variant".into(), json!(variant_label.to_lowercase()));
    record.insert("dir".into(), json!(loaded.dir.display().to_string()));
    record.insert(
        "other_versions_present".into(),
        json!(loaded.other_versions_present),
    );
    record.insert("name".into(), json!(loaded.theme.name));
    record.insert(
        "mode".into(),
        serde_json::to_value(&active.mode).unwrap_or(Value::Null),
    );
    record.insert(
        "mode_error".into(),
        active
            .mode_error
            .as_ref()
            .map_or(Value::Null, |error| json!(error.to_string())),
    );
    record.insert(
        "issues".into(),
        serde_json::to_value(&loaded.issues).unwrap_or(Value::Null),
    );
    record.insert(
        "digest_sha256".into(),
        json!(theme_digest(
            &source.config_root,
            &loaded.dir,
            variant_label
        )),
    );
    let mapping = theme::map_to_chroma(loaded);
    let roles = &mapping.roles;
    let missing: Vec<&str> = [
        ("bg (background.base)", roles.bg.is_none()),
        ("fg (background.on)", roles.fg.is_none()),
        ("accent (accent.base)", roles.accent.is_none()),
    ]
    .iter()
    .filter(|(_, absent)| *absent)
    .map(|(name, _)| *name)
    .collect();
    let (Some(bg), Some(fg), Some(accent)) = (roles.bg, roles.fg, roles.accent) else {
        return fallback(
            record,
            format!("theme degraded: missing {}", missing.join(", ")),
        );
    };
    let field = roles.primary_container.unwrap_or(bg);
    let field_source = if roles.primary_container.is_some() {
        "primary.base"
    } else {
        "background.base (primary.base missing)"
    };
    let (focus_fg, focus_fg_source) = match roles.on_accent {
        Some(on) => (on, "accent.on"),
        None => {
            if aa_ratio(fg, accent) >= aa_ratio(bg, accent) {
                (
                    fg,
                    "background.on (accent.on missing; higher contrast on accent)",
                )
            } else {
                (
                    bg,
                    "background.base (accent.on missing; higher contrast on accent)",
                )
            }
        }
    };
    let paint = Paint {
        fg: triple(fg),
        bg: triple(bg),
        client_fill: Some(argb(bg)),
        field_fill: Some(argb(field)),
        focus_fill: argb(accent),
        focus_fg: Some(argb(focus_fg)),
    };
    record.insert("applied".into(), json!(true));
    record.insert("fallback_reason".into(), Value::Null);
    record.insert("colours".into(), paint_json(&paint));
    record.insert(
        "colour_sources".into(),
        json!({
            "fg": "background.on",
            "bg": "background.base",
            "client_fill": "background.base",
            "field_fill": field_source,
            "focus_fill": "accent.base",
            "focus_fg": focus_fg_source,
        }),
    );
    record.insert(
        "contrast".into(),
        json!({
            "note": "measured, not enforced; COSMIC colours are kept as authored",
            "wcag_target": format!("{:?}", mapping.wcag_target),
            "fg_on_bg": ratio(fg, bg),
            "fg_on_field": ratio(fg, field),
            "focus_fg_on_focus_fill": ratio(focus_fg, accent),
        }),
    );
    record.insert("palette_complete".into(), json!(mapping.palette_complete));
    record.insert("unmapped".into(), json!(mapping.unmapped));
    CosmicBake {
        paint,
        applied: true,
        record: Value::Object(record),
    }
}

/// Generated `presentation.rs` for a `cosmic` build.
pub fn presentation_rust(bake: &CosmicBake) -> String {
    let paint = &bake.paint;
    let fill = |value: Option<u32>| match value {
        Some(v) => format!("Some(0x{v:08X})"),
        None => "None".to_owned(),
    };
    let digest = bake
        .record
        .get("digest_sha256")
        .and_then(Value::as_str)
        .unwrap_or("none");
    let mut file = String::from("// Generated by Emperor ");
    file.push_str(crate::COMPILER_VERSION);
    file.push_str(". Do not edit.\n");
    file.push_str("// Desktop profile cosmic. Colours were resolved at build time (");
    file.push_str(if bake.applied {
        "COSMIC theme applied"
    } else {
        "fallback to the generic constants"
    });
    file.push_str(").\n// Theme digest sha256 ");
    file.push_str(digest);
    file.push_str(". Live theme changes are not followed.\n");
    file.push_str("pub static PRESENTATION: emperor_runtime::Presentation = emperor_runtime::Presentation::cosmic(emperor_runtime::Paint {\n");
    file.push_str(&format!(
        "    fg: [{}, {}, {}],\n    bg: [{}, {}, {}],\n    client_fill: {},\n    field_fill: {},\n    focus_fill: 0x{:08X},\n    focus_fg: {},\n",
        paint.fg[0],
        paint.fg[1],
        paint.fg[2],
        paint.bg[0],
        paint.bg[1],
        paint.bg[2],
        fill(paint.client_fill),
        fill(paint.field_fill),
        paint.focus_fill,
        fill(paint.focus_fg),
    ));
    file.push_str("});\n");
    file.push_str(
        "pub fn presentation() -> &'static emperor_runtime::Presentation { &PRESENTATION }\n",
    );
    file
}

/// Compositor identification recorded next to a smoke. `XDG_CURRENT_DESKTOP`
/// and `WAYLAND_DISPLAY` presence for every profile; the Nebula session
/// verdict and the installed `cosmic-comp` build for `cosmic`.
pub fn identify(desktop: &str) -> Value {
    let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let mut record = Map::new();
    record.insert(
        "xdg_current_desktop".into(),
        json!(var("XDG_CURRENT_DESKTOP")),
    );
    record.insert("xdg_session_type".into(), json!(var("XDG_SESSION_TYPE")));
    record.insert(
        "wayland_display_set".into(),
        json!(var("WAYLAND_DISPLAY").is_some()),
    );
    record.insert("x11_display_set".into(), json!(var("DISPLAY").is_some()));
    if desktop == COSMIC {
        let verdict = nebula::session::classify(&nebula::session::SessionEnv::from_process());
        record.insert(
            "nebula_session".into(),
            serde_json::to_value(&verdict).unwrap_or(Value::Null),
        );
        record.insert(
            "compositor_build".into(),
            serde_json::to_value(nebula::session::identify_compositor()).unwrap_or(Value::Null),
        );
    }
    Value::Object(record)
}

/// Parse the `wm ...` line a Wayland smoke prints after its report line.
/// `null` when the run printed none (oracle, X11 host, or an older App).
pub fn parse_wm_line(stdout: &str) -> Value {
    let Some(line) = stdout.lines().find(|line| line.starts_with("wm ")) else {
        return Value::Null;
    };
    let mut record = Map::new();
    for pair in line["wm ".len()..].split_whitespace() {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let typed = match value {
            "true" => json!(true),
            "false" => json!(false),
            _ => match value.parse::<u64>() {
                Ok(number) => json!(number),
                Err(_) => json!(value),
            },
        };
        record.insert(key.to_owned(), typed);
    }
    Value::Object(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> ThemeSource {
        ThemeSource::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../nebula/tests/fixtures")
                .join(name),
        )
    }

    #[test]
    fn desktop_values_are_generic_or_cosmic() {
        assert_eq!(parse("generic").unwrap(), GENERIC);
        assert_eq!(parse("cosmic").unwrap(), COSMIC);
        let err = parse("gnome").unwrap_err();
        assert_eq!((err.exit, err.code), (2, "EMP-USAGE"));
        assert!(check_window(COSMIC, "wayland").is_ok());
        assert!(check_window(GENERIC, "kumimanu").is_ok());
        let err = check_window(COSMIC, "kumimanu").unwrap_err();
        assert_eq!((err.exit, err.code), (2, "EMP-USAGE"));
        assert!(err.message.contains("Xwayland"));
    }

    #[test]
    fn fixture_theme_bakes_cosmic_dark_colours() {
        let bake = bake_cosmic(Some(&fixture("pop-2026-11-19")));
        assert!(bake.applied, "{}", bake.record);
        assert_eq!(bake.paint.bg, [0x1B, 0x1B, 0x1B]);
        assert_eq!(bake.paint.focus_fill, 0xFF63D0DF);
        assert_eq!(bake.paint.client_fill, Some(0xFF1B1B1B));
        assert!(bake.paint.field_fill.is_some());
        assert!(bake.paint.focus_fg.is_some());
        assert_ne!(bake.paint, Paint::GENERIC);
        let record = &bake.record;
        assert_eq!(record["applied"], true);
        assert_eq!(record["variant"], "dark");
        assert_eq!(record["format"], "v1");
        assert_eq!(record["name"], "cosmic-dark");
        assert_eq!(record["fallback_reason"], Value::Null);
        assert_eq!(record["colours"]["focus_fill"], "#63D0DF");
        assert_eq!(record["colours"]["bg"], "#1B1B1B");
        assert_eq!(record["colour_sources"]["focus_fill"], "accent.base");
        let digest = record["digest_sha256"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        // Deterministic: the same files give the same digest.
        let again = bake_cosmic(Some(&fixture("pop-2026-11-19")));
        assert_eq!(again.record["digest_sha256"], record["digest_sha256"]);
        assert_eq!(again.paint, bake.paint);
        // The generated Rust names the baked values and the digest.
        let rust = presentation_rust(&bake);
        assert!(rust.contains("emperor_runtime::Presentation::cosmic("));
        assert!(rust.contains("bg: [27, 27, 27]"));
        assert!(rust.contains("focus_fill: 0xFF63D0DF"));
        assert!(rust.contains(digest));
        assert!(rust.contains("Live theme changes are not followed."));
    }

    #[test]
    fn degraded_theme_falls_back_to_the_generic_constants() {
        // The degraded fixture has no background key.
        let bake = bake_cosmic(Some(&fixture("degraded")));
        assert!(!bake.applied);
        assert_eq!(bake.paint, Paint::GENERIC);
        let reason = bake.record["fallback_reason"].as_str().unwrap();
        assert!(reason.starts_with("theme degraded: missing"), "{reason}");
        assert!(reason.contains("background.base"), "{reason}");
        assert_eq!(bake.record["colours"]["fg"], "#14141C");
        assert_eq!(bake.record["digest_sha256"].as_str().unwrap().len(), 64);
        let rust = presentation_rust(&bake);
        assert!(rust.contains("fallback to the generic constants"));
        assert!(rust.contains("fg: [20, 20, 28]"));
    }

    #[test]
    fn unreadable_theme_falls_back_and_says_why() {
        let bake = bake_cosmic(Some(&fixture("v2-only")));
        assert!(!bake.applied);
        assert_eq!(bake.paint, Paint::GENERIC);
        let reason = bake.record["fallback_reason"].as_str().unwrap();
        assert!(reason.starts_with("theme unreadable:"), "{reason}");
        let missing = bake_cosmic(Some(&ThemeSource::new("/nonexistent/emperor-test")));
        assert!(!missing.applied);
        let none = bake_cosmic(None);
        assert!(!none.applied);
        assert_eq!(none.record["config_root"], Value::Null);
    }

    #[test]
    fn generic_presentation_record_is_fixed_with_the_constants() {
        let record = presentation_json(&Presentation::GENERIC);
        assert_eq!(record["desktop"], "generic");
        assert_eq!(record["resize"]["policy"], "fixed");
        assert_eq!(record["paint"]["fg"], "#14141C");
        assert_eq!(record["paint"]["bg"], "#EBEBF0");
        assert_eq!(record["paint"]["focus_fill"], "#D6E4F5");
        assert_eq!(record["paint"]["field_fill"], Value::Null);
        let cosmic = presentation_json(&Presentation::cosmic(Paint::GENERIC));
        assert_eq!(cosmic["resize"]["policy"], "proportional");
        assert_eq!(cosmic["resize"]["min_scale_milli"], 500);
        assert_eq!(cosmic["resize"]["max_scale_milli"], 4000);
        assert_eq!(cosmic["resize"]["preserve_aspect"], true);
        assert_eq!(cosmic["resize"]["anchor"], "Center");
    }

    #[test]
    fn wm_line_parses_into_typed_fields() {
        let stdout = "count=0 revision=0 presented=1 wayland=opened window=wayland copy=true paste=false edit=1\nwm xdg_wm_base=7 size=320x160 activated=true tiled=left+right+top+bottom bounds=1920x1080 desktop=cosmic tiled_hint=none\n";
        let wm = parse_wm_line(stdout);
        assert_eq!(wm["xdg_wm_base"], 7);
        assert_eq!(wm["size"], "320x160");
        assert_eq!(wm["activated"], true);
        assert_eq!(wm["tiled"], "left+right+top+bottom");
        assert_eq!(wm["tiled_hint"], "none");
        assert_eq!(parse_wm_line("count=1 revision=1\n"), Value::Null);
    }

    #[test]
    fn identification_records_the_session_variables() {
        let generic = identify(GENERIC);
        assert!(generic.get("wayland_display_set").is_some());
        assert!(generic.get("xdg_current_desktop").is_some());
        assert!(generic.get("nebula_session").is_none());
        let cosmic = identify(COSMIC);
        assert!(cosmic["nebula_session"].get("qualifies").is_some());
        assert_eq!(cosmic["compositor_build"]["package"], "cosmic-comp");
    }
}
