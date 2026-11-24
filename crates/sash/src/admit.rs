//! Admission for one static top-level window.
//!
//! Reachable meaning outside this profile is a fatal capability error.
//! Geometry is taken from `goya_lang::layout`, then converted only when the
//! millipoint result divides evenly into reference pixels.

use crate::Diagnostic;
use goya_lang::{handoff, layout, CellKind, ParsedScene, RectMp, SceneCell, SourceRange};

/// Profile implemented by this module. It is not the full Sash v0.0.1 slice.
pub const PROFILE: &str = "sash.win32.static-window/0.0.3";

/// Static window emitted for the win8 API floor: the same admitted meaning
/// with the system-DPI-aware Win32 adapter. The floor is a build-time choice,
/// not an admission change.
pub const PROFILE_LEGACY: &str = "sash.win32.static-window-legacy/0.0.1";

/// Millipoints in one Goya point. Matches the private layout constant.
pub(crate) const MP: f64 = 1000.0;

/// Most cells, including containers, admitted in one window.
const MAX_CELLS: usize = 32;

/// Reference pixels used by the existing Goya host (`MP_PER_PX`).
pub const MP_PER_DIP: i64 = 250;

/// Largest client edge, in device-independent pixels, admitted by this profile.
pub const MAX_DIP: i32 = 4096;

/// Largest UTF-8 scene accepted before parsing.
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;

/// Largest authored label, window title, or semantic id.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;

const MAX_TITLE_BYTES: usize = 256;
pub(crate) const MAX_ID_BYTES: usize = 256;

/// Explicit client viewport for a proportional GSSH/3 static scene.
#[derive(Clone, Copy)]
pub struct ViewportDip {
    width: i32,
    height: i32,
}

impl ViewportDip {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        ((1..=MAX_DIP).contains(&width) && (1..=MAX_DIP).contains(&height))
            .then_some(Self { width, height })
    }

    pub(crate) fn dimensions(self) -> [i32; 2] {
        [self.width, self.height]
    }
}

/// How an authored fact is carried into the Windows proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disposition {
    pub subject: String,
    pub disposition: &'static str,
    pub detail: String,
}

/// One text label placed in the root window's client area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedLabel {
    pub semantic_id: String,
    pub text: String,
    pub x_dip: i32,
    pub y_dip: i32,
    pub width_dip: i32,
    pub height_dip: i32,
    pub text_range: SourceRange,
    pub id_range: SourceRange,
    pub extent_range: SourceRange,
    /// `@iden` label and hint with its source range (GSSH/4, decision D14).
    pub iden: Option<(String, Option<String>, SourceRange)>,
}

/// A scene whose entire admitted meaning fits one static window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedWindow {
    pub scene_name: String,
    pub semantic_id: String,
    pub text: String,
    pub labels: Vec<PlacedLabel>,
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
    check_source_size(source)?;
    let parsed = parsed_for_platform(source, "windows", false)?;
    admit_scene(&parsed, None)
}

fn check_source_size(source: &str) -> Result<(), Diagnostic> {
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
    Ok(())
}

/// Static window or the bounded interactive profile.
pub enum SceneTarget {
    Static(AdmittedWindow),
    Interactive(crate::interactive::AdmittedInteractive),
}

/// Admit a static scene, or a scene the interactive profile can carry.
///
/// `admit` stays the static-window gate. Callers that link an executable use
/// this selector so a button is not forced through the static rejection.
pub fn open(source: &str) -> Result<SceneTarget, Diagnostic> {
    open_for_platform(source, "windows")
}

/// Admit a scene through Sash's shared geometry and interaction gates while
/// resolving exceptions for the requested target platform.
pub fn open_for_platform(source: &str, platform: &str) -> Result<SceneTarget, Diagnostic> {
    check_source_size(source)?;
    let parsed = parsed_for_platform(source, platform, false)?;
    let scene = &parsed.scene;
    if !scene.interactions.is_empty() || control_in(&scene.root) {
        crate::interactive::admit_parsed(&parsed).map(SceneTarget::Interactive)
    } else {
        admit_scene(&parsed, None).map(SceneTarget::Static)
    }
}

/// The contract a `--viewport-dip` scene is admitted under: GSSH/4-live for
/// the interactive counter; for a static scene GSSH/4 when it uses a GSSH/4
/// declaration (`@platform`, `@iden`, `@persistence`), else GSSH/3.
pub fn viewport_contract(source: &str) -> &'static str {
    if viewport_interactive(source) {
        return handoff::CONTRACT_V4_LIVE;
    }
    match handoff::admit_v4(source) {
        Ok(handoff) if uses_gssh4(&handoff) => handoff::CONTRACT_V4,
        _ => handoff::CONTRACT_V3,
    }
}

fn uses_gssh4(handoff: &handoff::SemanticHandoffV2) -> bool {
    !handoff.platform_exceptions().is_empty()
        || handoff
            .parsed()
            .entries
            .iter()
            .any(|entry| matches!(entry.kind.as_str(), "iden" | "persistence"))
}

