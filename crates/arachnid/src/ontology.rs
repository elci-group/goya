//! The webOS ontology: strict admission, layout at the 1920×1080 ontology
//! limit, and the compile-time focus graph.
//!
//! Contract `arachnid.ontology/0.0.1`.
//!
//! **Ontology limit.** Every webOS build resolves the `@ontology` against
//! exactly 1920 × 1080 points. This is LG's default web-app resolution
//! (`appinfo.json` `"resolution": "1920x1080"`). Web apps never get a UHD
//! surface, so no other size has to be supported. One point is one CSS
//! pixel at that resolution. Geometry is proportional only: the source must
//! pass GSSH/3 admission (`parse_strict_gssh3`), which rejects absolute
//! lengths. Every resolved node must lie inside the limit.
//!
//! **Admission.** The current lowering covers cells and buttons: text,
//! fill, stroke, text size, anchor and wrap. Every other admitted GSSH/3
//! construct is refused with a stable `GY-AR-61x` code rather than dropped.
//! That covers `@interaction`, `@state` and bindings, `@input`, `@intent`,
//! gesture policies and `scroll`.
//!
//! **Focus.** Every button is actionable and essential. Its rectangle at
//! the ontology limit feeds the copied focus engine. The build fails when:
//! * any actionable element has no area (`GY-AR-630`);
//! * two actionable elements overlap (`GY-AR-631`), since a pointer click
//!   would be ambiguous;
//! * any actionable element is unreachable by the arrow keys (`GY-AR-640`);
//! * navigation can trap the user (`GY-AR-641`).

use crate::diagnostics::{Code, Diagnostic};
use crate::focus::{self, Direction, FocusElement, FocusSuccessors};
use crate::traps;
use goya_lang::{layout, parse_strict_gssh3, Frame, ParsedScene, RectMp, SceneCell, SourceRange};

pub const CONTRACT: &str = "arachnid.ontology/0.0.1";

/// Ontology limit for webOS builds, in points (= CSS pixels).
pub const ONTOLOGY_WIDTH_PT: u32 = 1920;
pub const ONTOLOGY_HEIGHT_PT: u32 = 1080;
/// The same limit in millipoints.
pub const ONTOLOGY_LIMIT: RectMp = RectMp {
    x: 0,
    y: 0,
    width: ONTOLOGY_WIDTH_PT as i64 * 1000,
    height: ONTOLOGY_HEIGHT_PT as i64 * 1000,
};

/// Upper bound on actionable elements. It keeps the trap check (one search
/// per element) and the generated focus table bounded.
pub const MAX_ACTIONABLE: usize = 512;

/// One actionable element and its place in the frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Actionable {
    /// Focus id: the element's index among actionable elements, in draw order.
    pub focus_id: u32,
    /// Index into `Frame::nodes`.
    pub node: usize,
    pub name: String,
    pub authored_id: Option<String>,
}

/// An admitted scene, laid out at the ontology limit, with its focus graph.
#[derive(Debug)]
pub struct Ontology {
    pub parsed: ParsedScene,
    pub frame: Frame,
    /// Authored id of each frame node (`None` when not authored), by index.
    pub authored_ids: Vec<Option<String>>,
    pub actionable: Vec<Actionable>,
    pub focus: Vec<FocusElement>,
    pub graph: Vec<FocusSuccessors>,
    /// Initial focus id, or `None` when nothing is actionable.
    pub initial: Option<u32>,
}

fn range_of(parsed: &ParsedScene, name: &str, authored: Option<&str>) -> Option<SourceRange> {
    parsed
        .entries
        .iter()
        .find(|e| {
            e.kind != "property"
                && (authored.is_some_and(|id| e.semantic_id.as_deref() == Some(id))
                    || e.name == name)
        })
        .map(|e| e.range)
}

fn preorder<'a>(cell: &'a SceneCell, out: &mut Vec<&'a SceneCell>) {
    out.push(cell);
    for child in &cell.children {
        preorder(child, out);
    }
}

fn inside(rect: &RectMp, limit: &RectMp) -> bool {
    rect.x >= limit.x
        && rect.y >= limit.y
        && rect.x as i128 + rect.width as i128 <= limit.x as i128 + limit.width as i128
        && rect.y as i128 + rect.height as i128 <= limit.y as i128 + limit.height as i128
}

