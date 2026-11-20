//! Blocking static conformance for the Nevada Android TV emitter.

use std::collections::BTreeSet;
use std::fmt;

use crate::caprichio;
use crate::conformance::{self, ConformanceStatus};
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::focus::{self, Direction, FocusElement, FocusSuccessors, Viewport};
use crate::translate::{self, RemoteInteraction, TouchInteraction, TranslationOutcome};

/// Nevada's static verification gate, at Emperor stage 11.
pub const ID: &str = "GY-NV-GATE-11";
pub const STAGE: &str = "static-verification";

/// A lowered scene control and the authored behavior it must preserve.
#[derive(Debug, Clone, Copy)]
pub struct SceneInteraction<'a> {
    pub element_id: u32,
    pub authored_id: &'a str,
    pub kind: TouchInteraction,
}

/// A single source interaction and the route implemented by the TV runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub element_id: u32,
    pub authored_id: String,
    pub kind: TouchInteraction,
    pub remote: RemoteInteraction,
}

/// Successful static verification. The emitter consumes this exact graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedScene {
    profile_id: String,
    viewport: Viewport,
    initial_focus: u32,
    graph: Vec<FocusSuccessors>,
    routes: Vec<Route>,
}

impl VerifiedScene {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn initial_focus(&self) -> u32 {
        self.initial_focus
    }

    pub fn graph(&self) -> &[FocusSuccessors] {
        &self.graph
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateError(Vec<Diagnostic>);

impl GateError {
    fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self(vec![Diagnostic::new(code, message)])
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.0
    }
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{ID} {STAGE} FAIL")?;
        for diagnostic in &self.0 {
            write!(f, "\n{diagnostic}")?;
        }
        Ok(())
    }
}

impl std::error::Error for GateError {}

/// Prove that each authored interaction has a wired route and that its
/// control is reachable in the exact viewport and graph the runtime uses.
pub fn verify(
    profile_id: &str,
    viewport: Viewport,
    elements: &[FocusElement],
    interactions: &[SceneInteraction<'_>],
) -> Result<VerifiedScene, GateError> {
    use DiagnosticCode::*;

    let factory = caprichio::compile_registered(profile_id)
        .map_err(|error| GateError::new(NvUnknownProfile, error.to_string()))?;
    let mut ids = BTreeSet::new();
    for element in elements {
        if !ids.insert(element.id) {
            return Err(GateError::new(
                NvFeatureRefused,
                format!("duplicate focus control {}", element.id),
            ));
        }
        if !element.focusable
            || !element.essential
            || element.w <= 0
            || element.h <= 0
            || !viewport.contains(element)
        {
            return Err(GateError::new(
                NvFocusUnreachableEssential,
                format!("control {} has no usable focus rectangle", element.id),
            ));
        }
    }
    let mut bound = BTreeSet::new();
    for interaction in interactions {
        if interaction.authored_id.is_empty()
            || !ids.contains(&interaction.element_id)
            || !bound.insert(interaction.element_id)
        {
            return Err(GateError::new(
                NvFeatureRefused,
                format!("invalid authored binding on control {}", interaction.element_id),
            ));
        }
    }
    if bound != ids {
        return Err(GateError::new(
            NvFeatureRefused,
            "each focus control must have one authored binding",
        ));
    }

    let kinds: Vec<_> = interactions.iter().map(|interaction| interaction.kind).collect();
    let report = conformance::validate(elements, &kinds, factory.census());
    if report.status != ConformanceStatus::Pass {
        return Err(GateError(report.diagnostics));
    }

    let initial_focus =
        focus::next_focus_in_viewport(elements, None, Direction::Down, &viewport)
            .ok_or_else(|| GateError::new(NvPipelineBlocked, "no TV focus entry point"))?;
    let capabilities = factory.translation_capabilities();
    let mut routes = Vec::with_capacity(interactions.len());
    for interaction in interactions {
        let implemented = match interaction.kind {
            TouchInteraction::TouchTap => Some(RemoteInteraction::FocusAndSelect),
            TouchInteraction::TextInput => Some(RemoteInteraction::TvKeyboard),
            _ => None,
        };
        let outcome = translate::translate(interaction.kind, &capabilities);
        let wired = implemented.filter(|route| match &outcome {
            TranslationOutcome::Translated { primary, fallbacks } => {
                route == primary || fallbacks.contains(route)
            }
            TranslationOutcome::Refused { .. } => false,
        });
        let remote = wired.ok_or_else(|| {
            GateError::new(
                NvFeatureRefused,
                format!("no wired TV route for {:?} ({})", interaction.kind, interaction.authored_id),
            )
        })?;
        routes.push(Route {
            element_id: interaction.element_id,
            authored_id: interaction.authored_id.to_owned(),
            kind: interaction.kind,
            remote,
        });
    }
    routes.sort_unstable_by_key(|route| route.element_id);
    Ok(VerifiedScene {
        profile_id: profile_id.to_owned(),
        viewport,
        initial_focus,
        graph: focus::focus_graph_in_viewport(elements, Some(&viewport)),
        routes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = "oem.generic.tv/1.0.0";
    const VIEWPORT: Viewport = Viewport { x: 0, y: 0, w: 100, h: 100 };
    const BUTTON: FocusElement = FocusElement {
        id: 1, x: 0, y: 0, w: 10, h: 10, focusable: true, essential: true,
    };
    const TAP: SceneInteraction<'static> = SceneInteraction {
        element_id: 1, authored_id: "action.add", kind: TouchInteraction::TouchTap,
    };

    #[test]
    fn receipt_binds_focus_and_wired_route() {
        let verified = verify(PROFILE, VIEWPORT, &[BUTTON], &[TAP]).unwrap();
        assert_eq!(verified.initial_focus(), 1);
        assert_eq!(verified.graph()[0].id, 1);
        assert_eq!(verified.routes()[0].remote, RemoteInteraction::FocusAndSelect);
        assert_eq!(verified.routes()[0].authored_id, "action.add");
    }

    #[test]
    fn translated_gesture_with_no_runtime_handler_is_refused() {
        let swipe = SceneInteraction { kind: TouchInteraction::Swipe, ..TAP };
        let error = verify(PROFILE, VIEWPORT, &[BUTTON], &[swipe]).unwrap_err();
        assert_eq!(error.diagnostics()[0].code, DiagnosticCode::NvFeatureRefused);
    }

    #[test]
    fn off_axis_and_offscreen_controls_block_emission() {
        let second = SceneInteraction { element_id: 2, authored_id: "action.other", ..TAP };
        for position in [20, 100] {
            let isolated = FocusElement { id: 2, x: position, y: position, ..BUTTON };
            let error = verify(PROFILE, VIEWPORT, &[BUTTON, isolated], &[TAP, second]).unwrap_err();
            assert_eq!(error.diagnostics()[0].code, DiagnosticCode::NvFocusUnreachableEssential);
        }
    }
}
