//! Q1 capability resolution: intent fallbacks and device support over the
//! Nevada television infrastructure (proposed, not ratified).
//!
//! This module consumes `goya_nevada` as-is — [`goya_nevada::profiles`] for
//! the OEM census, [`goya_nevada::dcr`] for capability resolution, and
//! [`goya_nevada::focus`] for spatial reachability. It does **not** modify
//! Nevada and does not claim the shared scene→`FocusElement` lowering
//! (Nevada gap N0-FOCUSGRAPH): the minimal lowering here is Quitasol-local,
//! and Nevada's phase-1 shared lowering supersedes it when it exists.
//!
//! Resolution is fail-closed and deterministic. The recorded device support
//! says what the *hardware* can express; it never claims that Alexa (or any
//! voice platform) integration exists — the manifest lowering tier stays
//! `"unresolved"` per the Q0 capability audit.

use crate::IntentManifest;
use goya_lang::intents::IntentDiagnostic;
use goya_lang::{layout, Scene, SourceRange};
use goya_nevada::dcr::{self, InteractionSupport, ResolveError};
use goya_nevada::focus::{unreachable_essential, FocusElement};
use goya_nevada::profiles;

/// Diagnostic code space for Quitasol resolution. Nevada's ledger reserves
/// `NV-…` codes for Nevada (gap N0-DIAGNOSTICS); Quitasol uses `QS-…`.
pub const QS_E2000: &str = "QS-E2000"; // unknown or unresolvable OEM profile
pub const QS_E2001: &str = "QS-E2001"; // fallback target names no scene element
pub const QS_E2002: &str = "QS-E2002"; // remote fallback unreachable by D-pad focus
pub const QS_E2003: &str = "QS-E2003"; // census violates the Nevada baseline

/// Nevada module contracts consumed by this module, recorded for evidence.
pub const NEVADA_PROFILES_CONTRACT: &str = "nevada.profiles/0.0.1";
pub const NEVADA_DCR_CONTRACT: &str = "nevada.dcr/0.0.1";
pub const NEVADA_FOCUS_CONTRACT: &str = "nevada.focus/0.0.1";

/// Resolved, fail-closed device support projected from Nevada's
/// [`InteractionSupport`]. These are hardware expressions only — not voice
/// platform integration claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSupport {
    /// The full profile id the support was resolved from, `family/version`.
    pub profile_id: String,
    /// D-pad style directional focus traversal (`spatial_navigation`).
    pub dpad: bool,
    /// Voice dictation and commands via the device census microphone.
    pub voice_input: bool,
    /// Physical keyboard / mouse interaction.
    pub keyboard: bool,
    /// Spatial pointer (air mouse) cursor control.
    pub spatial_pointer: bool,
    /// Analogue stick navigation from a game controller.
    pub gamepad: bool,
}

impl DeviceSupport {
    fn from_support(profile_id: String, support: InteractionSupport) -> Self {
        Self {
            profile_id,
            dpad: support.spatial_navigation,
            voice_input: support.voice_input,
            keyboard: support.keyboard_mouse,
            spatial_pointer: support.spatial_pointer,
            gamepad: support.analogue_navigation,
        }
    }
}

/// One resolved fallback target of one intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFallback {
    pub name: String,
    /// Remote fallbacks: whether the target is reachable by deterministic
    /// D-pad focus from Nevada's initial focus. Keyboard fallbacks are
    /// existence-checked only (`reachable` is always `true`); no keyboard
    /// navigation model is claimed.
    pub reachable: bool,
}

/// Per-intent fallback resolution, sorted by intent name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FallbackResolution {
    pub intent: String,
    pub remote: Option<ResolvedFallback>,
    pub keyboard: Option<ResolvedFallback>,
}

/// The result of Q1 resolution: the phase-0 manifest plus the resolved
/// device support and per-intent fallback reachability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedManifest {
    manifest: IntentManifest,
    device: DeviceSupport,
    fallbacks: Vec<FallbackResolution>,
}

impl ResolvedManifest {
    pub fn manifest(&self) -> &IntentManifest {
        &self.manifest
    }

    pub fn device(&self) -> &DeviceSupport {
        &self.device
    }

    /// Per-intent fallback resolutions, sorted by intent name (deterministic).
    pub fn fallbacks(&self) -> &[FallbackResolution] {
        &self.fallbacks
    }

