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
//! **Admission.** Legacy sources pass GSSH/3 live admission. Sources with
//! explicit platform exceptions pass GSSH/4 live admission, which also
//! requires an authored persistence decision. The lowering covers
//! cells and buttons (text, fill, stroke, text size, anchor, wrap), session
//! and host `@state`, D1 and D2 `@interaction`s, and the live bindings
//! `text-from`, `image-from` and `present-when` (see `live.rs`). Every other
//! admitted construct is refused with a stable `GY-AR-61x` code rather than
//! dropped: `@input` and input bindings, durable state, `@intent`, gesture
//! policies and `scroll`.
//!
//! **Focus.** Every button is actionable and essential. Its rectangle at
//! the ontology limit feeds the copied focus engine. The build fails when:
//! * any actionable element has no area (`GY-AR-630`);
//! * two actionable elements overlap (`GY-AR-631`), since a pointer click
//!   would be ambiguous;
//! * any actionable element is unreachable by the arrow keys (`GY-AR-640`);
//! * navigation can trap the user (`GY-AR-641`).
//!
//! With `present-when`, these checks run once per presence configuration
//! over the buttons present in it (`live::Presence`).

use crate::diagnostics::{Code, Diagnostic};
use crate::focus::{self, Direction, FocusElement, FocusSuccessors};
use crate::live::{self, Lowered, Presence, StateKey};
use crate::traps;
use goya_lang::{
    layout, parse_strict_gssh3_live, parse_strict_gssh4_live, Frame, InteractionValue,
    ParsedScene, PlatformUse, RectMp, SceneCell, SourceRange,
};

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

/// One distinct presence configuration with its own focus graph.
#[derive(Clone, Debug)]
pub struct Configuration {
    /// A value combination that produces it, described for diagnostics.
    pub when: String,
    /// Presence of every frame node.
    pub present_nodes: Vec<bool>,
    /// Focus ids of the buttons present, ascending.
    pub present: Vec<u32>,
    pub graph: Vec<FocusSuccessors>,
    pub initial: Option<u32>,
}

