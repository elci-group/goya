//! Admission for one static top-level window.
//!
//! Reachable meaning outside this profile is a fatal capability error.
//! Geometry is taken from `goya_lang::layout`, then converted only when the
//! millipoint result divides evenly into reference pixels.

use crate::Diagnostic;
use goya_lang::{handoff, layout, CellKind, ParsedScene, SceneCell, SourceRange};

/// Profile implemented by this module. It is not the full Sash v0.0.1 slice.
pub const PROFILE: &str = "sash.win32.static-window/0.0.1";

/// Reference pixels used by the existing Goya host (`MP_PER_PX`).
pub const MP_PER_DIP: i64 = 250;

/// Largest client edge, in device-independent pixels, admitted by this profile.
pub const MAX_DIP: i32 = 4096;

/// Largest UTF-8 scene accepted before parsing.
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// Largest authored label, window title, or semantic id.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

const MAX_TITLE_BYTES: usize = 256;
const MAX_ID_BYTES: usize = 256;

/// How an authored fact is carried into the Windows proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disposition {
    pub subject: String,
    pub disposition: &'static str,
    pub detail: String,
}

/// A scene whose entire admitted meaning fits one static window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedWindow {
    pub scene_name: String,
    pub semantic_id: String,
    pub text: String,
    pub ontology_range: SourceRange,
    pub text_range: SourceRange,
    pub extent_range: SourceRange,
    pub id_range: SourceRange,
    pub width_mp: i64,
    pub height_mp: i64,
    pub width_dip: i32,
    pub height_dip: i32,
    pub source_digest: [u8; 32],
    pub dispositions: Vec<Disposition>,
}

/// Parse and admit `source`, or return the first fatal diagnostic.
pub fn admit(source: &str) -> Result<AdmittedWindow, Diagnostic> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(Diagnostic::new(
            "SASH-PARSE",
            "ingest",
            format!(
                "scene is {} bytes; this profile accepts at most {MAX_SOURCE_BYTES}",
                source.len()
            ),
            "Split the scene or raise the bound in a new admitted profile.",
        ));
    }
    let handoff = handoff::admit(source).map_err(diagnostic_from_strict)?;
    admit_scene(&handoff.parsed)
}

fn diagnostic_from_strict(error: goya_lang::StrictParseError) -> Diagnostic {
    let remediation = match error.code {
        "GY-E1001" => "Remove the unknown property. Sash does not drop unrecognised fields.",
        "GY-E1002" => "Remove the template. Templates are not preserved by the shared frontend.",
        "GY-E1005" => "Give every element and interaction one unique authored id.",
        _ => "Correct the scene so strict Goya admission accepts it.",
    };
    Diagnostic::new("SASH-PARSE", "parse", error.to_string(), remediation)
        .at(error.range.start, error.range.end)
        .recode(error.code)
}