/// Select GSSH/3 static admission when an explicit viewport is provided.
pub fn open_with_viewport(
    source: &str,
    viewport: Option<ViewportDip>,
) -> Result<SceneTarget, Diagnostic> {
    let Some(viewport) = viewport else {
        return open(source);
    };
    check_source_size(source)?;
    // Controls and interactions on the latest profile (decision D15): the
    // GSSH/4-live counter, with declared state, bindings, text-from and @iden.
    if viewport_interactive(source) {
        let handoff = handoff::admit_v4_live(source).map_err(diagnostic_from_strict)?;
        refuse_platform_rules(&handoff, "windows")?;
        return crate::interactive::admit_parsed_live(handoff.parsed(), viewport)
            .map(SceneTarget::Interactive);
    }
    let parsed = parsed_for_platform(source, "windows", true)?;
    if let Some((_, range)) = &parsed.declarations.state {
        return Err(cap(
            "declared state is outside the GSSH/3 static window slice",
            "Remove the state declaration.",
            Some(*range),
        ));
    }
    admit_scene(&parsed, Some(viewport)).map(SceneTarget::Static)
}

/// GSSH/4's additional syntax is subject to each downstream slice's existing
/// capability checks. A target with any matching exception refuses it at the
/// authored span until its rule semantics are implemented.
fn parsed_for_platform(
    source: &str,
    platform: &str,
    legacy_gssh3: bool,
) -> Result<ParsedScene, Diagnostic> {
    if let Ok(handoff) = handoff::admit_v4(source) {
        // Use GSSH/4 when the scene needs it: platform exceptions, or a
        // GSSH/4 declaration such as `@iden` (decision D14) that earlier
        // profiles refuse.
        if uses_gssh4(&handoff) {
            refuse_platform_rules(&handoff, platform)?;
            return Ok(handoff.parsed().clone());
        }
    }

    if legacy_gssh3 {
        handoff::admit_v3(source)
            .map(|handoff| handoff.parsed().clone())
            .map_err(diagnostic_from_strict)
    } else {
        handoff::admit(source)
            .map(|handoff| handoff.parsed.clone())
            .map_err(diagnostic_from_strict)
    }
}

/// Refuse any exception for `platform`: none is implemented by Sash yet.
fn refuse_platform_rules(
    handoff: &handoff::SemanticHandoffV2,
    platform: &str,
) -> Result<(), Diagnostic> {
    if let Some(rule) = handoff.platform_exceptions_for(platform).next() {
        return Err(cap(
            format!(
                "{platform} platform exception for '{}' ({:?}) is not implemented by this profile",
                rule.feature, rule.use_kind,
            ),
            "Remove this matching platform exception or use a compiler that resolves it.",
            Some(rule.range),
        ));
    }
    Ok(())
}

/// Whether a `--viewport-dip` scene is the interactive GSSH/4-live counter:
/// it has a control or an interaction (decided on the loose parse, so a
/// strict refusal is reported by the profile that would admit the scene).
fn viewport_interactive(source: &str) -> bool {
    goya_lang::parse_with_source_map(source).is_ok_and(|parsed| {
        !parsed.scene.interactions.is_empty() || control_in(&parsed.scene.root)
    })
}

fn control_in(cell: &SceneCell) -> bool {
    cell.kind != CellKind::Cell || cell.is_input || cell.children.iter().any(control_in)
}

pub(crate) fn diagnostic_from_strict(error: goya_lang::StrictParseError) -> Diagnostic {
    let remediation = match error.code {
        "GY-E1000" => "Correct the syntax. Sash does not guess a missing token.",
        "GY-E1001" => "Remove the unknown property. Sash does not drop unrecognised fields.",
        "GY-E1002" => "Remove the template. Templates are not preserved by the shared frontend.",
        "GY-E1003" => "Keep one definition of the property.",
        "GY-E1004" => "Correct the interaction, or remove it. This profile has no action owner.",
        "GY-E1005" => "Give every element and interaction one unique authored id.",
        _ => "Correct the scene so strict Goya admission accepts it.",
    };
    Diagnostic::new("SASH-PARSE", "parse", error.to_string(), remediation)
        .at(error.range.start, error.range.end)
        .recode(error.code)
}

