//! Lower an admitted GSSH/3 scene to Nevada's fixed Android TV viewport.
//!
//! The numeric focus IDs are Aragon's depth-first cell indexes. Buttons
//! with D1 activation and editable fields are essential focus targets.
//! Every such target must have a nonempty rectangle in the reference layout.

use std::collections::BTreeMap;
use std::fmt;

use goya_lang::handoff::{admit_v3, Origin};
use goya_lang::{CellKind, InteractionClass, SourceRange, StrictParseError};
use goya_nevada::{focus::FocusElement, orchestrate::PlanInput, translate::TouchInteraction};

/// Logical TV viewport in dp, uniformly fitted to the available display area.
pub const TV_VIEWPORT: (u32, u32) = (960, 540);

/// A source-bound interaction that must have a remote activation path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NevadaInteraction {
    pub element_id: u32,
    /// Authored action ID for D1 activation, or authored element ID for input.
    pub authored_id: String,
    pub kind: TouchInteraction,
    pub range: SourceRange,
}

/// A Nevada conformance input and its correspondence with the authored scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NevadaScene {
    pub source_digest: [u8; 32],
    /// Authored cell IDs in Aragon's depth-first order. An implicit root has
    /// an empty ID and is never focusable.
    pub cell_ids: Vec<String>,
    /// Geometry for every cell, in millipoints at [`TV_VIEWPORT`].
    pub elements: Vec<FocusElement>,
    /// One entry per essential control, in depth-first cell order.
    pub interactions: Vec<NevadaInteraction>,
}

impl NevadaScene {
    /// Provide the pipeline with the same geometry and interaction vocabulary.
    pub fn plan_input(&self, profile_id: impl Into<String>) -> PlanInput {
        PlanInput::new(
            profile_id,
            self.elements.clone(),
            self.interactions.iter().map(|entry| entry.kind).collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NevadaSceneReason {
    Admission(StrictParseError),
    Unsupported(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NevadaSceneError {
    pub reason: NevadaSceneReason,
    pub origin: Option<Origin>,
    pub range: SourceRange,
}

impl fmt::Display for NevadaSceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}: ", self.range.start, self.range.end)?;
        match &self.reason {
            NevadaSceneReason::Admission(error) => write!(f, "{error}"),
            NevadaSceneReason::Unsupported(what) => write!(f, "Nevada does not support {what}"),
        }
    }
}

impl std::error::Error for NevadaSceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.reason {
            NevadaSceneReason::Admission(error) => Some(error),
            NevadaSceneReason::Unsupported(_) => None,
        }
    }
}