fn admit_scene(parsed: &ParsedScene) -> Result<AdmittedWindow, Diagnostic> {
    let scene = &parsed.scene;
    if let Some(interaction) = scene.interactions.first() {
        let range = entry_range(parsed, "interaction", &interaction.name);
        return Err(cap(
            format!(
                "interaction '{}' is reachable and this profile has no action or state owner",
                interaction.name
            ),
            "Remove interactions, or wait for a profile that admits typed actions.",
            range,
        ));
    }
    let root = &scene.root;
    if root.kind != CellKind::Cell || root.is_input {
        let kind = match root.kind {
            CellKind::Button => "button",
            CellKind::Input => "input",
            CellKind::Cell => "input",
        };
        return Err(cap(
            format!("{kind} '{}' is not a static window cell in this profile", root.name),
            "Use one @cell. Buttons and inputs need a later admitted profile.",
            cell_range(parsed, &[]),
        ));
    }
    if !root.children.is_empty() {
        return Err(cap(
            "nested cells are reachable and this profile has no child layout",
            "Use a single cell, or wait for a profile that lowers nested layout.",
            child_range(parsed),
        ));
    }
    reject_visuals(root, parsed)?;
    let text = require_text(root, parsed)?;
    reject_position(root, parsed)?;
    let (extent_range, width_pt, height_pt) = require_extent(root, parsed)?;
    let frame = layout(scene, width_pt, height_pt);
    let Some(node) = frame.nodes.first() else {
        return Err(layout_error(
            "layout produced no node",
            extent_range,
        ));
    };
    if frame.nodes.len() != 1 || node.rect.x != 0 || node.rect.y != 0 {
        return Err(layout_error(
            "layout did not place the cell at the viewport origin",
            extent_range,
        ));
    }
    if node.text.as_deref() != Some(text.as_str()) {
        return Err(layout_error(
            "layout changed the authored text",
            property_range(parsed, "text").unwrap_or(extent_range),
        ));
    }
    let (width_dip, height_dip) = dips(node.rect.width, node.rect.height, extent_range)?;
    let semantic_id = root.authored_id.clone().ok_or_else(|| {
        cap(
            "strict admission returned a cell without an id",
            "Report this as a frontend defect. Do not emit a window.",
            cell_range(parsed, &[]),
        )
    })?;
    check_label("semantic id", &semantic_id, MAX_ID_BYTES, id_range(parsed))?;
    check_label(
        "window title",
        &scene.name,
        MAX_TITLE_BYTES,
        ontology_range(parsed),
    )?;
    let ontology_range = ontology_range(parsed).ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record an ontology origin",
            "Report this as a frontend defect.",
        )
    })?;
    let text_range = property_range(parsed, "text").ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record a text origin",
            "Report this as a frontend defect.",
        )
    })?;
    let id_range = id_range(parsed).ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record an id origin",
            "Report this as a frontend defect.",
        )
    })?;
    let mut dispositions = Vec::new();
    dispositions.push(Disposition {
        subject: "text".into(),
        disposition: "preserved",
        detail: "UTF-8 label is stored as UTF-16 on one STATIC control, without word wrap or a newline translation.".into(),
    });
    dispositions.push(Disposition {
        subject: "id".into(),
        disposition: "preserved",
        detail: "Authored id is kept in the source map and as the window property Goya.SemanticId (UTF-16LE, NUL-terminated). UI Automation AutomationId is not mapped.".into(),
    });
    dispositions.push(Disposition {
        subject: "extent".into(),
        disposition: "preserved",
        detail: format!(
            "{width_mp} by {height_mp} millipoints lower to {width_dip} by {height_dip} device-independent pixels at {MP_PER_DIP} millipoints per reference pixel. The initial client area is that size. Resizing does not change the authored cell size; a smaller client clips it.",
            width_mp = node.rect.width,
            height_mp = node.rect.height,
        ),
    });
    dispositions.push(Disposition {
        subject: "position".into(),
        disposition: "preserved",
        detail: "The cell stays at the parent origin. An omitted position uses that language default.".into(),
    });
    dispositions.push(Disposition {
        subject: "depth".into(),
        disposition: "preserved",
        detail: "Authored depth is zero. A top-level HWND has no extra depth axis.".into(),
    });
    dispositions.push(Disposition {
        subject: "anchor".into(),
        disposition: "equivalently-transformed",
        detail: "No anchor was authored. The language default Start is a left-aligned STATIC control for text that has no strong right-to-left characters.".into(),
    });
    dispositions.push(Disposition {
        subject: "scroll".into(),
        disposition: "preserved",
        detail: "Scroll was not authored, so the cell is not a scroll viewport.".into(),
    });
    dispositions.push(Disposition {
        subject: "background".into(),
        disposition: "equivalently-transformed",
        detail: "No fill was authored. The window uses the system window colour and the stock default GUI font.".into(),
    });
    Ok(AdmittedWindow {
        scene_name: scene.name.clone(),
        semantic_id,
        text,
        ontology_range,
        text_range,
        extent_range,
        id_range,
        width_mp: node.rect.width,
        height_mp: node.rect.height,
        width_dip,
        height_dip,
        source_digest: parsed.source_digest,
        dispositions,
    })
}