    /// Deterministic JSON: the manifest shape with `"device"` inserted after
    /// `"lowering_tier"` and each intent entry gaining
    /// `"fallback_resolution"` before its closing brace. Byte-identical for
    /// equal values.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"contract\":");
        crate::push_json_string(&mut out, self.manifest.contract());
        out.push_str(",\"profile\":");
        crate::push_json_string(&mut out, self.manifest.profile());
        out.push_str(",\"utterance_grammar\":");
        crate::push_json_string(&mut out, self.manifest.utterance_grammar());
        out.push_str(",\"lowering_tier\":");
        crate::push_json_string(&mut out, self.manifest.lowering_tier());
        out.push_str(",\"device\":{\"profile\":");
        crate::push_json_string(&mut out, &self.device.profile_id);
        out.push_str(",\"dpad\":");
        out.push_str(bool_str(self.device.dpad));
        out.push_str(",\"voice_input\":");
        out.push_str(bool_str(self.device.voice_input));
        out.push_str(",\"keyboard\":");
        out.push_str(bool_str(self.device.keyboard));
        out.push_str(",\"spatial_pointer\":");
        out.push_str(bool_str(self.device.spatial_pointer));
        out.push_str(",\"gamepad\":");
        out.push_str(bool_str(self.device.gamepad));
        out.push_str("},\"intents\":[");
        for (i, entry) in self.manifest.entries().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let fallback = self
                .fallbacks
                .iter()
                .find(|fallback| fallback.intent == entry.name)
                .expect("fallback resolution exists for every manifest entry");
            crate::push_entry_json(&mut out, entry, Some(fallback));
        }
        out.push_str("]}");
        out
    }
}