/// Strictly admit a scene and lower the interactions supported by the TV runtime.
pub fn lower_nevada_scene(source: &str) -> Result<NevadaScene, NevadaSceneError> {
    let handoff = admit_v3(source).map_err(|error| NevadaSceneError {
        range: error.range,
        origin: None,
        reason: NevadaSceneReason::Admission(error),
    })?;
    let parsed = handoff.parsed();
    let scene = &parsed.scene;
    let range = |origin: &Origin| {
        handoff.origin(origin).unwrap_or(SourceRange {
            start: 0,
            end: source.len(),
        })
    };
    let refuse = |origin: Origin, what: &'static str| NevadaSceneError {
        range: range(&origin),
        origin: Some(origin),
        reason: NevadaSceneReason::Unsupported(what),
    };

    let mut actions = BTreeMap::new();
    for action in &scene.interactions {
        let id = action.authored_id.as_deref().unwrap_or_default();
        if action.class != InteractionClass::D1 {
            return Err(refuse(
                Origin::Interaction(id.to_owned()),
                "D2 interactions",
            ));
        }
        actions.insert(action.trigger_button.as_str(), id);
    }

    let mut cells = Vec::new();
    let mut pending = vec![&scene.root];
    while let Some(cell) = pending.pop() {
        let id = cell.authored_id.as_deref().unwrap_or_default();
        if cells.len() == crate::android_scene::ANDROID_SCENE_MAX_CELLS {
            return Err(refuse(
                Origin::Element(id.to_owned()),
                "scenes beyond Aragon's element limit",
            ));
        }
        for (unsupported, property, detail) in [
            (cell.scroll, "scroll", "scroll viewports on the D-pad path"),
            (
                cell.gestures.is_some(),
                "gestures",
                "clipboard gesture policies",
            ),
        ] {
            if unsupported {
                return Err(refuse(
                    Origin::Property {
                        element: id.to_owned(),
                        property: property.to_owned(),
                    },
                    detail,
                ));
            }
        }
        cells.push(cell);
        pending.extend(cell.children.iter().rev());
    }

    let (width, height) = TV_VIEWPORT;
    let frame = goya_lang::layout(scene, f64::from(width), f64::from(height));
    let mut result = NevadaScene {
        source_digest: parsed.source_digest,
        cell_ids: Vec::with_capacity(cells.len()),
        elements: Vec::with_capacity(cells.len()),
        interactions: Vec::new(),
    };
    for (index, (cell, node)) in cells.into_iter().zip(&frame.nodes).enumerate() {
        let id = cell.authored_id.as_deref().unwrap_or_default();
        let element = Origin::Element(id.to_owned());
        let interaction = match cell.kind {
            CellKind::Button => {
                let Some(&action) = actions.get(cell.name.as_str()) else {
                    return Err(refuse(element, "buttons without a D1 activation"));
                };
                Some((
                    action,
                    TouchInteraction::TouchTap,
                    Origin::Interaction(action.to_owned()),
                ))
            }
            CellKind::Input => Some((id, TouchInteraction::TextInput, element.clone())),
            CellKind::Cell => None,
        };
        let rect = node.rect;
        let element_id = index as u32;
        let focusable = interaction.is_some();
        if let Some((authored_id, kind, origin)) = interaction {
            if rect.width == 0 || rect.height == 0 {
                return Err(refuse(
                    element,
                    "controls with no visible area in the TV viewport",
                ));
            }
            result.interactions.push(NevadaInteraction {
                element_id,
                authored_id: authored_id.to_owned(),
                kind,
                range: range(&origin),
            });
        }
        result.cell_ids.push(id.to_owned());
        result.elements.push(FocusElement {
            id: element_id,
            x: rect.x,
            y: rect.y,
            w: rect.width,
            h: rect.height,
            focusable,
            essential: focusable,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_nevada::focus::{next_focus, unreachable_essential, Direction};

    const COUNTER: &str = r#"@ontology counter {
        @state { count: int = 0 session; }
        @cell root { id: "window.root";
            @cell panel { id: "window.panel";
                position_p: (0.1, 0.2, 0); extent_p: (0.8, 0.6, 0);
                @input amount { id: "window.amount"; extent_p: (0.4, 0.3, 0); }
                @button add { id: "window.add"; position_p: (0.5, 0, 0);
                    extent_p: (0.4, 0.3, 0); text: "Add"; }
            }
        }
        @interaction add { id: "action.add"; trigger: button.add.activate; class: D1;
            state { read count: int; write count: int; }
            input { amount: int from "window.amount"; }
            set count = state.count + input.amount;
        }
    }"#;

    #[test]
    fn produces_aragon_aligned_geometry_and_source_bound_interactions() {
        let lowered = lower_nevada_scene(COUNTER).unwrap();
        let android = crate::compile_android_scene(COUNTER).unwrap();
        assert_eq!(lowered.cell_ids, android.cell_ids);
        assert_eq!(lowered.source_digest, android.source_map.source_digest);
        assert_eq!(lowered, lower_nevada_scene(COUNTER).unwrap());
        assert_eq!(
            lowered.elements[2],
            FocusElement {
                id: 2,
                x: 96_000,
                y: 108_000,
                w: 307_200,
                h: 97_200,
                focusable: true,
                essential: true,
            }
        );
        assert!(!lowered.elements[1].focusable);
        assert_eq!(
            next_focus(&lowered.elements, None, Direction::Right),
            Some(2)
        );
        assert_eq!(
            next_focus(&lowered.elements, Some(2), Direction::Right),
            Some(3)
        );
        assert!(unreachable_essential(&lowered.elements, None).is_empty());

        let [field, action] = lowered.interactions.as_slice() else {
            panic!("expected one field and one action");
        };
        assert_eq!(field.element_id, 2);
        assert_eq!(field.kind, TouchInteraction::TextInput);
        assert_eq!(field.authored_id, "window.amount");
        assert!(COUNTER[field.range.start..field.range.end].starts_with("@input amount"));
        assert_eq!(action.element_id, 3);
        assert_eq!(action.kind, TouchInteraction::TouchTap);
        assert_eq!(action.authored_id, "action.add");
        assert!(COUNTER[action.range.start..action.range.end].starts_with("@interaction add"));
        assert_eq!(
            lowered.plan_input("oem.generic.tv/1.0.0").interactions(),
            [TouchInteraction::TextInput, TouchInteraction::TouchTap]
        );
    }

    #[test]
    fn keeps_an_off_axis_control_essential_for_the_gate() {
        let source = COUNTER.replace("(0.5, 0, 0)", "(0.5, 0.5, 0)");
        let scene = lower_nevada_scene(&source).unwrap();
        assert_eq!(unreachable_essential(&scene.elements, None), [3]);
    }

    #[test]
    fn refuses_scroll_unbound_buttons_and_collapsed_controls_at_their_source() {
        let scroll = COUNTER.replace(
            "id: \"window.panel\";",
            "id: \"window.panel\"; scroll: true;",
        );
        let collapse = COUNTER.replace("(0.4, 0.3, 0)", "(0, 0.3, 0)");
        for (source, expected_range) in [
            (&scroll[..], "scroll: true"),
            (&collapse[..], "@input amount"),
            (
                r#"@ontology app { @button go { id: "go"; } }"#,
                "@button go",
            ),
        ] {
            let error = lower_nevada_scene(source).unwrap_err();
            assert!(matches!(error.reason, NevadaSceneReason::Unsupported(_)));
            assert!(source[error.range.start..error.range.end].starts_with(expected_range));
        }
    }

    #[test]
    fn refuses_supported_language_constructs_without_tv_runtime_handlers() {
        let clipboard = r#"@ontology app {
            @cell root { id: "root"; gestures { copy: allow; } }
        }"#;
        let d2 = r#"@ontology app {
            @state { count: int = 0 session; }
            @button fetch { id: "button.fetch"; }
            @interaction fetch { id: "action.fetch"; trigger: button.fetch.activate; class: D2;
                effects { network; }
                requests { lookup: effect network capability "network" request "static" result int64; }
                state { write count: int; }
                input { }
                set count = effect.lookup;
            }
        }"#;
        for (source, expected_range) in [(clipboard, "gestures"), (d2, "@interaction fetch")] {
            let error = lower_nevada_scene(source).unwrap_err();
            assert!(
                matches!(error.reason, NevadaSceneReason::Unsupported(_)),
                "{error}"
            );
            assert!(source[error.range.start..error.range.end].starts_with(expected_range));
        }
    }

    #[test]
    fn preserves_strict_grammar_refusals() {
        let source = r#"@ontology app { @cell root { id: "root"; position: (0, 0, 0); } }"#;
        let error = lower_nevada_scene(source).unwrap_err();
        let NevadaSceneReason::Admission(inner) = error.reason else {
            panic!("expected strict admission failure");
        };
        assert_eq!(inner.code, "GY-E1009");
        assert_eq!(error.range, inner.range);
    }
}