fn reject_visuals(root: &SceneCell, parsed: &ParsedScene) -> Result<(), Diagnostic> {
    let checks: [(&str, bool); 8] = [
        ("position_p", root.position_p.is_some()),
        ("extent_p", root.extent_p.is_some()),
        ("fill", root.fill.is_some()),
        ("stroke", root.stroke.is_some()),
        ("stroke_width", root.stroke_width.is_some()),
        ("text_size", root.text_size.is_some()),
        ("anchor", root.anchor.is_some()),
        ("wrap", root.wrap.is_some()),
    ];
    for (name, present) in checks {
        if present {
            return Err(cap(
                format!("{name} is reachable and this profile does not preserve it"),
                "Remove the property, or wait for a profile that admits it.",
                property_range_aliases(parsed, name),
            ));
        }
    }
    if root.scroll {
        return Err(cap(
            "scroll is reachable and this profile does not preserve a scroll viewport",
            "Remove scroll, or wait for a profile that admits it.",
            property_range(parsed, "scroll"),
        ));
    }
    Ok(())
}

fn require_text(root: &SceneCell, parsed: &ParsedScene) -> Result<String, Diagnostic> {
    let range = property_range(parsed, "text");
    let Some(text) = root.text.clone() else {
        return Err(cap(
            "authored text is required so the native control has an accessible name",
            "Add a text property. An id is not displayed in place of missing text.",
            cell_range(parsed, &[]),
        ));
    };
    if text.is_empty() {
        return Err(cap(
            "authored text is empty, so the native control would have no accessible name",
            "Set text to the label that assistive technology should read.",
            range,
        ));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(cap(
            format!(
                "text is {} bytes; this profile accepts at most {MAX_TEXT_BYTES}",
                text.len()
            ),
            "Shorten the label or admit a larger text profile.",
            range,
        ));
    }
    if let Some(reason) = rejected_text(&text) {
        return Err(cap(
            format!("text contains {reason}, which this profile would not preserve"),
            "Use a single left-to-right line without control characters.",
            range,
        ));
    }
    if text.encode_utf16().count() > 32_767 {
        return Err(cap(
            "text exceeds 32767 UTF-16 code units",
            "Shorten the label. Win32 window text cannot carry a longer value.",
            range,
        ));
    }
    Ok(text)
}

fn reject_position(root: &SceneCell, parsed: &ParsedScene) -> Result<(), Diagnostic> {
    let Some(position) = root.position else {
        return Ok(());
    };
    let range = property_range(parsed, "position");
    if !position.x.is_finite() || !position.y.is_finite() || !position.z.is_finite() {
        return Err(layout_error("position is not finite", range));
    }
    if position.x != 0.0 || position.y != 0.0 || position.z != 0.0 {
        return Err(cap(
            "a non-zero position needs a viewport larger than the cell, and this profile has no separate viewport",
            "Omit position or set it to (0, 0, 0).",
            range,
        ));
    }
    Ok(())
}

fn require_extent(
    root: &SceneCell,
    parsed: &ParsedScene,
) -> Result<(SourceRange, f64, f64), Diagnostic> {
    let range = property_range(parsed, "extent");
    let Some(extent) = root.extent else {
        return Err(cap(
            "the cell has no extent, and this profile will not invent a viewport",
            "Set extent to a positive width and height in points, with zero depth.",
            cell_range(parsed, &[]),
        ));
    };
    let range = range.ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "extent was present on the cell but had no source origin",
            "Report this as a frontend defect.",
        )
    })?;
    if !extent.x.is_finite() || !extent.y.is_finite() || !extent.z.is_finite() {
        return Err(layout_error("extent is not finite", Some(range)));
    }
    if extent.z != 0.0 {
        return Err(cap(
            "non-zero depth is reachable and a top-level window has no depth axis",
            "Set the extent depth component to 0.",
            Some(range),
        ));
    }
    if extent.x <= 0.0 || extent.y <= 0.0 {
        return Err(layout_error(
            "extent width and height must be positive",
            Some(range),
        ));
    }
    Ok((range, extent.x, extent.y))
}