fn bool_str(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

/// Quitasol-minimal scene→focus-element lowering: lay out the scene with the
/// deterministic `goya_lang::layout` and expose every button and input as a
/// focusable [`FocusElement`] (id = layout index, millipoint rect). Plain
/// cells are not focusable. This is not the shared N0-FOCUSGRAPH lowering:
/// no viewport policy, scroll-container semantics or essential-element
/// derivation beyond the caller's markers is claimed.
pub fn scene_focus_elements(
    frame_nodes: &[goya_lang::DrawNode],
) -> Vec<FocusElement> {
    frame_nodes
        .iter()
        .enumerate()
        .map(|(index, node)| FocusElement {
            id: index as u32,
            x: node.rect.x,
            y: node.rect.y,
            w: node.rect.width,
            h: node.rect.height,
            focusable: node.is_button || node.is_input,
            essential: false,
        })
        .collect()
}

/// Resolve device support for an OEM profile id (fail-closed):
/// `nevada::profiles::lookup` must find the exact id and
/// `nevada::dcr::resolve` must accept the census. Unknown or malformed id →
/// `QS-E2000`; a census violating the Nevada D-pad baseline → `QS-E2003`.
pub fn device_support(profile_id: &str) -> Result<DeviceSupport, IntentDiagnostic> {
    let profile = profiles::lookup(profile_id).map_err(|error| IntentDiagnostic {
        code: QS_E2000,
        name: format!("unknown OEM profile: {error}"),
        range: SourceRange { start: 0, end: 0 },
    })?;
    let support = dcr::resolve(&profile.census).map_err(|error: ResolveError| IntentDiagnostic {
        code: QS_E2003,
        name: format!(
            "profile {} census fails the Nevada baseline: {error}",
            profile.id()
        ),
        range: SourceRange { start: 0, end: 0 },
    })?;
    Ok(DeviceSupport::from_support(profile.id(), support))
}

/// Resolve the phase-0 manifest against a device profile and a laid-out
/// scene: compile first (fail-closed, `GY-E1011`), then per intent require
/// both fallback targets to name an existing scene element (`QS-E2001`) and
/// every remote fallback to be D-pad-reachable from Nevada's deterministic
/// initial focus (`QS-E2002`). Keyboard fallbacks are existence-checked
/// only. Deterministic on all inputs: layout, focus reachability and the
/// Nevada profile registry are all pure functions.
pub fn resolve_manifest(
    scene: &Scene,
    profile_id: &str,
    width_pt: f64,
    height_pt: f64,
) -> Result<ResolvedManifest, IntentDiagnostic> {
    let manifest = crate::compile(scene)?;
    let device = device_support(profile_id)?;

    let frame = layout(scene, width_pt, height_pt);
    let mut elements = scene_focus_elements(&frame.nodes);
    // Remote fallback targets are essential: the deterministic remote tier is
    // the only tier the Q0 audit guarantees to every app, so it must be
    // reachable by focus. All layout nodes sharing the target's name are
    // marked (duplicate names are legal in scenes).
    let remote_targets: Vec<&String> = scene
        .intents
        .iter()
        .filter_map(|intent| intent.fallback_remote.as_ref())
        .collect();
    for element in elements.iter_mut() {
        let node = &frame.nodes[element.id as usize];
        if remote_targets.iter().any(|target| **target == node.name) {
            element.essential = true;
        }
    }
    let unreachable = unreachable_essential(&elements, None);

    let mut fallbacks = Vec::new();
    for intent in &scene.intents {
        let mut resolve_target = |target: &Option<String>, reachability: bool| {
            let Some(target) = target else {
                return Ok(None);
            };
            let indexes: Vec<u32> = frame
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| &node.name == target)
                .map(|(index, _)| index as u32)
                .collect();
            if indexes.is_empty() {
                return Err(IntentDiagnostic {
                    code: QS_E2001,
                    name: format!(
                        "intent {}: fallback target {target} names no scene element",
                        intent.name
                    ),
                    range: intent.range,
                });
            }
            let reachable = !reachability || !indexes.iter().any(|id| unreachable.contains(id));
            Ok(Some(ResolvedFallback {
                name: target.clone(),
                reachable,
            }))
        };
        let remote = resolve_target(&intent.fallback_remote, true)?;
        let keyboard = resolve_target(&intent.fallback_keyboard, false)?;
        if remote.as_ref().is_some_and(|remote| !remote.reachable) {
            return Err(IntentDiagnostic {
                code: QS_E2002,
                name: format!(
                    "intent {}: remote fallback {} is unreachable by D-pad focus",
                    intent.name, remote.as_ref().unwrap().name
                ),
                range: intent.range,
            });
        }
        fallbacks.push(FallbackResolution {
            intent: intent.name.clone(),
            remote,
            keyboard,
        });
    }
    // Deterministic output regardless of declaration order.
    fallbacks.sort_by(|a, b| a.intent.cmp(&b.intent));
    Ok(ResolvedManifest {
        manifest,
        device,
        fallbacks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_lang::parse;
    use goya_nevada::profiles::registered_ids;

    const SCENE: &str = "\
@ontology app {
    @button go { position: (100, 100, 0) extent: (200, 60, 0) }
    @button back { position: (400, 100, 0) extent: (200, 60, 0) }
    @input field { position: (100, 300, 0) extent: (300, 40, 0) }
    @cell panel { position: (100, 400, 0) extent: (300, 40, 0) }
    @interaction act {
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @intent main {
        action: act;
        voice { utterance: \"Do it\"; }
        fallback { remote: back; keyboard: field; }
    }
}
";

    #[test]
    fn scene_lowering_marks_only_buttons_and_inputs_focusable() {
        let scene = parse(SCENE).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let elements = scene_focus_elements(&frame.nodes);
        // Index 0 is the ontology wrapper cell (four children, so it does not
        // collapse); go, back, field follow, then the plain panel.
        let focusable: Vec<bool> = elements.iter().map(|e| e.focusable).collect();
        assert_eq!(focusable, vec![false, true, true, true, false]);
        // Millipoint geometry flows through unchanged.
        assert_eq!(elements[0].x, 100_000);
        assert_eq!(elements[0].w, 200_000);
        assert!(!elements[3].focusable);
    }

    #[test]
    fn device_support_projects_the_census_fail_closed() {
        let tv = device_support("oem.generic.tv/1.0.0").unwrap();
        assert!(tv.dpad);
        assert!(!tv.voice_input);
        assert!(!tv.keyboard);
        assert_eq!(tv.profile_id, "oem.generic.tv/1.0.0");

        let voice = device_support("oem.acme.voice-remote/1.0.0").unwrap();
        assert!(voice.voice_input, "microphone census flips voice_input");
        assert!(voice.dpad);

        let unknown = device_support("oem.nowhere/9.9.9").unwrap_err();
        assert_eq!(unknown.code, QS_E2000);
        assert!(unknown.name.contains("unknown OEM profile"));

        let malformed = device_support("no-slash").unwrap_err();
        assert_eq!(malformed.code, QS_E2000);

        // Nevada's built-in profiles are the only supported ids.
        assert_eq!(registered_ids().len(), 4);
    }

    #[test]
    fn reachable_fallbacks_resolve_with_device_and_deterministic_json() {
        let scene = parse(SCENE).unwrap();
        let resolved =
            resolve_manifest(&scene, "oem.acme.voice-remote/1.0.0", 800.0, 600.0)
                .expect("fallbacks exist and are reachable");
        assert!(resolved.device().voice_input);
        let fallbacks = resolved.fallbacks();
        assert_eq!(fallbacks.len(), 1);
        assert_eq!(fallbacks[0].intent, "main");
        let remote = fallbacks[0].remote.as_ref().unwrap();
        assert_eq!(remote.name, "back");
        assert!(remote.reachable, "back is a button next to go");
        let keyboard = fallbacks[0].keyboard.as_ref().unwrap();
        assert_eq!(keyboard.name, "field");
        assert!(keyboard.reachable, "keyboard fallbacks are existence-only");

        // Deterministic byte-identical JSON.
        let again = resolve_manifest(
            &parse(SCENE).unwrap(),
            "oem.acme.voice-remote/1.0.0",
            800.0,
            600.0,
        )
        .unwrap();
        assert_eq!(resolved.to_json(), again.to_json());
        let json = resolved.to_json();
        assert!(
            json.contains(
                "\"device\":{\"profile\":\"oem.acme.voice-remote/1.0.0\",\"dpad\":true,\"voice_input\":true"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "\"fallback_resolution\":{\"remote\":{\"name\":\"back\",\"reachable\":true},\"keyboard\":{\"name\":\"field\",\"reachable\":true}}"
            ),
            "{json}"
        );
    }

    #[test]
    fn unreachable_remote_fallback_is_rejected_fail_closed() {
        // The fallback names a plain cell: it exists but is not focusable,
        // so D-pad focus can never reach it.
        let mutant = SCENE.replacen("remote: back;", "remote: panel;", 1);
        let scene = parse(&mutant).unwrap();
        let error =
            resolve_manifest(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap_err();
        assert_eq!(error.code, QS_E2002);
        assert!(error.name.contains("unreachable by D-pad focus"), "{error}");
        assert!(mutant[error.range.start..error.range.end].starts_with("@intent main"));
    }

    #[test]
    fn unknown_fallback_target_is_rejected_fail_closed() {
        let mutant = SCENE.replacen("remote: back;", "remote: nowhere;", 1);
        let scene = parse(&mutant).unwrap();
        let error =
            resolve_manifest(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap_err();
        assert_eq!(error.code, QS_E2001);
        assert!(error.name.contains("names no scene element"), "{error}");
    }

    #[test]
    fn compile_errors_precede_resolution_errors() {
        let mutant = SCENE.replacen("action: act;", "action: missing;", 1);
        let scene = parse(&mutant).unwrap();
        let error =
            resolve_manifest(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap_err();
        assert_eq!(error.code, "GY-E1011");
        assert!(error.name.contains("does not reference a declared interaction"));
    }

    #[test]
    fn zero_intents_resolve_with_device_section_present() {
        let scene = parse(
            "@ontology app { @button go { position: (0, 0, 0) } @interaction a { trigger: button.go.activate; class: D1; state { } input { } } }",
        )
        .unwrap();
        let resolved = resolve_manifest(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap();
        assert!(resolved.fallbacks().is_empty());
        assert!(resolved.manifest().entries().is_empty());
        assert!(resolved
            .to_json()
            .contains("\"device\":{\"profile\":\"oem.generic.tv/1.0.0\""));
    }

    #[test]
    fn resolution_is_a_pure_function_of_scene_and_profile() {
        let a = resolve_manifest(&parse(SCENE).unwrap(), "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap();
        let b = resolve_manifest(&parse(SCENE).unwrap(), "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_json(), b.to_json());
    }
}