fn admit_scene(
    parsed: &ParsedScene,
    viewport: Option<ViewportDip>,
) -> Result<AdmittedWindow, Diagnostic> {
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
        return Err(reject_control(root, parsed, &[]));
    }
    let (extent_range, width_pt, height_pt) = if let Some(viewport) = viewport {
        let points_per_dip = MP_PER_DIP as f64 / MP;
        let range = property_range_at(parsed, &[], "extent_p")
            .or_else(|| cell_range(parsed, &[]))
            .ok_or_else(|| layout_error("root cell has no source origin", None))?;
        (
            range,
            f64::from(viewport.width) * points_per_dip,
            f64::from(viewport.height) * points_per_dip,
        )
    } else {
        reject_position(root, parsed)?;
        require_extent(root, parsed)?
    };
    let frame = layout(scene, width_pt, height_pt);
    let mut labels = Vec::new();
    let mut dispositions = Vec::new();
    let index = {
        let mut state = AdmissionWalk {
            frame: &frame,
            parsed,
            proportional: viewport.is_some(),
            index: 0,
            seen: 0,
            labels: &mut labels,
            dispositions: &mut dispositions,
        };
        walk(&mut state, root, &[], None, true)?;
        state.index
    };
    if index != frame.nodes.len() {
        return Err(layout_error(
            "layout node count does not match the scene tree",
            Some(extent_range),
        ));
    }
    if labels.is_empty() {
        return Err(cap(
            "the scene has no text, so the window would have no accessible name",
            "Add text to a cell. An id is not displayed in place of missing text.",
            cell_range(parsed, &[]),
        ));
    }
    let root_node = &frame.nodes[0];
    if root_node.rect.x != 0 || root_node.rect.y != 0 {
        return Err(layout_error(
            "layout did not place the root cell at the viewport origin",
            Some(extent_range),
        ));
    }
    let (width_dip, height_dip) = dips(root_node.rect.width, root_node.rect.height, extent_range)?;
    if let Some(viewport) = viewport {
        if viewport.dimensions() != [width_dip, height_dip] {
            return Err(layout_error(
                "the root cell must fill the GSSH/3 viewport",
                Some(extent_range),
            ));
        }
    }
    let semantic_id = root.authored_id.clone().ok_or_else(|| {
        cap(
            "strict admission returned a cell without an id",
            "Report this as a frontend defect. Do not emit a window.",
            cell_range(parsed, &[]),
        )
    })?;
    check_label(
        "semantic id",
        &semantic_id,
        MAX_ID_BYTES,
        id_range_at(parsed, &[]),
    )?;
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
    let text = root.text.clone().unwrap_or_default();
    let text_range = property_range_at(parsed, &[], "text").unwrap_or(extent_range);
    let id_range = id_range_at(parsed, &[]).ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record an id origin",
            "Report this as a frontend defect.",
        )
    })?;
    dispositions.insert(
        0,
        Disposition {
            subject: "extent".into(),
            disposition: if viewport.is_some() {
                "equivalently-transformed"
            } else {
                "preserved"
            },
            detail: format!(
                "{width_mp} by {height_mp} millipoints lower to {width_dip} by {height_dip} device-independent pixels at {MP_PER_DIP} millipoints per reference pixel. The initial client area is that size. Resizing clips placed labels and does not reflow them.",
                width_mp = root_node.rect.width,
                height_mp = root_node.rect.height,
            ),
        },
    );
    if let Some(viewport) = viewport {
        dispositions.push(Disposition {
            subject: "viewport".into(),
            disposition: "equivalently-transformed",
            detail: format!(
                "GSSH/3 rectangles are resolved against the explicit {} by {} DIP viewport with the shared Goya layout solver.",
                viewport.width, viewport.height,
            ),
        });
    }
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
        detail: "No fill was authored. The window uses the system window colour. Each label uses the system message font for the window DPI, recreated when that DPI changes. Authored text size remains rejected.".into(),
    });
    Ok(AdmittedWindow {
        scene_name: scene.name.clone(),
        semantic_id,
        text,
        labels,
        ontology_range,
        text_range,
        extent_range,
        id_range,
        width_mp: root_node.rect.width,
        height_mp: root_node.rect.height,
        width_dip,
        height_dip,
        source_digest: parsed.source_digest,
        dispositions,
    })
}

struct AdmissionWalk<'a> {
    frame: &'a goya_lang::Frame,
    parsed: &'a ParsedScene,
    proportional: bool,
    index: usize,
    seen: usize,
    labels: &'a mut Vec<PlacedLabel>,
    dispositions: &'a mut Vec<Disposition>,
}