fn overlap(a: &RectMp, b: &RectMp) -> bool {
    let (ax1, ay1) = (
        a.x as i128 + a.width as i128,
        a.y as i128 + a.height as i128,
    );
    let (bx1, by1) = (
        b.x as i128 + b.width as i128,
        b.y as i128 + b.height as i128,
    );
    (a.x as i128) < bx1 && (b.x as i128) < ax1 && (a.y as i128) < by1 && (b.y as i128) < ay1
}

/// Admit `source` as a webOS ontology. Returns every diagnostic found,
/// not only the first, except that a source rejected by GSSH/3 stops there.
pub fn admit(source: &str) -> Result<Ontology, Vec<Diagnostic>> {
    let parsed = parse_strict_gssh3(source).map_err(|e| {
        vec![Diagnostic::new(Code::SourceRejected, format!("{}: {}", e.code, e)).at(Some(e.range))]
    })?;
    let mut diags = Vec::new();
    let scene = &parsed.scene;

    for interaction in &scene.interactions {
        diags.push(
            Diagnostic::new(
                Code::InteractionNotLowered,
                format!(
                    "@interaction {} is not lowered to webOS yet",
                    interaction.name
                ),
            )
            .at(range_of(&parsed, &interaction.name, None)),
        );
    }
    if let Some((_, range)) = &parsed.declarations.state {
        diags.push(
            Diagnostic::new(Code::StateNotLowered, "@state is not lowered to webOS yet")
                .at(Some(*range)),
        );
    }
    for binding in &parsed.declarations.bindings {
        diags.push(
            Diagnostic::new(
                Code::StateNotLowered,
                format!(
                    "input binding {} is not lowered to webOS yet",
                    binding.input
                ),
            )
            .at(Some(binding.range)),
        );
    }
    for intent in &scene.intents {
        diags.push(
            Diagnostic::new(
                Code::IntentNotLowered,
                format!("@intent {} has no webOS lowering", intent.name),
            )
            .at(range_of(&parsed, &intent.name, None)),
        );
    }

    let mut cells = Vec::new();
    preorder(&scene.root, &mut cells);
    for cell in &cells {
        let range = || range_of(&parsed, &cell.name, cell.authored_id.as_deref());
        if cell.is_input {
            diags.push(
                Diagnostic::new(
                    Code::InputNotLowered,
                    format!("@input {} is not lowered to webOS yet", cell.name),
                )
                .at(range()),
            );
        }
        if cell.gestures.is_some() {
            diags.push(
                Diagnostic::new(
                    Code::GesturesNotLowered,
                    format!(
                        "gesture policy on {} is not lowered to webOS yet",
                        cell.name
                    ),
                )
                .at(range()),
            );
        }
        if cell.scroll {
            diags.push(
                Diagnostic::new(
                    Code::ScrollNotLowered,
                    format!("scroll on {} is not lowered to webOS yet", cell.name),
                )
                .at(range()),
            );
        }
    }

    let frame = layout(scene, ONTOLOGY_WIDTH_PT as f64, ONTOLOGY_HEIGHT_PT as f64);
    if frame.nodes.len() != cells.len() {
        diags.push(Diagnostic::new(
            Code::SourceRejected,
            format!(
                "layout produced {} nodes for {} scene cells; refusing to guess the mapping",
                frame.nodes.len(),
                cells.len()
            ),
        ));
        return Err(diags);
    }
    let authored_ids: Vec<Option<String>> = cells.iter().map(|c| c.authored_id.clone()).collect();

    let mut actionable = Vec::new();
    for (index, node) in frame.nodes.iter().enumerate() {
        let range = || range_of(&parsed, &node.name, authored_ids[index].as_deref());
        if !inside(&node.rect, &ONTOLOGY_LIMIT) {
            diags.push(
                Diagnostic::new(
                    Code::OutsideOntologyLimit,
                    format!("{} falls outside the {ONTOLOGY_WIDTH_PT}×{ONTOLOGY_HEIGHT_PT} ontology limit", node.name),
                )
                .at(range()),
            );
        }
        if node.is_button {
            if node.rect.width <= 0 || node.rect.height <= 0 {
                diags.push(
                    Diagnostic::new(
                        Code::ActionableWithoutArea,
                        format!("button {} has no area at the ontology limit", node.name),
                    )
                    .at(range()),
                );
            }
            actionable.push(Actionable {
                focus_id: actionable.len() as u32,
                node: index,
                name: node.name.clone(),
                authored_id: authored_ids[index].clone(),
            });
        }
    }

    if actionable.len() > MAX_ACTIONABLE {
        diags.push(Diagnostic::new(
            Code::TooManyActionable,
            format!(
                "{} actionable elements exceed the bound of {MAX_ACTIONABLE}; split the ontology into nested scenes",
                actionable.len()
            ),
        ));
        return Err(diags);
    }

    for (i, a) in actionable.iter().enumerate() {
        for b in &actionable[i + 1..] {
            if overlap(&frame.nodes[a.node].rect, &frame.nodes[b.node].rect) {
                diags.push(
                    Diagnostic::new(
                        Code::ActionableOverlap,
                        format!(
                            "buttons {} and {} overlap; a pointer click would be ambiguous",
                            a.name, b.name
                        ),
                    )
                    .at(range_of(&parsed, &b.name, b.authored_id.as_deref())),
                );
            }
        }
    }

    let focus: Vec<FocusElement> = actionable
        .iter()
        .map(|a| {
            let r = frame.nodes[a.node].rect;
            FocusElement {
                id: a.focus_id,
                x: r.x,
                y: r.y,
                w: r.width,
                h: r.height,
                focusable: r.width > 0 && r.height > 0,
                essential: true,
            }
        })
        .collect();
    let initial = focus::next_focus(&focus, None, Direction::Down);
    let graph = focus::focus_graph(&focus);
    let name = |id: u32| actionable[id as usize].name.as_str();

    if !actionable.is_empty() {
        for id in focus::unreachable_essential(&focus, initial) {
            let a = &actionable[id as usize];
            diags.push(
                Diagnostic::new(
                    Code::FocusUnreachable,
                    format!("button {} cannot be reached with the arrow keys", a.name),
                )
                .at(range_of(&parsed, &a.name, a.authored_id.as_deref())),
            );
        }
        if let Some(start) = initial {
            for trap in traps::traps(&focus, start) {
                let lost: Vec<&str> = trap.unreachable.iter().map(|&id| name(id)).collect();
                let a = &actionable[trap.from as usize];
                diags.push(
                    Diagnostic::new(
                        Code::FocusTrap,
                        format!(
                            "after focusing {}, the arrow keys can no longer reach {}",
                            a.name,
                            lost.join(", ")
                        ),
                    )
                    .at(range_of(&parsed, &a.name, a.authored_id.as_deref())),
                );
            }
        }
    }

    if diags.is_empty() {
        Ok(Ontology {
            parsed,
            frame,
            authored_ids,
            actionable,
            focus,
            graph,
            initial,
        })
    } else {
        Err(diags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(result: Result<Ontology, Vec<Diagnostic>>) -> Vec<u16> {
        result
            .expect_err("expected refusal")
            .iter()
            .map(|d| d.code.number())
            .collect()
    }

    const GRID: &str = r#"@ontology media {
  @cell root { id: "root"; fill: #101418;
    @cell title { id: "title"; position_p: (0.05, 0.05, 0); extent_p: (0.9, 0.1, 0); text: "Goya Media Centre"; text-size-p: 0.6; fill: #101418; }
    @button films { id: "films"; position_p: (0.05, 0.25, 0); extent_p: (0.2, 0.2, 0); text: "Films"; text-size-p: 0.2; fill: #2a3440; }
    @button tv { id: "tv"; position_p: (0.4, 0.25, 0); extent_p: (0.2, 0.2, 0); text: "Television"; text-size-p: 0.2; fill: #2a3440; }
    @button music { id: "music"; position_p: (0.75, 0.25, 0); extent_p: (0.2, 0.2, 0); text: "Music"; text-size-p: 0.2; fill: #2a3440; }
    @button photos { id: "photos"; position_p: (0.05, 0.6, 0); extent_p: (0.2, 0.2, 0); text: "Photos"; text-size-p: 0.2; fill: #2a3440; }
    @button library { id: "library"; position_p: (0.4, 0.6, 0); extent_p: (0.2, 0.2, 0); text: "Library"; text-size-p: 0.2; fill: #2a3440; }
    @button settings { id: "settings"; position_p: (0.75, 0.6, 0); extent_p: (0.2, 0.2, 0); text: "Settings"; text-size-p: 0.2; fill: #2a3440; }
  }
}"#;

    #[test]
    fn grid_is_admitted_at_the_ontology_limit() {
        let o = admit(GRID).unwrap_or_else(|d| panic!("{d:?}"));
        assert_eq!(o.frame.nodes[0].rect, ONTOLOGY_LIMIT);
        assert_eq!(o.actionable.len(), 6);
        assert_eq!(o.initial, Some(0));
        let films = &o.frame.nodes[o.actionable[0].node];
        assert_eq!((films.rect.x, films.rect.y), (96_000, 270_000));
        assert_eq!((films.rect.width, films.rect.height), (384_000, 216_000));
        // Right from films → tv; Down from films → photos.
        let row = o.graph.iter().find(|r| r.id == 0).unwrap();
        assert_eq!(row.successors[Direction::Right.slot()], Some(1));
        assert_eq!(row.successors[Direction::Down.slot()], Some(3));
        assert_eq!(o.authored_ids[2].as_deref(), Some("films"));
    }

    #[test]
    fn static_text_without_buttons_is_admitted() {
        let o =
            admit(r#"@ontology s { @cell root { id: "root"; text: "Hello"; text-size-p: 0.1; } }"#)
                .unwrap();
        assert!(o.actionable.is_empty());
        assert_eq!(o.initial, None);
    }

    #[test]
    fn absolute_lengths_are_rejected_by_gssh3() {
        let c = codes(admit(
            r#"@ontology s { @cell root { id: "root"; extent: (10, 10, 0); } }"#,
        ));
        assert_eq!(c, vec![601]);
    }

    #[test]
    fn diagonal_button_is_unreachable() {
        let src = r#"@ontology s { @cell root { id: "root";
            @button a { id: "a"; position_p: (0.0, 0.0, 0); extent_p: (0.1, 0.1, 0); }
            @button b { id: "b"; position_p: (0.5, 0.5, 0); extent_p: (0.1, 0.1, 0); }
        } }"#;
        assert_eq!(codes(admit(src)), vec![640]);
    }

    #[test]
    fn overlapping_buttons_are_refused() {
        let src = r#"@ontology s { @cell root { id: "root";
            @button a { id: "a"; position_p: (0.0, 0.0, 0); extent_p: (0.3, 0.3, 0); }
            @button b { id: "b"; position_p: (0.2, 0.0, 0); extent_p: (0.3, 0.3, 0); }
        } }"#;
        assert!(codes(admit(src)).contains(&631));
    }

    #[test]
    fn zero_area_button_is_refused() {
        let src = r#"@ontology s { @cell root { id: "root";
            @button a { id: "a"; position_p: (0.0, 0.0, 0); extent_p: (0.0, 0.3, 0); }
        } }"#;
        assert!(codes(admit(src)).contains(&630));
    }

    #[test]
    fn one_way_layout_is_a_trap() {
        // The topmost-leftmost button ("high") is the initial focus. Left
        // from it reaches the pillar. From the pillar, Right reaches the
        // nearer "low" box, and no direction from the pillar or "low" leads
        // back to "high" (see traps.rs for the geometry).
        let src = r#"@ontology s { @cell root { id: "root";
            @button pillar { id: "pillar"; position_p: (0.0, 0.01, 0); extent_p: (0.05, 0.9, 0); }
            @button low { id: "low"; position_p: (0.1, 0.5, 0); extent_p: (0.05, 0.05, 0); }
            @button high { id: "high"; position_p: (0.2, 0.0, 0); extent_p: (0.05, 0.05, 0); }
        } }"#;
        let Err(diags) = admit(src) else {
            panic!("trap must be refused")
        };
        let traps: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == Code::FocusTrap).collect();
        assert_eq!(traps.len(), 2, "{diags:?}");
        assert!(
            traps[0].message.contains("can no longer reach high"),
            "{}",
            traps[0].message
        );
        assert!(!diags.iter().any(|d| d.code == Code::FocusUnreachable));
    }

    #[test]
    fn unlowered_constructs_are_refused_not_dropped() {
        let src = r#"@ontology counter {
  @state { count: int = 0 session; }
  @cell root { id: "root"; @button go { id: "button.go"; text: "Go"; position_p: (0.1, 0.1, 0); extent_p: (0.2, 0.2, 0); } }
  @interaction add { id: "action.add"; trigger: button.go.activate; class: D1; state { read count: int; write count: int; } input { } set count = state.count + 1; }
}"#;
        let c = codes(admit(src));
        assert!(c.contains(&610), "{c:?}");
        assert!(c.contains(&611), "{c:?}");
    }
}
