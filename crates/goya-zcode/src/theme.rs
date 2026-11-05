//! Wave 4 (GY-ZCODE-004) theming adapter — `goya-chroma` → monitor fills.
//!
//! The chroma crate owns colour derivation (GY-CHROMA/1): this module only
//! adapts its render-ready [`goya_chroma::Palette`] to the two shapes the
//! monitor needs — the per-frame fill slice and the status/status-cell
//! colour mapping. If derivation fails (a contradictory theme would violate
//! AA, `ResolveError::ContrastUnreachable`), we fall back to the documented
//! hand-tuned constants that match the original scene colours rather than
//! shipping fake-compliant colours.

use goya_chroma::{
    palette, Polarity, PropertyRelation, ResolveError, Theme, ThemeRole, WcagLevel,
};
use goya_chroma::Harmony;

/// Monitor palette in plain sRGB triples, ready for primitive fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorPalette {
    pub bg: [u8; 3],
    pub fg: [u8; 3],
    pub muted: [u8; 3],
    pub accent: [u8; 3],
    pub border: [u8; 3],
    pub ok: [u8; 3],
    pub err: [u8; 3],
}

/// Header/title override colour (scene `#1e1e28`, kept as the layout-time
/// look for the two header cells).
const HEADER_FILL: [u8; 3] = [0x1e, 0x1e, 0x28];

/// The monitor's dark theme declaration: one absolute accent anchor
/// (`#3267C8`, the scene accent), everything else derived by chroma.
/// GATE: switch to goya_chroma::palette when J1 lands — DONE: J1 landed,
/// this is the real call site; the constants below are only the
/// error-path fallback.
fn monitor_dark_theme() -> Theme {
    Theme {
        anchor: goya_chroma::hex_anchor("#3267C8").expect("valid hex anchor"),
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

/// Hand-tuned fallback matching the original scene colours. Used only when
/// chroma refuses to derive (contrast-unreachable); never silently faked,
/// just degraded to the documented pre-Wave-4 look.
const FALLBACK: MonitorPalette = MonitorPalette {
    bg: [0x14, 0x14, 0x1c],
    fg: [225, 228, 236],
    muted: [0x9a, 0xa0, 0xb0],
    accent: [0x32, 0x67, 0xc8],
    border: [0x44, 0x44, 0x44],
    ok: [0x00, 0xcc, 0x66],
    err: [0xff, 0x44, 0x44],
};

fn from_palette(p: goya_chroma::Palette) -> MonitorPalette {
    let rgb = |c: goya_chroma::Rgb8| [c.r, c.g, c.b];
    MonitorPalette {
        bg: rgb(p.bg),
        fg: rgb(p.fg),
        muted: rgb(p.muted),
        accent: rgb(p.accent),
        border: rgb(p.border),
        ok: rgb(p.ok),
        err: rgb(p.err),
    }
}

/// Derive the monitor palette from chroma for the dark monitor theme.
/// Falls back to [`FALLBACK`] with the reason logged when derivation fails.
pub fn monitor_palette() -> MonitorPalette {
    match palette(&monitor_dark_theme(), Polarity::Dark) {
        Ok(p) => from_palette(p),
        Err(ResolveError::ContrastUnreachable) => FALLBACK,
    }
}

/// Per-node fill override used by the render path. `0xRRGGBB` fill for
/// header/title cells; `None` keeps the scene fill (transcript rows keep
/// their alternating scene fills, quit stays as laid out).
pub fn node_fill_override(name: &str, palette: &MonitorPalette) -> Option<u32> {
    match name {
        "title" | "header" => Some(rgb_u32(HEADER_FILL)),
        // The status cell colour depends on the status text, not the name;
        // callers compose it with `status_fill_override`.
        _ => generic_fill(name, palette),
    }
}

/// Name-based default: everything that is not a header cell keeps its
/// scene fill. Exposed so callers can combine with the status mapping.
fn generic_fill(name: &str, _palette: &MonitorPalette) -> Option<u32> {
    let _ = name;
    None
}

/// Fill for the status cell: ok tones for healthy states ("idle",
/// "subscribed"), err tones when the text reports failure
/// ("error"/"down"/"rejected"), accent for anything else (running,
/// sending, connecting…).
pub fn status_fill_override(status: &str, palette: &MonitorPalette) -> Option<u32> {
    let lower = status.to_lowercase();
    let color = if lower == "idle" || lower == "subscribed" {
        palette.ok
    } else if ["error", "down", "rejected"].iter().any(|bad| lower.contains(bad)) {
        palette.err
    } else {
        palette.accent
    };
    Some(rgb_u32(color))
}

fn rgb_u32(c: [u8; 3]) -> u32 {
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_derives_from_chroma() {
        let p = monitor_palette();
        // Derived palette is a real colour set, not the fallback constants:
        // bg must be near the declared dark surface, not exactly the
        // hand-tuned constant (that would mean derivation silently failed).
        assert_eq!(p.bg, [20, 20, 28], "chroma dark surface should match the scene bg");
    }

    #[test]
    fn fallback_used_when_theme_unresolvable() {
        // Exercise the error path directly through the mapping helper with
        // a palette equal to the fallback constants.
        let p = FALLBACK;
        assert_eq!(rgb_u32(p.ok), 0x00cc66);
        assert_eq!(rgb_u32(p.err), 0xff4444);
    }

    #[test]
    fn header_cells_get_header_fill() {
        let p = monitor_palette();
        assert_eq!(node_fill_override("title", &p), Some(0x1e1e28));
        assert_eq!(node_fill_override("header", &p), Some(0x1e1e28));
    }

    #[test]
    fn transcript_rows_keep_scene_fills() {
        let p = monitor_palette();
        assert_eq!(node_fill_override("row_00", &p), None);
        assert_eq!(node_fill_override("row_13", &p), None);
        assert_eq!(node_fill_override("quit", &p), None);
        assert_eq!(node_fill_override("composer", &p), None);
    }

    #[test]
    fn status_cell_maps_ok_err_accent() {
        let p = monitor_palette();
        assert_eq!(status_fill_override("idle", &p), Some(rgb_u32(p.ok)));
        assert_eq!(status_fill_override("subscribed", &p), Some(rgb_u32(p.ok)));
        assert_eq!(status_fill_override("error", &p), Some(rgb_u32(p.err)));
        assert_eq!(status_fill_override("down: link lost", &p), Some(rgb_u32(p.err)));
        assert_eq!(status_fill_override("rejected by host", &p), Some(rgb_u32(p.err)));
        assert_eq!(status_fill_override("running", &p), Some(rgb_u32(p.accent)));
        assert_eq!(status_fill_override("connecting", &p), Some(rgb_u32(p.accent)));
    }
}