fn walk(
    state: &mut AdmissionWalk<'_>,
    cell: &SceneCell,
    path: &[usize],
    parent: Option<RectMp>,
    is_root: bool,
) -> Result<(), Diagnostic> {
    if state.seen >= MAX_CELLS {
        return Err(cap(
            format!("the scene has more than {MAX_CELLS} cells"),
            "Split the window or wait for a profile with a higher bound.",
            cell_range(state.parsed, path),
        ));
    }
    state.seen += 1;
    if cell.gestures.is_some() {
        return Err(cap(
            "this profile does not compile gesture policy",
            "Remove the gestures block, or author it on an interactive edit that states copy and paste.",
            cell_range(state.parsed, path),
        ));
    }
    if cell.kind != CellKind::Cell || cell.is_input {
        return Err(reject_control(cell, state.parsed, path));
    }
    if state.proportional {
        for (property, value) in [("position_p", cell.position_p), ("extent_p", cell.extent_p)] {
            if let Some(value) = value {
                let range = property_range_at(state.parsed, path, property);
                if !value.x.is_finite() || !value.y.is_finite() || !value.z.is_finite() {
                    return Err(layout_error("proportional geometry is not finite", range));
                }
                if value.z != 0.0 {
                    return Err(cap(
                        "non-zero depth is reachable and a top-level window has no depth axis",
                        "Set the proportional depth component to 0.",
                        range,
                    ));
                }
            }
        }
    }
    reject_visuals_for(cell, state.parsed, path, state.proportional)?;
    let node_index = state.index;
    state.index += 1;
    let Some(node) = state.frame.nodes.get(node_index) else {
        return Err(layout_error(
            "layout produced fewer nodes than the scene tree",
            cell_range(state.parsed, path),
        ));
    };
    if node.name != cell.name || node.text.as_deref() != cell.text.as_deref() {
        return Err(layout_error(
            "layout changed the authored cell text",
            property_range_at(state.parsed, path, "text")
                .or_else(|| cell_range(state.parsed, path)),
        ));
    }
    if !is_root && !state.proportional {
        expect_child_rect(
            cell,
            parent.expect("child has a parent"),
            node.rect,
            state.parsed,
            path,
        )?;
    }
    let extent_property = if state.proportional {
        "extent_p"
    } else {
        "extent"
    };
    let position_property = if state.proportional {
        "position_p"
    } else {
        "position"
    };
    let extent_range = property_range_at(state.parsed, path, extent_property)
        .or_else(|| cell_range(state.parsed, path))
        .unwrap_or(SourceRange { start: 0, end: 0 });
    let position_range =
        property_range_at(state.parsed, path, position_property).unwrap_or(extent_range);
    let (x_dip, y_dip) = dips_pair(node.rect.x, node.rect.y, position_range, "position")?;
    let (width_dip, height_dip) = dips(node.rect.width, node.rect.height, extent_range)?;
    match validate_text(cell, state.parsed, path)? {
        Some(text) => {
            let semantic_id = cell.authored_id.clone().ok_or_else(|| {
                cap(
                    "strict admission returned a cell without an id",
                    "Report this as a frontend defect. Do not emit a window.",
                    cell_range(state.parsed, path),
                )
            })?;
            let id_range = id_range_at(state.parsed, path).ok_or_else(|| {
                Diagnostic::new(
                    "SASH-PARSE",
                    "parse",
                    "strict admission did not record an id origin",
                    "Report this as a frontend defect.",
                )
            })?;
            check_label("semantic id", &semantic_id, MAX_ID_BYTES, Some(id_range))?;
            let text_range = property_range_at(state.parsed, path, "text").ok_or_else(|| {
                Diagnostic::new(
                    "SASH-PARSE",
                    "parse",
                    "strict admission did not record a text origin",
                    "Report this as a frontend defect.",
                )
            })?;
            state.dispositions.push(Disposition {
                subject: format!("{semantic_id} text"),
                disposition: "preserved",
                detail: format!(
                    "UTF-8 label is a STATIC control at {x_dip},{y_dip} size {width_dip} by {height_dip} dip."
                ),
            });
            let iden = admitted_iden(cell, state.parsed, path)?;
            if let Some((label, hint, _)) = &iden {
                state.dispositions.push(Disposition {
                    subject: format!("{semantic_id} @iden"),
                    disposition: "preserved",
                    detail: format!(
                        "UI Automation Name {label:?}{} set by dynamic annotation (IAccPropServices).",
                        hint.as_ref().map_or(String::new(), |hint| format!(", HelpText and description {hint:?}"))
                    ),
                });
            }
            state.labels.push(PlacedLabel {
                semantic_id,
                text,
                x_dip,
                y_dip,
                width_dip,
                height_dip,
                text_range,
                id_range,
                extent_range,
                iden,
            });
        }
        None if cell.children.is_empty() => {
            return Err(cap(
                format!("cell '{}' has no text and no children", cell.name),
                "Add text or remove the cell.",
                cell_range(state.parsed, path),
            ));
        }
        None => {
            if cell.iden.is_some() {
                // A text-less cell becomes no control, so there is no native
                // object to carry the name; refuse rather than drop it.
                return Err(cap(
                    format!(
                        "cell '{}' has @iden but no text, so it has no Win32 control to name",
                        cell.name
                    ),
                    "Give the cell text, or move @iden to a cell that has text.",
                    iden_range_at(state.parsed, path),
                ));
            }
            let id = cell.authored_id.as_deref().unwrap_or(cell.name.as_str());
            state.dispositions.push(Disposition {
                subject: format!("{id} container"),
                disposition: "equivalently-transformed",
                detail:
                    "The cell has no text, so it contributes layout only and has no STATIC control."
                        .into(),
            });
        }
    }
    for (child_index, child) in cell.children.iter().enumerate() {
        let mut child_path = path.to_vec();
        child_path.push(child_index);
        walk(state, child, &child_path, Some(node.rect), false)?;
    }
    Ok(())
}

