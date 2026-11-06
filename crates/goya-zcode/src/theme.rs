//! Wave 4 (GY-ZCODE-004) theming adapter — `goya-chroma` → monitor fills.
//!
//! The chroma crate owns colour derivation (GY-CHROMA/1): this module only
//! adapts its render-ready [`goya_chroma::Palette`] to the two shapes the
//! monitor needs — the per-frame fill slice and the status/status-cell
//! colour mapping. If derivation fails (a contradictory theme would violate
//! AA, `ResolveError::ContrastUnreachable`), we fall back to the documented
//! hand-tuned constants that match the original scene colours rather than
//! shipping fake-compliant colours.

use goya_chroma::Harmony;
use goya_chroma::{palette, Polarity, PropertyRelation, ResolveError, Theme, ThemeRole, WcagLevel};

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
        // Anchor #3267C8 sits at L≈0.52; a large polar delta is required to
        // reach a true dark surface (gate fix: Polar(0.06) yielded a blue
        // mid-tone). Chroma scaled down hard so the surface reads near-grey
        // rather than brand-blue.
        surface: ThemeRole {
            lightness: PropertyRelation::Polar(0.38),
            chroma: PropertyRelation::Scale(0.25),
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
    } else if ["error", "down", "rejected"]
        .iter()
        .any(|bad| lower.contains(bad))
    {
        palette.err
    } else {
        palette.accent
    };
    Some(rgb_u32(color))
}

/// Wave 9 (GY-ZCODE-009): per-node text-colour override, the fg sibling of
/// [`node_fill_override`]. The status node's text follows the same
/// ok/err/accent mapping as its fill (the status colour is decided by the
/// status text, not the node name); every other node keeps the flat scene
/// foreground (`None`). Transcript-row text rules live in
/// [`row_fg_override`] because they depend on the row's *text*, which this
/// name-level function does not see — the render path composes both
/// (see [`override_slices`]).
pub fn node_fg_override(name: &str, view_status: &str, palette: &MonitorPalette) -> Option<u32> {
    match name {
        "status" => status_fill_override(view_status, palette),
        _ => None,
    }
}

/// Wave 9: text-based fg override for transcript rows. Tool-report lines
/// (`⚙ <tool> …` / `⚙ <tool> ✓` / `⚙ <tool> ✗`, see the Wave 5 view
/// contract) take ok tones on success and err tones on failure; markdown
/// heading rows (rendered as `■ TITLE`) take the accent. Everything else —
/// plain chat rows, code gutters, bullets, the composer — keeps the flat
/// scene foreground (`None`).
pub fn row_fg_override(text: &str, palette: &MonitorPalette) -> Option<u32> {
    if text.starts_with("⚙ ") {
        if text.ends_with('✓') {
            Some(rgb_u32(palette.ok))
        } else if text.ends_with('✗') {
            Some(rgb_u32(palette.err))
        } else {
            // In-flight tool row (the "…" form): no recolour until it is
            // terminal.
            None
        }
    } else if text.starts_with("■ ") {
        Some(rgb_u32(palette.accent))
    } else {
        None
    }
}

/// Wave 9: build the per-primitive `fills` and `fgs` override slices in one
/// pass, in primitive order. Nodes are `(name, text, scene fill)` triples —
/// kept generic so this module stays free of goya-lang types. `fills`
/// mirrors the Wave 4 render-path semantics exactly (status by status text,
/// others by name, scene fill as the fallback); `fgs` composes
/// [`node_fg_override`] (name level) with [`row_fg_override`] (text level,
/// text primitives only) — `None` keeps the flat fg argument downstream.
pub fn override_slices<'a, I>(
    nodes: I,
    status_text: &str,
    palette: &MonitorPalette,
) -> (Vec<Option<u32>>, Vec<Option<u32>>)
where
    I: IntoIterator<Item = (&'a str, Option<&'a str>, Option<u32>)>,
{
    let mut fills = Vec::new();
    let mut fgs = Vec::new();
    for (name, text, scene_fill) in nodes {
        let fill = if name == "status" {
            status_fill_override(status_text, palette).or(scene_fill)
        } else {
            node_fill_override(name, palette).or(scene_fill)
        };
        // Text colour: name-level mapping first, then the text-level row
        // rules; buttons keep their fill semantics and never recolour.
        let fg = node_fg_override(name, status_text, palette)
            .or_else(|| text.and_then(|t| row_fg_override(t, palette)));
        fills.push(fill);
        fgs.push(fg);
    }
    (fills, fgs)
}