fn dips(width_mp: i64, height_mp: i64, range: SourceRange) -> Result<(i32, i32), Diagnostic> {
    if width_mp <= 0
        || height_mp <= 0
        || width_mp % MP_PER_DIP != 0
        || height_mp % MP_PER_DIP != 0
    {
        return Err(layout_error(
            format!(
                "layout size {width_mp} by {height_mp} millipoints is not an exact multiple of {MP_PER_DIP}"
            ),
            Some(range),
        ));
    }
    let width = width_mp / MP_PER_DIP;
    let height = height_mp / MP_PER_DIP;
    if width > i64::from(MAX_DIP) || height > i64::from(MAX_DIP) {
        return Err(cap(
            format!("client size {width} by {height} dip exceeds the {MAX_DIP} dip edge bound"),
            "Use a smaller extent. The bound keeps the Win32 size inside a 16-bit client message at high DPI.",
            Some(range),
        ));
    }
    Ok((width as i32, height as i32))
}

fn check_label(
    what: &str,
    value: &str,
    max: usize,
    range: Option<SourceRange>,
) -> Result<(), Diagnostic> {
    if value.is_empty() || value.len() > max {
        return Err(cap(
            format!("{what} must be 1 to {max} bytes"),
            "Use a shorter plain identifier.",
            range,
        ));
    }
    if let Some(reason) = rejected_text(value) {
        return Err(cap(
            format!("{what} contains {reason}"),
            "Use a single left-to-right line without control characters.",
            range,
        ));
    }
    Ok(())
}

fn rejected_text(text: &str) -> Option<&'static str> {
    if text.chars().any(|c| c == '\0') {
        return Some("NUL");
    }
    if text.chars().any(|c| c == '\n' || c == '\r') {
        return Some("a line break");
    }
    if text.chars().any(|c| (c as u32) < 0x20 || c == '\u{7f}') {
        return Some("a control character");
    }
    if text.chars().any(is_strong_rtl) {
        return Some("bidirectional text");
    }
    None
}

fn is_strong_rtl(c: char) -> bool {
    matches!(
        c as u32,
        0x0590..=0x08FF
            | 0xFB1D..=0xFDFF
            | 0xFE70..=0xFEFF
            | 0x10800..=0x10FFF
            | 0x1E800..=0x1EFFF
            | 0x200F
            | 0x202B
            | 0x202E
            | 0x2067
            | 0x061C
    )
}

fn cap(message: String, remediation: &'static str, range: Option<SourceRange>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new("SASH-CAP", "admit", message, remediation);
    if let Some(range) = range {
        diagnostic = diagnostic.at(range.start, range.end);
    }
    diagnostic
}

fn layout_error(message: impl Into<String>, range: Option<SourceRange>) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        "SASH-LAYOUT",
        "admit",
        message,
        "Change the geometry so layout preserves it under the 250 millipoint reference-pixel mapping.",
    );
    if let Some(range) = range {
        diagnostic = diagnostic.at(range.start, range.end);
    }
    diagnostic
}

fn property_range(parsed: &ParsedScene, name: &str) -> Option<SourceRange> {
    property_range_aliases(parsed, name)
}

fn property_range_aliases(parsed: &ParsedScene, name: &str) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| {
            entry.kind == "property"
                && (entry.name == name
                    || entry.name.replace('-', "_") == name
                    || entry.name.replace('_', "-") == name)
        })
        .map(|entry| entry.range)
}

fn ontology_range(parsed: &ParsedScene) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "ontology")
        .map(|entry| entry.range)
}

fn id_range(parsed: &ParsedScene) -> Option<SourceRange> {
    property_range(parsed, "id")
}

fn entry_range(parsed: &ParsedScene, kind: &str, name: &str) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| entry.kind == kind && entry.name == name)
        .map(|entry| entry.range)
}