fn reject_control(cell: &SceneCell, parsed: &ParsedScene, path: &[usize]) -> Diagnostic {
    let kind = match cell.kind {
        CellKind::Button => "button",
        CellKind::Input => "input",
        CellKind::Cell => "input",
    };
    cap(
        format!(
            "{kind} '{}' is not a static window cell in this profile",
            cell.name
        ),
        "Use @cell. Buttons and inputs need a later admitted profile.",
        cell_range(parsed, path),
    )
}

pub(crate) fn expect_child_rect(
    cell: &SceneCell,
    parent: RectMp,
    actual: RectMp,
    parsed: &ParsedScene,
    path: &[usize],
) -> Result<(), Diagnostic> {
    let range = property_range_at(parsed, path, "extent")
        .or_else(|| property_range_at(parsed, path, "position"))
        .or_else(|| cell_range(parsed, path));
    let (x, y) = if let Some(position) = cell.position {
        if !position.x.is_finite() || !position.y.is_finite() || !position.z.is_finite() {
            return Err(layout_error("position is not finite", range));
        }
        if position.z != 0.0 {
            return Err(cap(
                "non-zero depth is reachable and a top-level window has no depth axis",
                "Set the position depth component to 0.",
                property_range_at(parsed, path, "position"),
            ));
        }
        if position.x < 0.0 || position.y < 0.0 {
            return Err(layout_error(
                "a negative position would be moved by layout",
                range,
            ));
        }
        (
            parent.x.saturating_add(points_to_mp(position.x)),
            parent.y.saturating_add(points_to_mp(position.y)),
        )
    } else {
        (parent.x, parent.y)
    };
    let (width, height) = if let Some(extent) = cell.extent {
        if !extent.x.is_finite() || !extent.y.is_finite() || !extent.z.is_finite() {
            return Err(layout_error("extent is not finite", range));
        }
        if extent.z != 0.0 {
            return Err(cap(
                "non-zero depth is reachable and a top-level window has no depth axis",
                "Set the extent depth component to 0.",
                property_range_at(parsed, path, "extent"),
            ));
        }
        (points_to_mp(extent.x), points_to_mp(extent.y))
    } else {
        (parent.width, parent.height)
    };
    if actual.x != x || actual.y != y || actual.width != width || actual.height != height {
        return Err(layout_error(
            "layout changed the authored geometry; the cell does not fit its parent",
            range,
        ));
    }
    Ok(())
}

fn points_to_mp(points: f64) -> i64 {
    if !points.is_finite() {
        return 0;
    }
    (points * MP) as i64
}

pub(crate) fn reject_visuals_for(
    cell: &SceneCell,
    parsed: &ParsedScene,
    path: &[usize],
    proportional: bool,
) -> Result<(), Diagnostic> {
    let checks = [
        ("position_p", !proportional && cell.position_p.is_some()),
        ("extent_p", !proportional && cell.extent_p.is_some()),
        ("fill", cell.fill.is_some()),
        ("stroke", cell.stroke.is_some()),
        ("stroke_width", cell.stroke_width.is_some()),
        ("text_size", cell.text_size.is_some()),
        ("stroke_width_p", cell.stroke_width_p.is_some()),
        ("text_size_p", cell.text_size_p.is_some()),
        ("anchor", cell.anchor.is_some()),
        ("wrap", cell.wrap.is_some()),
    ];
    for (name, present) in checks {
        if present {
            return Err(cap(
                format!("{name} is reachable and this profile does not preserve it"),
                "Remove the property, or wait for a profile that admits it.",
                property_range_at(parsed, path, name),
            ));
        }
    }
    if cell.scroll {
        return Err(cap(
            "scroll is reachable and this profile does not preserve a scroll viewport",
            "Remove scroll, or wait for a profile that admits it.",
            property_range_at(parsed, path, "scroll"),
        ));
    }
    Ok(())
}

/// Source range of the `@iden` of the element at `path`.
pub(crate) fn iden_range_at(parsed: &ParsedScene, path: &[usize]) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "iden" && entry.scene_path.as_deref() == Some(path))
        .map(|entry| entry.range)
}

/// `@iden` for Windows (decision D14): its label and hint become the UI
/// Automation Name and HelpText through dynamic annotation. GSSH/4 has
/// already bounded them and refused control characters; Windows needs them
/// to be nonempty UTF-16 without NUL, which that implies.
pub(crate) fn admitted_iden(
    cell: &SceneCell,
    parsed: &ParsedScene,
    path: &[usize],
) -> Result<Option<(String, Option<String>, SourceRange)>, Diagnostic> {
    let Some(iden) = &cell.iden else {
        return Ok(None);
    };
    let range = iden_range_at(parsed, path).ok_or_else(|| {
        Diagnostic::new(
            "SASH-PARSE",
            "parse",
            "strict admission did not record an @iden origin",
            "Report this as a frontend defect.",
        )
    })?;
    Ok(Some((iden.label.clone(), iden.hint.clone(), range)))
}

