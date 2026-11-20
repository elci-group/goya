//! Experimental interactive profile `maja.web.local.v2` over GSSH/3.
//!
//! Every construct GSSH/3 admits is either carried into [`InteractiveWebIr`]
//! or refused with `MAJA-E0200` at that construct's exact source range.
//! The allow-list enumerates source entries, so an authored construct can
//! never disappear without an admission decision. Coverage mirrors Aragon's
//! (`docs/aragon/android-scene.md`) except that `scroll` is refused until
//! Maja mirrors `Frame::scroll_limits`.

use crate::Diagnostic;
use goya_lang::handoff::{admit_v3, canonical_property, Origin, SemanticHandoffV2};
use goya_lang::{
    CellKind, InteractionClass, InteractionValue, SceneCell, SourceRange, StateLifetime,
};

/// Experimental interactive browser profile; not a production qualification.
pub const INTERACTIVE_PROFILE: &str = "maja.web.local.v2";
/// Largest scene the profile admits.
pub const MAX_CELLS: usize = 1024;
/// Largest UTF-8 text of one element after the reference wrap transform.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
/// D1 limits checked at admission so the runtime never refuses its initial state.
const D1_MAX_TEXT_BYTES: usize = 64 * 1024;
const D1_MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebCellKind {
    Cell,
    Button,
    Input,
}

/// One authored element in depth-first preorder, the reference draw order.
#[derive(Debug, Clone, PartialEq)]
pub struct WebCell {
    pub id: String,
    /// Structural name; interaction triggers refer to buttons by it.
    pub name: String,
    pub kind: WebCellKind,
    /// Index of the parent in [`InteractiveWebIr::cells`].
    pub parent: Option<usize>,
    pub position_p: Option<[f64; 2]>,
    pub extent_p: Option<[f64; 2]>,
    /// 0xRRGGBB.
    pub fill: Option<u32>,
    /// Colour and `stroke-width-p`; admission requires both or neither.
    pub stroke: Option<(u32, f64)>,
    pub text_size_p: Option<f64>,
    /// Text after the reference wrap transform.
    pub text: Option<String>,
    /// A scroll viewport: its direct children may extend past it.
    pub scroll: bool,
    pub range: SourceRange,
}

/// A strictly admitted scene for the interactive profile.
#[derive(Debug, Clone)]
pub struct InteractiveWebIr {
    handoff: SemanticHandoffV2,
    source: String,
    cells: Vec<WebCell>,
}

impl InteractiveWebIr {
    pub fn handoff(&self) -> &SemanticHandoffV2 {
        &self.handoff
    }
    /// Exact source bytes of this revision.
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn cells(&self) -> &[WebCell] {
        &self.cells
    }
    pub fn source_digest(&self) -> [u8; 32] {
        self.handoff.parsed().source_digest
    }
    /// Authored element ID of the structural name, when unique.
    pub fn cell_by_name(&self, name: &str) -> Option<&WebCell> {
        let mut found = self.cells.iter().filter(|cell| cell.name == name);
        match (found.next(), found.next()) {
            (Some(cell), None) => Some(cell),
            _ => None,
        }
    }
}

fn refuse(message: impl Into<String>, range: Option<SourceRange>) -> Diagnostic {
    Diagnostic {
        code: "MAJA-E0200",
        message: message.into(),
        range,
    }
}