/// An admitted scene, laid out at the ontology limit, with its focus graph.
#[derive(Debug)]
pub struct Ontology {
    pub parsed: ParsedScene,
    pub frame: Frame,
    /// Authored id of each frame node (`None` when not authored), by index.
    pub authored_ids: Vec<Option<String>>,
    pub actionable: Vec<Actionable>,
    /// Every actionable element, present or not.
    pub focus: Vec<FocusElement>,
    /// Focus graph and initial focus of the initial configuration.
    pub graph: Vec<FocusSuccessors>,
    /// Initial focus id, or `None` when nothing is actionable.
    pub initial: Option<u32>,
    pub presence: Presence,
    pub configurations: Vec<Configuration>,
    /// Index of the configuration the initial state produces.
    pub initial_configuration: usize,
    pub state: Vec<StateKey>,
    pub interactions: Vec<Lowered>,
    /// `text-from` / `image-from` key per frame node.
    pub text_from: Vec<Option<String>>,
    pub image_from: Vec<Option<String>>,
    pub remote_digit: Vec<Option<u8>>,
    pub motion: crate::motion::Motion,
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

fn preorder<'a>(
    cell: &'a SceneCell,
    parent: Option<usize>,
    out: &mut Vec<&'a SceneCell>,
    parents: &mut Vec<Option<usize>>,
) {
    let me = out.len();
    out.push(cell);
    parents.push(parent);
    for child in &cell.children {
        preorder(child, Some(me), out, parents);
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

/// Admit `source` as a webOS ontology. A platform-scoped source must satisfy
/// GSSH/4 live and every webOS exception before lowering.
pub fn admit(source: &str) -> Result<Ontology, Vec<Diagnostic>> {
    let parsed = parse_strict_gssh3_live(source).or_else(|error| {
        if error.code == "GY-E1015" {
            parse_strict_gssh4_live(source)
        } else {
            Err(error)
        }
    }).map_err(|e| {
        vec![Diagnostic::new(Code::SourceRejected, format!("{}: {}", e.code, e)).at(Some(e.range))]
    })?;
    for rule in parsed.declarations.platform_exceptions.iter().filter(|rule| rule.platform == "webos") {
        let implemented = match (rule.feature.as_str(), rule.use_kind, rule.options.as_slice()) {
            ("media.playback", PlatformUse::OnlyUse, [option]) if option == "html.video" => true,
            ("desktop.link", PlatformUse::MustUse, [option]) if option == "goya.lanlink/0" =>
                parsed.declarations.links.iter().any(|link| link.protocol == "goya.lanlink/0"),
            _ => false,
        };
        if !implemented {
            return Err(vec![Diagnostic::new(Code::SourceRejected,
                format!("GY-E1016: webOS cannot satisfy platform exception '{}' ({:?})", rule.feature, rule.use_kind))
                .at(Some(rule.range))]);
        }
    }
    let mut diags = Vec::new();
    let scene = &parsed.scene;

    let mut interactions = Vec::new();
    for interaction in &scene.interactions {
        match live::lower(interaction) {
            Ok(l) => interactions.push(l),
            Err(why) => diags.push(
                Diagnostic::new(
                    Code::InputNotLowered,
                    format!("@interaction {}: {why}", interaction.name),
                )
                .at(range_of(&parsed, &interaction.name, None)),
            ),
        }
    }
    diags.extend(live::refuse_durable(&parsed));
    diags.extend(live::refuse_unsafe_ints(&parsed));
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
    let mut parents = Vec::new();
    preorder(&scene.root, None, &mut cells, &mut parents);
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

    let remote_digit: Vec<_> = cells.iter().map(|cell| cell.remote_digit).collect();
    for (node, digit) in remote_digit.iter().enumerate() {
        if digit.is_some() && !actionable.iter().any(|button| button.node == node) {
            diags.push(Diagnostic::new(Code::HostContract, "remote-digit requires a button"));
        }
    }
    let presence = Presence::from_cells(&cells, &parents);
    if presence.signature_count().is_none() {
        diags.push(Diagnostic::new(
            Code::TooManyConfigurations,
            format!(
                "present-when conditions combine into more than {} value combinations; compare fewer keys or literals",
                live::MAX_SIGNATURES
            ),
        ));
        return Err(diags);
    }
    let state = live::state_keys(&parsed);
    let initial_signature = presence.signature_of(|key| {
        state
            .iter()
            .find(|k| k.key == key)
            .map_or(InteractionValue::Bool(false), |k| k.initial.clone())
    });
    let node_count = cells.len();
    let presence_of = |sig: &live::Signature| -> Vec<bool> {
        (0..node_count).map(|i| presence.present(i, sig)).collect()
    };
    // Distinct sets of present nodes, in first-seen order.
    let mut distinct: Vec<(Vec<bool>, String)> = Vec::new();
    let mut index: std::collections::HashMap<Vec<bool>, usize> = std::collections::HashMap::new();
    let mut overflow = false;
    presence.for_each_signature(|sig| {
        if overflow {
            return;
        }
        let nodes = presence_of(sig);
        if !index.contains_key(&nodes) {
            if distinct.len() >= live::MAX_CONFIGURATIONS {
                overflow = true;
                return;
            }
            index.insert(nodes.clone(), distinct.len());
            distinct.push((nodes, presence.describe(sig)));
        }
    });
    if overflow {
        diags.push(Diagnostic::new(
            Code::TooManyConfigurations,
            format!(
                "present-when conditions produce more than {} distinct screens; nest conditions so fewer combine",
                live::MAX_CONFIGURATIONS
            ),
        ));
        return Err(diags);
    }
    let initial_nodes = presence_of(&initial_signature);

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
    let name = |id: u32| actionable[id as usize].name.as_str();
    let mut configurations = Vec::new();
    let mut reported_digits = std::collections::BTreeSet::new();
    let mut reported_overlaps = std::collections::BTreeSet::new();
    let mut reported = std::collections::BTreeSet::new();
    for (present_nodes, when) in distinct {
        let suffix = if presence.keys.is_empty() {
            String::new()
        } else {
            format!(" (when {when})")
        };
        let present: Vec<&Actionable> = actionable
            .iter()
            .filter(|a| present_nodes[a.node])
            .collect();
        let mut digits = [None; 10];
        for button in &present {
            if let Some(digit) = remote_digit[button.node] {
                if let Some(other) = digits[usize::from(digit)].replace(button.name.as_str()) {
                    if reported_digits.insert((digit, other, button.name.as_str())) {
                        diags.push(Diagnostic::new(Code::HostContract, format!(
                            "buttons {other} and {} share remote digit {digit}{suffix}", button.name
                        )));
                    }
                }
            }
        }
        for (i, a) in present.iter().enumerate() {
            for b in &present[i + 1..] {
                if overlap(&frame.nodes[a.node].rect, &frame.nodes[b.node].rect)
                    && reported_overlaps.insert((a.focus_id, b.focus_id))
                {
                    diags.push(
                        Diagnostic::new(
                            Code::ActionableOverlap,
                            format!(
                                "buttons {} and {} overlap{suffix}; a pointer click would be ambiguous",
                                a.name, b.name
                            ),
                        )
                        .at(range_of(&parsed, &b.name, b.authored_id.as_deref())),
                    );
                }
            }
        }
        let elements: Vec<FocusElement> = present
            .iter()
            .map(|a| focus[a.focus_id as usize])
            .collect();
        let initial = focus::next_focus(&elements, None, Direction::Down);
        let graph = focus::focus_graph(&elements);
        if !elements.is_empty() {
            for id in focus::unreachable_essential(&elements, initial) {
                let a = &actionable[id as usize];
                if reported.insert((Code::FocusUnreachable, id, when.clone())) {
                    diags.push(
                        Diagnostic::new(
                            Code::FocusUnreachable,
                            format!("button {} cannot be reached with the arrow keys{suffix}", a.name),
                        )
                        .at(range_of(&parsed, &a.name, a.authored_id.as_deref())),
                    );
                }
            }
            if let Some(start) = initial {
                for trap in traps::traps(&elements, start) {
                    let lost: Vec<&str> = trap.unreachable.iter().map(|&id| name(id)).collect();
                    let a = &actionable[trap.from as usize];
                    diags.push(
                        Diagnostic::new(
                            Code::FocusTrap,
                            format!(
                                "after focusing {}, the arrow keys can no longer reach {}{suffix}",
                                a.name,
                                lost.join(", ")
                            ),
                        )
                        .at(range_of(&parsed, &a.name, a.authored_id.as_deref())),
                    );
                }
            }
        }
        configurations.push(Configuration {
            when,
            present_nodes,
            present: present.iter().map(|a| a.focus_id).collect(),
            graph,
            initial,
        });
    }
    let initial_configuration = configurations
        .iter()
        .position(|c| c.present_nodes == initial_nodes)
        .expect("the initial state's presence is one of the enumerated configurations");
    let graph = configurations[initial_configuration].graph.clone();
    let initial = configurations[initial_configuration].initial;
    for l in &interactions {
        if !actionable.iter().any(|a| a.name == l.trigger_button) {
            diags.push(Diagnostic::new(
                Code::SourceRejected,
                format!("interaction {} is triggered by {}, which is not a button", l.name, l.trigger_button),
            ));
        }
    }
    let actionable_nodes: Vec<usize> = actionable.iter().map(|a| a.node).collect();
    let co_present = |a: usize, b: usize| {
        configurations
            .iter()
            .any(|c| c.present_nodes[a] && c.present_nodes[b])
    };
    let (motion, motion_diags) = crate::motion::analyse(
        &cells,
        &parents,
        &frame,
        &ONTOLOGY_LIMIT,
        &actionable_nodes,
        &co_present,
    );
    diags.extend(motion_diags);
    let text_from = cells.iter().map(|c| c.text_from.clone()).collect();
    let image_from = cells.iter().map(|c| c.image_from.clone()).collect();

    if diags.is_empty() {
        Ok(Ontology {
            parsed,
            frame,
            authored_ids,
            actionable,
            focus,
            graph,
            initial,
            presence,
            configurations,
            initial_configuration,
            state,
            interactions,
            text_from,
            image_from,
            remote_digit,
            motion,
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
    fn d1_state_is_lowered_and_durable_state_refused() {
        let src = r#"@ontology counter {
  @state { count: int = 0 session; }
  @cell root { id: "root"; @button go { id: "button.go"; text: "Go"; position_p: (0.1, 0.1, 0); extent_p: (0.2, 0.2, 0); } }
  @interaction add { id: "action.add"; trigger: button.go.activate; class: D1; state { read count: int; write count: int; } input { } set count = state.count + 1; }
}"#;
        let o = admit(src).unwrap_or_else(|d| panic!("{d:?}"));
        assert_eq!(o.interactions.len(), 1);
        assert_eq!(o.interactions[0].writes, vec![("count".to_string(), "add(s[\"count\"], 1)".to_string())]);
        let c = codes(admit(&src.replace("0 session", "0 durable")));
        assert_eq!(c, vec![611]);
        let c = codes(admit(&src.replace("+ 1", "+ 9007199254740992")));
        assert_eq!(c, vec![616]);
    }

    const SCREENS: &str = r#"@ontology app {
  @cell root { id: "root";
    @cell home { id: "home"; position_p: (0, 0, 0); extent_p: (1, 1, 0); present-when: state.screen is 0;
      @button open { id: "open"; position_p: (0.1, 0.1, 0); extent_p: (0.2, 0.2, 0); text: "Open"; }
    }
    @cell viewer { id: "viewer"; position_p: (0, 0, 0); extent_p: (1, 1, 0); present-when: state.screen is 1;
      @button back { id: "back"; position_p: (0.1, 0.1, 0); extent_p: (0.2, 0.2, 0); text: "Back"; }
      @button next { id: "next"; position_p: (0.5, 0.1, 0); extent_p: (0.2, 0.2, 0); text: "Next"; }
    }
  }
  @state { screen: int = 0 session; }
  @interaction show { id: "action.show"; trigger: button.open.activate; class: D1; state { write screen: int; } input { } set screen = 1; }
  @interaction hide { id: "action.hide"; trigger: button.back.activate; class: D1; state { write screen: int; } input { } set screen = 0; }
}"#;

    #[test]
    fn screens_get_one_focus_graph_per_configuration() {
        // "open" overlaps "back" geometrically, but they are never present together.
        let o = admit(SCREENS).unwrap_or_else(|d| panic!("{d:?}"));
        assert_eq!(o.configurations.len(), 3, "screen = 0, screen = 1, any other value");
        let by = |present: Vec<u32>| o.configurations.iter().find(|c| c.present == present).unwrap_or_else(|| panic!("{present:?}"));
        assert_eq!(by(vec![0]).initial, Some(0));
        assert_eq!(by(vec![1, 2]).initial, Some(1));
        assert_eq!(by(vec![]).when, "screen = -1");
        assert_eq!(o.configurations[o.initial_configuration].present, vec![0]);
        assert_eq!(o.initial, Some(0));
    }

    #[test]
    fn checks_run_inside_each_configuration() {
        // In screen 1 the two buttons now overlap: refused, naming the configuration.
        let src = SCREENS.replace("position_p: (0.5, 0.1, 0)", "position_p: (0.2, 0.1, 0)");
        let Err(d) = admit(&src) else { panic!("overlap must be refused") };
        assert!(d.iter().any(|d| d.code == Code::ActionableOverlap && d.message.contains("screen = 1")), "{d:?}");
        // A diagonal button in screen 1 is unreachable there.
        let src = SCREENS.replace("position_p: (0.5, 0.1, 0)", "position_p: (0.5, 0.5, 0)");
        let Err(d) = admit(&src) else { panic!("unreachable must be refused") };
        assert!(d.iter().any(|d| d.code == Code::FocusUnreachable && d.message.contains("next")), "{d:?}");
    }

    const AMBIENT: &str = r#"@ontology app {
  @cell root { id: "root"; fill: #f4f1ea; fill-to: #e9eef1; fill-period-s: 45;
    @cell thread { id: "thread"; position_p: (0.2, 0.5, 0); extent_p: (0.5, 0.02, 0); fill: #e6dccb; drift-p: (0.1, 0.1, 0); drift-period-s: 90; }
    @cell stage { id: "stage"; position_p: (0, 0, 0); extent_p: (1, 1, 0); drift-p: (0.004, 0.006, 0); drift-period-s: 240;
      @cell title { id: "title"; position_p: (0.05, 0.05, 0); extent_p: (0.6, 0.1, 0); text: "Ambient"; text-size-p: 0.6; }
      @button a { id: "a"; position_p: (0.05, 0.7, 0); extent_p: (0.2, 0.15, 0); text: "A"; fill: #d9e2ec; }
      @button b { id: "b"; position_p: (0.3, 0.7, 0); extent_p: (0.2, 0.15, 0); text: "B"; fill: #d9e2ec; }
    }
  }
}"#;

    #[test]
    fn ambient_motion_is_admitted_and_lowered() {
        let o = admit(AMBIENT).unwrap_or_else(|d| panic!("{d:?}"));
        let stage = o.authored_ids.iter().position(|id| id.as_deref() == Some("stage")).unwrap();
        let d = o.motion.drift[stage].expect("stage drifts");
        assert_eq!((d.ax, d.ay, d.period_ms, d.decorative), (7_680, 6_480, 240_000, false));
        let thread = o.authored_ids.iter().position(|id| id.as_deref() == Some("thread")).unwrap();
        assert!(o.motion.drift[thread].unwrap().decorative);
        assert!(o.motion.flow[0].is_some(), "the root breathes");
        assert_eq!(o.motion.subtree_end[stage], o.frame.nodes.len());
        let html = crate::emit::index_html(&o, &crate::appinfo::AppInfo { id: "com.example.a".into(), title: "A".into(), version: "1.0.0".into(), vendor: None }, None);
        assert!(html.contains(r#"<div class="m" style="--ax:7.68px;--ay:6.48px;animation-duration:240000ms">"#), "{html}");
        assert!(html.contains(r#"<div class="m dec""#), "{html}");
        assert!(html.contains(r#"class="g" style="background:linear-gradient(115deg,#e9eef1,#f4f1ea);animation-duration:45000ms""#), "{html}");
        assert_eq!(html.matches("<div class=\"m").count(), 2);
        // The thread's half-axes are fractions of its parent (the root), not of itself.
        let t = o.motion.drift[thread].unwrap();
        assert_eq!((t.ax, t.ay), (192_000, 108_000));
    }

    #[test]
    fn appear_offsets_are_parent_relative_and_bounded() {
        let src = AMBIENT.replace(
            r#"@cell title { id: "title";"#,
            r#"@cell title { id: "title"; appear-s: 0.6; appear-from-p: (0, 0.05, 0);"#,
        );
        let o = admit(&src).unwrap_or_else(|d| panic!("{d:?}"));
        let title = o.authored_ids.iter().position(|id| id.as_deref() == Some("title")).unwrap();
        // 0.05 of the stage's 1080 pt height.
        assert_eq!(o.motion.appear[title], Some(crate::motion::Appear { dx: 0, dy: 54_000, ms: 600 }));
        let html = crate::emit::index_html(&o, &crate::appinfo::AppInfo { id: "com.example.a".into(), title: "A".into(), version: "1.0.0".into(), vendor: None }, None);
        assert!(html.contains(&format!(r#"<div class="w a" data-appear="{title}" style="--dx:0px;--dy:54px;animation-duration:600ms">"#)), "{html}");
        // Sliding in from far above would leave the limit.
        let src = src.replace("appear-from-p: (0, 0.05, 0)", "appear-from-p: (0, -0.2, 0)");
        assert!(codes(admit(&src)).contains(&621));
    }

    #[test]
    fn a_drift_must_carry_all_buttons_or_none() {
        let src = AMBIENT.replace(
            r#"@button a { id: "a";"#,
            r#"@button a { id: "a"; drift-p: (0.05, 0.05, 0); drift-period-s: 30;"#,
        );
        assert!(codes(admit(&src)).contains(&633));
    }

    #[test]
    fn drawn_nodes_stay_inside_the_limit_while_drifting() {
        let src = AMBIENT.replace("position_p: (0.05, 0.05, 0); extent_p: (0.6, 0.1, 0)", "position_p: (0.0, 0.0, 0); extent_p: (0.6, 0.1, 0)");
        assert!(codes(admit(&src)).contains(&621));
        // The undrawn stage itself crosses the edge: allowed.
        assert!(admit(AMBIENT).is_ok());
    }

    #[test]
    fn text_keeps_contrast_over_breathing_and_moving_backgrounds() {
        // The root breathes towards a dark blue: the title would vanish.
        let src = AMBIENT.replace("fill-to: #e9eef1", "fill-to: #203040");
        let Err(d) = admit(&src) else { panic!("contrast must be refused") };
        assert!(d.iter().any(|d| d.code == Code::TextContrast && d.message.contains("title")), "{d:?}");
        // A dark decorative thread drifting under the title is refused too.
        let src = AMBIENT.replace("position_p: (0.2, 0.5, 0); extent_p: (0.5, 0.02, 0); fill: #e6dccb", "position_p: (0.05, 0.08, 0); extent_p: (0.5, 0.02, 0); fill: #303030");
        assert!(codes(admit(&src)).contains(&622));
    }

    #[test]
    fn at_least_rows_and_dedupe() {
        // Three rows shown by count; nine combinations collapse to four screens.
        let src = r#"@ontology app {
  @cell root { id: "root";
    @button r1 { id: "r1"; position_p: (0.1, 0.1, 0); extent_p: (0.3, 0.1, 0); present-when: state.rows at-least 1; }
    @button r2 { id: "r2"; position_p: (0.1, 0.3, 0); extent_p: (0.3, 0.1, 0); present-when: state.rows at-least 2; }
    @button r3 { id: "r3"; position_p: (0.1, 0.5, 0); extent_p: (0.3, 0.1, 0); present-when: state.rows at-least 3; }
    @cell label { id: "label"; position_p: (0.5, 0.1, 0); extent_p: (0.3, 0.1, 0); text-from: state.rows; }
  }
  @state { rows: int = 2 session; }
  @interaction more { id: "action.more"; trigger: button.r1.activate; class: D1; state { read rows: int; write rows: int; } input { } set rows = state.rows + 1; }
}"#;
        let o = admit(src).unwrap_or_else(|d| panic!("{d:?}"));
        let mut sets: Vec<Vec<u32>> = o.configurations.iter().map(|c| c.present.clone()).collect();
        sets.sort();
        assert_eq!(sets, vec![vec![], vec![0], vec![0, 1], vec![0, 1, 2]]);
        assert_eq!(o.configurations[o.initial_configuration].present, vec![0, 1]);
        let js = crate::emit::app_js(&o);
        assert!(js.contains(r#"["rows","ge",2]"#), "{js}");
    }

    #[test]
    fn webos_platform_exceptions_require_implemented_routes() {
        let source = r#"@ontology demo {
            @persistence { cache_targets: null; }
            @cell root { id: "root"; text: "Demo"; }
            @platform webos { "media.playback": only-use("html.video"); }
        }"#;
        let parsed = admit(source).unwrap();
        assert_eq!(parsed.parsed.declarations.platform_exceptions.len(), 1);
        let unknown = source.replace("html.video", "unknown.player");
        let errors = admit(&unknown).unwrap_err();
        assert_eq!(errors[0].code, Code::SourceRejected);
        assert!(errors[0].message.contains("GY-E1016"));
        assert!(errors[0].range.is_some());
        let other = source.replace("@platform webos", "@platform android_tv").replace("html.video", "unknown.player");
        assert!(admit(&other).is_ok());
    }
}