pub(crate) fn validate_text(
    cell: &SceneCell,
    parsed: &ParsedScene,
    path: &[usize],
) -> Result<Option<String>, Diagnostic> {
    let range = property_range_at(parsed, path, "text");
    let Some(text) = cell.text.clone() else {
        return Ok(None);
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
    Ok(Some(text))
}

pub(crate) fn reject_position(root: &SceneCell, parsed: &ParsedScene) -> Result<(), Diagnostic> {
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

pub(crate) fn require_extent(
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

pub(crate) fn dips_pair(
    x_mp: i64,
    y_mp: i64,
    range: SourceRange,
    what: &str,
) -> Result<(i32, i32), Diagnostic> {
    if x_mp < 0 || y_mp < 0 || x_mp % MP_PER_DIP != 0 || y_mp % MP_PER_DIP != 0 {
        return Err(layout_error(
            format!("{what} {x_mp} by {y_mp} millipoints is not an exact multiple of {MP_PER_DIP}"),
            Some(range),
        ));
    }
    let x = x_mp / MP_PER_DIP;
    let y = y_mp / MP_PER_DIP;
    if x > i64::from(MAX_DIP) || y > i64::from(MAX_DIP) {
        return Err(cap(
            format!("{what} {x} by {y} dip exceeds the {MAX_DIP} dip edge bound"),
            "Use a smaller position or extent.",
            Some(range),
        ));
    }
    Ok((x as i32, y as i32))
}

pub(crate) fn dips(
    width_mp: i64,
    height_mp: i64,
    range: SourceRange,
) -> Result<(i32, i32), Diagnostic> {
    if width_mp <= 0 || height_mp <= 0 || width_mp % MP_PER_DIP != 0 || height_mp % MP_PER_DIP != 0
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

pub(crate) fn check_label(
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
    )
}

pub(crate) fn cap(
    message: impl Into<String>,
    remediation: &'static str,
    range: Option<SourceRange>,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new("SASH-CAP", "admit", message, remediation);
    if let Some(range) = range {
        diagnostic = diagnostic.at(range.start, range.end);
    }
    diagnostic
}

pub(crate) fn layout_error(message: impl Into<String>, range: Option<SourceRange>) -> Diagnostic {
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
    property_range_at(parsed, &[], name)
}

pub(crate) fn property_range_at(
    parsed: &ParsedScene,
    path: &[usize],
    name: &str,
) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| {
            entry.kind == "property"
                && entry.scene_path.as_deref() == Some(path)
                && (entry.name == name
                    || entry.name.replace('-', "_") == name
                    || entry.name.replace('_', "-") == name)
        })
        .map(|entry| entry.range)
}

pub(crate) fn ontology_range(parsed: &ParsedScene) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "ontology")
        .map(|entry| entry.range)
}

pub(crate) fn id_range_at(parsed: &ParsedScene, path: &[usize]) -> Option<SourceRange> {
    property_range_at(parsed, path, "id")
}

pub(crate) fn entry_range(parsed: &ParsedScene, kind: &str, name: &str) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| entry.kind == kind && entry.name == name)
        .map(|entry| entry.range)
}