/// Admit `source` under the interactive profile.
pub fn admit_local_interactive(
    source: &str,
    profile: &str,
) -> Result<InteractiveWebIr, Diagnostic> {
    if profile != INTERACTIVE_PROFILE {
        return Err(Diagnostic {
            code: "MAJA-E0002",
            message: format!("unknown local profile: {profile}"),
            range: None,
        });
    }
    let handoff = admit_v3(source).map_err(Diagnostic::from)?;
    let parsed = handoff.parsed();
    if parsed.entries.iter().any(|e| {
        e.source_digest != parsed.source_digest
            || e.range.start > e.range.end
            || e.range.end > source.len()
            || !source.is_char_boundary(e.range.start)
            || !source.is_char_boundary(e.range.end)
    }) {
        return Err(Diagnostic {
            code: "MAJA-E0004",
            message: "canonical source map does not match source bytes".into(),
            range: None,
        });
    }
    let whole = SourceRange {
        start: 0,
        end: source.len(),
    };
    let at = |origin: &Origin| handoff.origin(origin).or(Some(whole));

    // 1. Closed allow-list over every source entry.
    for entry in &parsed.entries {
        let allowed = match entry.kind.as_str() {
            "ontology" | "cell" | "button" | "input" | "interaction" | "state" | "state-key"
            | "binding" => true,
            "property" => {
                let element = entry
                    .scene_path
                    .as_deref()
                    .and_then(|path| cell_at(&parsed.scene.root, path));
                let input = element.is_some_and(|cell| cell.kind == CellKind::Input);
                match canonical_property(&entry.name).as_str() {
                    "id" | "position_p" | "extent_p" | "fill" | "stroke" | "stroke_width_p"
                    | "text_size_p" | "wrap" => true,
                    "text" if input => {
                        return Err(refuse(
                            "`text:` on @input is refused until its meaning (placeholder or initial content) is decided (docs/gssh/decisions.md D3)",
                            Some(entry.range),
                        ))
                    }
                    "text" => true,
                    // Checked per element below.
                    "scroll" => true,
                    "text_anchor" => {
                        return Err(refuse(
                            "`text-anchor` is refused: the reference renderer ignores it",
                            Some(entry.range),
                        ))
                    }
                    _ => false,
                }
            }
            "gestures" => {
                return Err(refuse(
                    "clipboard gesture policies are not implemented for the web",
                    Some(entry.range),
                ))
            }
            _ => false,
        };
        if !allowed {
            return Err(refuse(
                format!(
                    "unsupported construct in {INTERACTIVE_PROFILE}: {} {}",
                    entry.kind, entry.name
                ),
                Some(entry.range),
            ));
        }
    }

    // 2. Elements, with text taken from the reference frame so the browser
    // never re-implements wrapping.
    let scene = &parsed.scene;
    let mut cells = Vec::new();
    collect(&scene.root, None, &mut cells);
    let frame = goya_lang::layout(scene, 1.0, 1.0);
    if frame.nodes.len() != cells.len() {
        return Err(refuse("reference frame does not match the scene", None));
    }
    let mut out = Vec::with_capacity(cells.len());
    for (index, (cell, parent)) in cells.into_iter().enumerate() {
        let id = cell.authored_id.clone().unwrap_or_default();
        let element = Origin::Element(id.clone());
        let property = |name: &str| Origin::Property {
            element: id.clone(),
            property: name.to_owned(),
        };
        if index == MAX_CELLS {
            return Err(refuse("more than 1024 elements in one scene", at(&element)));
        }
        if matches!(cell.kind, CellKind::Button | CellKind::Input) {
            if let Some(child) = cell.children.first() {
                return Err(refuse(
                    "elements nested inside a button or input",
                    at(&Origin::Element(
                        child.authored_id.clone().unwrap_or_default(),
                    )),
                ));
            }
        }
        if cell.stroke.is_some() != cell.stroke_width_p.is_some() {
            let (name, what) = if cell.stroke.is_some() {
                ("stroke", "a stroke colour without stroke-width-p")
            } else {
                ("stroke_width_p", "stroke-width-p without a stroke colour")
            };
            return Err(refuse(what, at(&property(name))));
        }
        if cell.scroll {
            if cell.kind != CellKind::Cell {
                return Err(refuse(
                    "`scroll` on a button or input (they cannot have children)",
                    at(&property("scroll")),
                ));
            }
            let mut ancestor = parent;
            while let Some(a) = ancestor {
                if out_scroll(&out, a) {
                    return Err(refuse(
                        "a scroll viewport inside another scroll viewport (the reference counts nested content in the outer limit; browsers do not)",
                        at(&property("scroll")),
                    ));
                }
                ancestor = out_parent(&out, a);
            }
            if frame.nodes.iter().filter(|n| n.name == cell.name).count() > 1 {
                return Err(refuse(
                    "a scroll viewport whose structural name is not unique (the reference keys scroll offsets by name)",
                    at(&property("scroll")),
                ));
            }
        }
        let text = frame.nodes[index].text.clone();
        if let Some(text) = &text {
            if text.len() > MAX_TEXT_BYTES {
                return Err(refuse(
                    "element text above 16 KiB of UTF-8",
                    at(&property("text")),
                ));
            }
            if text.contains(['\0', '\r']) {
                return Err(refuse(
                    "text contains a character the browser renders differently (U+0000 or carriage return)",
                    at(&property("text")),
                ));
            }
        }
        let pair = |v: Option<goya_lang::Vec3>| v.map(|v| [v.x, v.y]);
        out.push(WebCell {
            id,
            name: cell.name.clone(),
            kind: match cell.kind {
                CellKind::Button => WebCellKind::Button,
                CellKind::Input => WebCellKind::Input,
                _ => WebCellKind::Cell,
            },
            parent,
            position_p: pair(cell.position_p),
            extent_p: pair(cell.extent_p),
            fill: cell.fill,
            stroke: cell.stroke.zip(cell.stroke_width_p),
            text_size_p: cell.text_size_p,
            text,
            scroll: cell.scroll,
            range: at(&element).unwrap_or(whole),
        });
    }

    // 3. State and interactions.
    let mut snapshot_bytes = 8usize;
    for decl in handoff.state() {
        let origin = Origin::State(decl.key.clone());
        if decl.lifetime == StateLifetime::Durable {
            return Err(refuse(
                "durable state needs a browser storage adapter with a lifecycle contract",
                at(&origin),
            ));
        }
        let value_bytes = match &decl.initial {
            InteractionValue::Int(_) => 8,
            InteractionValue::Bool(_) => 1,
            InteractionValue::Text(text) if text.len() > D1_MAX_TEXT_BYTES => {
                return Err(refuse(
                    "an initial text value above the 64 KiB D1 limit",
                    at(&origin),
                ))
            }
            InteractionValue::Text(text) => text.len(),
        };
        snapshot_bytes += 8 + decl.key.len() + value_bytes;
    }
    if snapshot_bytes > D1_MAX_SNAPSHOT_BYTES {
        return Err(refuse(
            "an initial state snapshot above the 1 MiB D1 limit",
            parsed.declarations.state.as_ref().map(|(_, range)| *range),
        ));
    }
    for interaction in &scene.interactions {
        if interaction.class == InteractionClass::D2 {
            return Err(refuse(
                "D2 interactions (no browser capability adapters)",
                at(&Origin::Interaction(
                    interaction.authored_id.clone().unwrap_or_default(),
                )),
            ));
        }
    }

    Ok(InteractiveWebIr {
        handoff,
        source: source.to_owned(),
        cells: out,
    })
}