fn cell_range(parsed: &ParsedScene, path: &[usize]) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| {
            matches!(entry.kind.as_str(), "cell" | "button" | "input")
                && entry.scene_path.as_deref() == Some(path)
        })
        .map(|entry| entry.range)
}

fn child_range(parsed: &ParsedScene) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| {
            matches!(entry.kind.as_str(), "cell" | "button" | "input")
                && entry.scene_path.as_ref().is_some_and(|path| !path.is_empty())
        })
        .map(|entry| entry.range)
}

impl Diagnostic {
    fn recode(mut self, code: &'static str) -> Self {
        self.code = code;
        if code.starts_with("GY-E") {
            self.code = code;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(body: &str) -> String {
        format!("@ontology hello {{\n{body}\n}}\n")
    }

    fn cell(inner: &str) -> String {
        scene(&format!("  @cell root {{\n{inner}\n  }}"))
    }

    #[test]
    fn fixture_admits_exact_dips() {
        let source = include_str!("../fixtures/static-window.gscene");
        let window = admit(source).unwrap();
        assert_eq!(window.semantic_id, "window.root");
        assert_eq!(window.text, "Hello");
        assert_eq!(window.scene_name, "hello");
        assert_eq!((window.width_dip, window.height_dip), (320, 160));
        assert_eq!(window.width_mp, 80_000);
        assert_eq!(window.height_mp, 40_000);
        assert!(window.dispositions.iter().any(|item| item.disposition == "preserved"));
    }

    #[test]
    fn fractional_quarter_point_is_exact() {
        let source = cell("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80.5, 40, 0);");
        let window = admit(&source).unwrap();
        assert_eq!(window.width_dip, 322);
    }

    #[test]
    fn unknown_property_keeps_frontend_origin() {
        let source = cell("    id: \"window.root\";\n    text: \"Hi\";\n    colur: #fff;\n    extent: (80, 40, 0);");
        let error = admit(&source).unwrap_err();
        assert_eq!(error.code, "GY-E1001");
        let start = error.start.unwrap();
        let end = error.end.unwrap();
        assert_eq!(&source[start..end], "colur: #fff;");
    }

    #[test]
    fn nested_cell_is_rejected() {
        let source = cell("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80, 40, 0);\n    @cell child { id: \"child\"; text: \"No\"; }");
        let error = admit(&source).unwrap_err();
        assert_eq!(error.code, "SASH-CAP");
        assert!(error.message.contains("nested"));
    }

    #[test]
    fn button_and_interaction_are_rejected() {
        let source = "@ontology app {\n  @button go { id: \"element.go\"; text: \"Go\"; extent: (80, 40, 0); }\n  @interaction save { id: \"action.save.v1\"; trigger: button.go.activate; class: D1; state { write count: int; } input { } set count = 1; }\n}\n";
        let error = admit(source).unwrap_err();
        assert_eq!(error.code, "SASH-CAP");
        assert!(error.message.contains("interaction"));
    }

    #[test]
    fn fill_newline_rtl_and_missing_extent_fail() {
        let fill = cell("    id: \"window.root\";\n    text: \"Hi\";\n    fill: #fff;\n    extent: (80, 40, 0);");
        assert_eq!(admit(&fill).unwrap_err().code, "SASH-CAP");
        let newline = cell("    id: \"window.root\";\n    text: \"A\nB\";\n    extent: (80, 40, 0);");
        assert!(admit(&newline).unwrap_err().message.contains("line break"));
        let rtl = cell("    id: \"window.root\";\n    text: \"שלום\";\n    extent: (80, 40, 0);");
        assert!(admit(&rtl).unwrap_err().message.contains("bidirectional"));
        let missing = cell("    id: \"window.root\";\n    text: \"Hi\";");
        assert!(admit(&missing).unwrap_err().message.contains("extent"));
    }

    #[test]
    fn inexact_extent_is_a_layout_error() {
        let source = cell("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80.1, 40, 0);");
        let error = admit(&source).unwrap_err();
        assert_eq!(error.code, "SASH-LAYOUT");
    }
}