pub(crate) fn cell_range(parsed: &ParsedScene, path: &[usize]) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|entry| {
            matches!(entry.kind.as_str(), "cell" | "button" | "input")
                && entry.scene_path.as_deref() == Some(path)
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
    fn iden_is_accepted_on_labels_and_refused_where_no_control_carries_it() {
        let viewport = ViewportDip::new(640, 480);
        let source = include_str!("../fixtures/iden-static.gscene");
        let SceneTarget::Static(window) = open_with_viewport(source, viewport).unwrap() else {
            panic!("expected a static window");
        };
        assert_eq!(window.labels[0].iden.as_ref().unwrap().0, "Headline");
        assert!(window
            .dispositions
            .iter()
            .any(|d| d.subject == "label.headline @iden" && d.detail.contains("UI Automation Name")));
        // The group has no text, so it becomes no control to name.
        let container = source.replace(
            "id: \"panel.group\";",
            "id: \"panel.group\"; @iden { label: \"Panel\"; }",
        );
        let error = match open_with_viewport(&container, viewport) {
            Err(error) => error,
            Ok(_) => panic!("a text-less @iden was admitted"),
        };
        assert!(error.message.contains("no Win32 control to name"), "{}", error.message);
        assert_eq!(
            &container[error.start.unwrap()..error.end.unwrap()],
            "@iden { label: \"Panel\"; }"
        );
        assert_eq!(viewport_contract(source), "GSSH/4");
    }

    #[test]
    fn platform_exceptions_are_scoped_to_the_requested_target() {
        let source = r#"@ontology demo {
            @persistence { cache_targets: null; }
            @cell root { id: "root"; }
            @platform windows { "media": only-use("native.player"); }
        }"#;
        let error = parsed_for_platform(source, "windows", false).unwrap_err();
        assert!(error.message.contains("windows platform exception"), "{}", error.message);
        assert_eq!(
            &source[error.start.unwrap()..error.end.unwrap()],
            "\"media\": only-use(\"native.player\");"
        );

        let linux_only = source.replace("windows", "linux");
        assert!(parsed_for_platform(&linux_only, "windows", false).is_ok());
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
        assert!(window
            .dispositions
            .iter()
            .any(|item| item.disposition == "preserved"));
    }

    #[test]
    fn gssh3_rejects_unrepresentable_operations_and_geometry() {
        let viewport = ViewportDip::new(640, 480);
        for (property, expected) in [
            ("text-size-p: 0.5;", "text_size_p"),
            ("stroke-width-p: 0.125;", "stroke_width_p"),
            ("extent_p: (0.5, 1, 0);", "must fill"),
            ("extent_p: (1, 1, 1);", "non-zero depth"),
        ] {
            let source = cell(&format!("id: \"root\"; text: \"Hi\"; {property}"));
            match open_with_viewport(&source, viewport) {
                Ok(_) => panic!("unexpectedly admitted {property}"),
                Err(error) => assert!(error.message.contains(expected), "{}", error.message),
            }
        }
        let source = include_str!("../fixtures/proportional-static.gscene");
        match open_with_viewport(source, ViewportDip::new(639, 480)) {
            Ok(_) => panic!("fractional DIP unexpectedly admitted"),
            Err(error) => assert!(error.message.contains("exact multiple")),
        }
    }

    #[test]
    fn fractional_quarter_point_is_exact() {
        let source =
            cell("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80.5, 40, 0);");
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
    fn nested_cell_is_placed_in_the_parent() {
        let source = include_str!("../fixtures/nested-static.gscene");
        let window = admit(source).unwrap();
        assert_eq!(window.labels.len(), 2);
        assert_eq!(window.labels[0].text, "Hello");
        assert_eq!(
            (
                window.labels[0].x_dip,
                window.labels[0].y_dip,
                window.labels[0].width_dip,
                window.labels[0].height_dip
            ),
            (0, 0, 320, 160)
        );
        assert_eq!(window.labels[1].semantic_id, "window.child");
        assert_eq!(window.labels[1].text, "There");
        assert_eq!(
            (
                window.labels[1].x_dip,
                window.labels[1].y_dip,
                window.labels[1].width_dip,
                window.labels[1].height_dip
            ),
            (40, 20, 80, 40)
        );
        let start = window.labels[1].text_range.start;
        let end = window.labels[1].text_range.end;
        assert!(source[start..end].contains("There"));
        assert!(!source[start..end].contains("Hello"));
    }

    #[test]
    fn omitted_child_geometry_fills_the_parent() {
        let source = cell(
            "    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80, 40, 0);\n    @cell child { id: \"window.child\"; text: \"In\"; }",
        );
        let window = admit(&source).unwrap();
        assert_eq!(window.labels.len(), 2);
        assert_eq!(
            (
                window.labels[1].x_dip,
                window.labels[1].y_dip,
                window.labels[1].width_dip,
                window.labels[1].height_dip
            ),
            (0, 0, 320, 160)
        );
    }

    #[test]
    fn textless_parent_places_only_the_child() {
        let source = cell(
            "    id: \"window.root\";\n    extent: (80, 40, 0);\n    @cell child { id: \"window.child\"; text: \"In\"; position: (10, 5, 0); extent: (20, 10, 0); }",
        );
        let window = admit(&source).unwrap();
        assert_eq!(window.text, "");
        assert_eq!(window.labels.len(), 1);
        assert_eq!(window.labels[0].semantic_id, "window.child");
        assert_eq!(
            (
                window.labels[0].x_dip,
                window.labels[0].y_dip,
                window.labels[0].width_dip,
                window.labels[0].height_dip
            ),
            (40, 20, 80, 40)
        );
        assert!(window.dispositions.iter().any(|item| {
            item.disposition == "equivalently-transformed" && item.subject.contains("container")
        }));
    }

    #[test]
    fn empty_cell_and_too_many_cells_are_rejected() {
        let empty = cell(
            "    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80, 40, 0);\n    @cell child { id: \"window.child\"; }",
        );
        assert!(admit(&empty).unwrap_err().message.contains("no text"));
        let mut body =
            String::from("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80, 40, 0);\n");
        for index in 0..32 {
            body.push_str(&format!(
                "    @cell c{index} {{ id: \"window.c{index}\"; text: \"T\"; position: (0, 0, 0); extent: (1, 1, 0); }}\n"
            ));
        }
        let error = admit(&cell(&body)).unwrap_err();
        assert_eq!(error.code, "SASH-CAP");
        assert!(error.message.contains("32"));
    }

    #[test]
    fn child_that_does_not_fit_is_rejected() {
        let source = cell("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80, 40, 0);\n    @cell child { id: \"window.child\"; text: \"No\"; position: (70, 0, 0); extent: (20, 10, 0); }");
        let error = admit(&source).unwrap_err();
        assert_eq!(error.code, "SASH-LAYOUT");
        assert!(
            origin(&source, &error).contains("extent:")
                || origin(&source, &error).contains("position:")
        );
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
        let newline =
            cell("    id: \"window.root\";\n    text: \"A\nB\";\n    extent: (80, 40, 0);");
        assert!(admit(&newline).unwrap_err().message.contains("line break"));
        let rtl = cell("    id: \"window.root\";\n    text: \"שלום\";\n    extent: (80, 40, 0);");
        assert!(admit(&rtl).unwrap_err().message.contains("bidirectional"));
        let missing = cell("    id: \"window.root\";\n    text: \"Hi\";");
        assert!(admit(&missing).unwrap_err().message.contains("extent"));
    }

    #[test]
    fn inexact_extent_is_a_layout_error() {
        let source =
            cell("    id: \"window.root\";\n    text: \"Hi\";\n    extent: (80.1, 40, 0);");
        let error = admit(&source).unwrap_err();
        assert_eq!(error.code, "SASH-LAYOUT");
        assert!(origin(&source, &error).contains("extent:"));
    }

    #[test]
    fn rejection_origin_is_the_authored_span() {
        let cases = [
            (
                "@ontology hello {\n  @cell root {\n    text \"missing-colon\"\n  }\n}\n",
                "GY-E1000",
                "parse",
                "text \"missing-colon\"",
            ),
            (
                "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; colur: #fff; extent: (80, 40, 0); } }\n",
                "GY-E1001",
                "parse",
                "colur: #fff;",
            ),
            (
                "@ontology hello { @cell root { @template card { text: \"x\" } } }\n",
                "GY-E1002",
                "parse",
                "@template card { text: \"x\" }",
            ),
            (
                "@ontology hello { @cell root { id: \"window.root\"; text: \"a\"; text: \"b\"; } }\n",
                "GY-E1003",
                "parse",
                "text: \"b\"",
            ),
            (
                "@ontology hello { @button go { id: \"button.go\"; text: \"Go\"; } @interaction save { id: \"action.save\"; trigger: button.missing.activate; class: D1; state { write x: int; } input { } set x = 1; } }\n",
                "GY-E1004",
                "parse",
                "@interaction save",
            ),
            (
                "@ontology hello { @cell root { text: \"Hi\" } }\n",
                "GY-E1005",
                "parse",
                "@cell root { text: \"Hi\" }",
            ),
            (
                "@ontology hello { @cell root { id: \"same\"; text: \"Hi\"; extent: (80, 40, 0); @cell child { id: \"same\"; text: \"No\"; } } }\n",
                "GY-E1005",
                "parse",
                "@cell child",
            ),
            (
                "@ontology hello { @button go { id: \"window.go\"; text: \"Go\"; extent: (80, 40, 0); } }\n",
                "SASH-CAP",
                "admit",
                "@button go",
            ),
            (
                "@ontology hello { @input name { id: \"window.name\"; text: \"Name\"; extent: (80, 40, 0); } }\n",
                "SASH-CAP",
                "admit",
                "@input name",
            ),
            (
                "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; extent: (80, 40, 0); @button go { id: \"window.go\"; text: \"Go\"; } } }\n",
                "SASH-CAP",
                "admit",
                "@button go",
            ),
            (
                "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; fill: #fff; extent: (80, 40, 0); } }\n",
                "SASH-CAP",
                "admit",
                "fill:",
            ),
            (
                "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; scroll: true; extent: (80, 40, 0); } }\n",
                "SASH-CAP",
                "admit",
                "scroll:",
            ),
            (
                "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; extent: (80, 40, 1); } }\n",
                "SASH-CAP",
                "admit",
                "extent:",
            ),
            (
                "@ontology hello { @button go { id: \"button.go\"; text: \"Go\"; extent: (80, 40, 0); } @interaction save { id: \"action.save\"; trigger: button.go.activate; class: D1; state { write count: int; } input { } set count = 1; } }\n",
                "SASH-CAP",
                "admit",
                "@interaction save",
            ),
        ];
        for (source, code, stage, slice) in cases {
            let error = admit(source).expect_err(code);
            assert_eq!(error.code, code, "{source}");
            assert_eq!(error.stage, stage, "{code}");
            let found = origin(source, &error);
            assert!(
                found.contains(slice),
                "{code} origin {found:?} does not cover {slice:?}"
            );
        }
    }

    fn origin<'a>(source: &'a str, error: &Diagnostic) -> &'a str {
        let start = error.start.expect("diagnostic origin");
        let end = error.end.expect("diagnostic origin");
        &source[start..end]
    }
}