fn out_scroll(cells: &[WebCell], index: usize) -> bool {
    cells[index].scroll
}

fn out_parent(cells: &[WebCell], index: usize) -> Option<usize> {
    cells[index].parent
}

fn collect<'a>(
    cell: &'a SceneCell,
    parent: Option<usize>,
    out: &mut Vec<(&'a SceneCell, Option<usize>)>,
) {
    let index = out.len();
    out.push((cell, parent));
    for child in &cell.children {
        collect(child, Some(index), out);
    }
}

fn cell_at<'a>(root: &'a SceneCell, path: &[usize]) -> Option<&'a SceneCell> {
    path.iter()
        .try_fold(root, |cell, &index| cell.children.get(index))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNTER: &str = include_str!("../../../fixtures/nevada/counter.gscene");

    fn refused(source: &str) -> (String, &str) {
        let error = admit_local_interactive(source, INTERACTIVE_PROFILE).unwrap_err();
        let range = error.range.expect("refusals carry a range");
        (error.code.to_owned(), &source[range.start..range.end])
    }

    #[test]
    fn counter_is_admitted_with_elements_in_draw_order() {
        let ir = admit_local_interactive(COUNTER, INTERACTIVE_PROFILE).unwrap();
        let ids: Vec<_> = ir.cells().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(
            ids,
            ["window.root", "window.title", "window.amount", "window.add"]
        );
        assert_eq!(ir.cells()[2].kind, WebCellKind::Input);
        assert_eq!(ir.cells()[2].parent, Some(0));
        assert_eq!(ir.cells()[2].stroke, Some((0x333333, 0.05)));
        assert_eq!(ir.cells()[3].kind, WebCellKind::Button);
        assert_eq!(ir.cells()[1].text.as_deref(), Some("Counter"));
        assert_eq!(ir.cell_by_name("add").unwrap().id, "window.add");
        assert_eq!(ir.handoff().contract(), "GSSH/3");
        assert_eq!(ir.source(), COUNTER);
    }

    #[test]
    fn every_unsupported_construct_is_refused_at_its_own_range() {
        let cases = [
            (
                COUNTER.replace("stroke: #333;", "stroke: #333; text: \"5\";"),
                "text: \"5\"",
            ),
            (
                COUNTER.replace("text: \"Add\";", "text: \"Add\"; scroll: true;"),
                "scroll: true",
            ),
            (
                COUNTER.replace(
                    "id: \"window.root\";",
                    "id: \"window.root\"; scroll: true;",
                )
                .replace(
                    "text: \"Counter\";",
                    "text: \"Counter\"; scroll: true;",
                ),
                "scroll: true",
            ),
            (
                COUNTER.replace(
                    "text: \"Counter\";",
                    "text: \"Counter\"; text-anchor: middle;",
                ),
                "text-anchor: middle",
            ),
            (
                COUNTER.replace("= 0 session", "= 0 durable"),
                "count: int = 0 durable",
            ),
            (
                COUNTER.replace("stroke: #333; stroke_width_p: 0.05;", "stroke: #333;"),
                "stroke: #333",
            ),
            (
                COUNTER.replace(
                    "text: \"Add\";",
                    "text: \"Add\"; @cell inner { id: \"window.inner\"; }",
                ),
                "@cell inner { id: \"window.inner\"; }",
            ),
            (
                COUNTER.replace("text: \"Counter\";", "text: \"Coun\rter\";"),
                "text: \"Coun\rter\"",
            ),
        ];
        for (source, expected) in &cases {
            let (code, text) = refused(source);
            assert_eq!(code, "MAJA-E0200", "{expected}");
            assert!(
                text.starts_with(expected),
                "expected range {expected:?}, got {text:?}"
            );
        }
    }

    #[test]
    fn scroll_viewports_are_admitted_when_unambiguous() {
        let source = COUNTER.replace("text: \"Counter\";", "text: \"Counter\"; scroll: true;");
        let ir = admit_local_interactive(&source, INTERACTIVE_PROFILE).unwrap();
        assert!(ir.cells()[1].scroll);
        assert!(!ir.cells()[0].scroll);
        let duplicate = r#"@ontology d { @cell root { id: "r"; @cell a { id: "a1"; scroll: true; } @cell b { id: "b"; @cell a { id: "a2"; } } } }"#;
        let (code, text) = refused(duplicate);
        assert_eq!(code, "MAJA-E0200");
        assert!(text.starts_with("scroll: true"), "{text:?}");
    }

    #[test]
    fn admission_errors_and_unknown_profiles_fail_closed() {
        let absolute = COUNTER.replace("text_size_p: 0.6", "text_size: 12");
        let error = admit_local_interactive(&absolute, INTERACTIVE_PROFILE).unwrap_err();
        assert_eq!(error.code, "GY-E1009");
        assert_eq!(
            admit_local_interactive(COUNTER, "maja.web.local.v9")
                .unwrap_err()
                .code,
            "MAJA-E0002"
        );
        let d2 = COUNTER.replace("class: D1;", "class: D2;");
        assert!(admit_local_interactive(&d2, INTERACTIVE_PROFILE).is_err());
    }
}