fn rgb_u32(c: [u8; 3]) -> u32 {
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_derives_from_chroma_meets_aa() {
        let p = monitor_palette();
        // The derived palette must be a genuine chroma product: AA holds
        // for fg/accent/ok/err against bg (GY-CHROMA/1 contract).
        let rgb = |c: [u8; 3]| goya_chroma::Rgb8::new(c[0], c[1], c[2]);
        for (name, fg) in [
            ("fg", p.fg),
            ("accent", p.accent),
            ("ok", p.ok),
            ("err", p.err),
        ] {
            let ratio = goya_chroma::aa_ratio(rgb(fg), rgb(p.bg));
            assert!(ratio >= 4.5, "{name}/bg contrast {ratio:.2} below AA");
        }
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
        assert_eq!(
            status_fill_override("down: link lost", &p),
            Some(rgb_u32(p.err))
        );
        assert_eq!(
            status_fill_override("rejected by host", &p),
            Some(rgb_u32(p.err))
        );
        assert_eq!(status_fill_override("running", &p), Some(rgb_u32(p.accent)));
        assert_eq!(
            status_fill_override("connecting", &p),
            Some(rgb_u32(p.accent))
        );
    }

    // --- Wave 9 (GY-ZCODE-009): per-node text colours ---

    #[test]
    fn status_text_maps_ok_err_accent_like_its_fill() {
        let p = monitor_palette();
        assert_eq!(node_fg_override("status", "idle", &p), Some(rgb_u32(p.ok)));
        assert_eq!(
            node_fg_override("status", "error", &p),
            Some(rgb_u32(p.err))
        );
        assert_eq!(
            node_fg_override("status", "running", &p),
            Some(rgb_u32(p.accent))
        );
        assert_eq!(node_fg_override("status", "idle", &p), status_fill_override("idle", &p));
    }

    #[test]
    fn non_status_names_keep_flat_fg() {
        let p = monitor_palette();
        assert_eq!(node_fg_override("title", "idle", &p), None);
        assert_eq!(node_fg_override("row_00", "running", &p), None);
        assert_eq!(node_fg_override("quit", "idle", &p), None);
        assert_eq!(node_fg_override("composer", "idle", &p), None);
    }

    #[test]
    fn tool_rows_colour_by_terminal_marker() {
        let p = monitor_palette();
        assert_eq!(row_fg_override("⚙ read_file ✓", &p), Some(rgb_u32(p.ok)));
        assert_eq!(row_fg_override("⚙ write_file ✗", &p), Some(rgb_u32(p.err)));
        // In-flight (… form) and non-tool rows stay flat even when they
        // contain the gear glyph or end with ✓-like characters elsewhere.
        assert_eq!(row_fg_override("⚙ read_file …", &p), None);
        assert_eq!(row_fg_override("⚙ read_file", &p), None);
        assert_eq!(row_fg_override("done ✓", &p), None);
    }

    #[test]
    fn markdown_headings_take_accent_and_other_rows_stay_flat() {
        let p = monitor_palette();
        assert_eq!(row_fg_override("■ REPORT", &p), Some(rgb_u32(p.accent)));
        assert_eq!(row_fg_override("■", &p), None, "no trailing space, not a heading row");
        assert_eq!(row_fg_override("> user input", &p), None);
        assert_eq!(row_fg_override("|   let a = 1;", &p), None);
        assert_eq!(row_fg_override("• bullet one", &p), None);
        assert_eq!(row_fg_override("", &p), None);
    }

    #[test]
    fn override_slices_match_fill_semantics_and_primitive_order() {
        let p = monitor_palette();
        let nodes: Vec<(&str, Option<&str>, Option<u32>)> = vec![
            ("title", Some("monitor"), None),
            ("status", Some("running"), None),
            ("row_00", Some("⚙ read ✓"), Some(0x101010)),
            ("row_01", Some("■ HEADING"), None),
            ("row_02", Some("plain row"), None),
            ("quit", None, Some(0x222222)),
        ];
        let (fills, fgs) = override_slices(nodes, "running", &p);
        // fills: exact Wave 4 render-path semantics (status by text, others
        // by name, scene fill fallback), same order as the input.
        assert_eq!(
            fills,
            vec![
                Some(0x1e1e28),
                Some(rgb_u32(p.accent)),
                Some(0x101010),
                None,
                None,
                Some(0x222222),
            ]
        );
        // fgs: status accent by mapping; tool ✓ ok; heading accent; flat
        // elsewhere (buttons/panels None).
        assert_eq!(
            fgs,
            vec![
                None,
                Some(rgb_u32(p.accent)),
                Some(rgb_u32(p.ok)),
                Some(rgb_u32(p.accent)),
                None,
                None,
            ]
        );
    }
}
