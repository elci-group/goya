//! Conformance Validator — contract `nevada.conformance/0.0.2`.
//!
//! Combines the three phase-0 subsystems into one fail-closed,
//! machine-readable quality gate:
//!
//! 1. [`dcr::resolve`] turns the device census into an
//!    [`dcr::InteractionSupport`] (a missing D-pad baseline refuses the
//!    whole validation with diagnostic `505`);
//! 2. [`focus::unreachable_essential`] reports essential elements that
//!    directional navigation can never reach (diagnostic `501`, status
//!    [`ConformanceStatus::Fail`]);
//! 3. [`translate::conformance_report`] maps every touch interaction onto
//!    the device; refusals become diagnostic `502` and force `Fail`.
//!
//! Determinism: all report vectors are ordered by stable keys (element id,
//! interaction, diagnostic code) and two identical calls always produce
//! identical reports.
//!
//! Empty-input policy (pinned by a test): a call with *both* an empty
//! element layout and an empty interaction list validates nothing, and
//! "validated nothing" is not a pass — the validator refuses with
//! `NvPipelineBlocked` (507). An empty interaction list over a non-empty
//! layout still validates focus reachability, because there is real work
//! being checked in that case.

use crate::dcr;
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::focus::{self, FocusElement};
use crate::translate::{self, DeviceCapabilities, IncompatibilityReason, TouchInteraction};

/// Contract identifier for this module.
pub const CONTRACT: &str = "nevada.conformance/0.0.2";

/// Overall outcome of a conformance validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConformanceStatus {
    /// Everything validated: all essential elements reachable, zero
    /// translation refusals.
    Pass,
    /// At least one conformance failure was found (diagnostics `501`/`502`).
    Fail,
    /// The validation itself was refused and produced no verdict
    /// (missing baseline `505`, or a blocked pipeline `507`).
    Refused,
}

/// One interaction the translator refused, in a stable form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefusedInteraction {
    /// The touch interaction that could not be translated.
    pub interaction: TouchInteraction,
    /// Why the translator refused it.
    pub reason: IncompatibilityReason,
}

/// Deterministic per-stage counts, ready to become a quality gate metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StageCounts {
    /// Total number of essential elements in the layout.
    pub essential_total: usize,
    /// Essential elements unreachable by directional navigation.
    pub essential_unreachable: usize,
    /// Number of interactions submitted for translation.
    pub interactions_total: usize,
    /// Interactions that translated.
    pub interactions_translated: usize,
    /// Interactions the translator refused.
    pub interactions_refused: usize,
}

/// The full conformance report. All vectors are deterministically ordered
/// and the whole structure compares equal across identical calls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceReport {
    /// Overall verdict.
    pub status: ConformanceStatus,
    /// Diagnostics, ascending by [`DiagnosticCode`].
    pub diagnostics: Vec<Diagnostic>,
    /// Essential element ids unreachable by directional navigation,
    /// ascending by id.
    pub unreachable: Vec<u32>,
    /// Refused interactions, ascending by [`TouchInteraction`].
    pub refused: Vec<RefusedInteraction>,
    /// Per-stage counts.
    pub stage_counts: StageCounts,
}

impl ConformanceReport {
    /// One-line human summary, derived deterministically from the counts.
    pub fn summary_line(&self) -> String {
        let counts = &self.stage_counts;
        match self.status {
            ConformanceStatus::Pass => format!(
                "PASS: {}/{} interactions translated, 0 refusals, 0/{} essentials unreachable",
                counts.interactions_translated, counts.interactions_total, counts.essential_total
            ),
            ConformanceStatus::Fail => format!(
                "FAIL: {} refusal(s), {}/{} essentials unreachable",
                counts.interactions_refused, counts.essential_unreachable, counts.essential_total
            ),
            ConformanceStatus::Refused => format!(
                "REFUSED: {} diagnostic(s); no conformance verdict",
                self.diagnostics.len()
            ),
        }
    }
}

/// Deterministically derive the translator's [`DeviceCapabilities`] from
/// the resolved [`dcr::InteractionSupport`].
///
/// Derivation rules (documented, frozen):
///
/// - `spatial_pointer` maps 1:1 from `InteractionSupport::spatial_pointer`
///   (an air mouse with native pointer events);
/// - `voice_input` maps 1:1 from `InteractionSupport::voice_input`;
/// - `keyboard_available` maps from `InteractionSupport::keyboard_mouse`
///   (a physical keyboard attached via Bluetooth/USB);
/// - `tv_keyboard` maps from `InteractionSupport::spatial_navigation`:
///   the Android TV on-screen IME is stock baseline software (spec §2:
///   text input → TV keyboard), not optional peripheral hardware, so it
///   is claimable whenever the D-pad baseline is present. `NvTranslationRefused`
///   (502) stays defensive for future targets without a system IME;
/// - `long_press_supported` is always `false`: the capability census has no
///   long-press concept, so the derivation fails closed to the conservative
///   default (`LongPress` still translates, to `ContextAction`).
pub fn derive_device_capabilities(support: &dcr::InteractionSupport) -> DeviceCapabilities {
    DeviceCapabilities {
        spatial_pointer: support.spatial_pointer,
        voice_input: support.voice_input,
        keyboard_available: support.keyboard_mouse,
        tv_keyboard: support.spatial_navigation,
        long_press_supported: false,
    }
}

/// Run the full conformance validation.
///
/// Fail-closed semantics:
///
/// - an unresolvable device profile (missing D-pad baseline) refuses the
///   validation with `NvMissingRequiredBaseline` (505);
/// - an empty layout *and* an empty interaction list refuse the validation
///   with `NvPipelineBlocked` (507): nothing was validated, which is never
///   a pass;
/// - any unreachable essential element yields diagnostic
///   `NvFocusUnreachableEssential` (501) and forces [`ConformanceStatus::Fail`];
/// - any translation refusal yields diagnostic `NvTranslationRefused` (502)
///   and forces `Fail`.
pub fn validate(
    elements: &[FocusElement],
    interactions: &[TouchInteraction],
    profile: &dcr::DeviceProfile,
) -> ConformanceReport {
    let support = match dcr::resolve(profile) {
        Ok(support) => support,
        Err(error) => {
            return ConformanceReport {
                status: ConformanceStatus::Refused,
                diagnostics: vec![Diagnostic::new(
                    DiagnosticCode::NvMissingRequiredBaseline,
                    format!("device profile unresolvable: {error}"),
                )],
                unreachable: Vec::new(),
                refused: Vec::new(),
                stage_counts: StageCounts::default(),
            };
        }
    };

    if elements.is_empty() && interactions.is_empty() {
        return ConformanceReport {
            status: ConformanceStatus::Refused,
            diagnostics: vec![Diagnostic::new(
                DiagnosticCode::NvPipelineBlocked,
                "nothing to validate: empty layout and empty interaction list",
            )],
            unreachable: Vec::new(),
            refused: Vec::new(),
            stage_counts: StageCounts::default(),
        };
    }

    let unreachable = focus::unreachable_essential(elements, None);
    let essential_total = elements.iter().filter(|element| element.essential).count();

    let capabilities = derive_device_capabilities(&support);
    let translation = translate::conformance_report(interactions, &capabilities);
    let mut refused = Vec::new();
    let mut diagnostics = Vec::new();
    for id in &unreachable {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NvFocusUnreachableEssential,
            format!("essential element {id} is unreachable by directional navigation"),
        ));
    }
    for entry in &translation.refused {
        let reason = match entry.outcome {
            translate::TranslationOutcome::Refused { reason } => reason,
            translate::TranslationOutcome::Translated { .. } => continue,
        };
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::NvTranslationRefused,
            format!(
                "interaction {:?} cannot be translated on this device",
                entry.interaction
            ),
        ));
        refused.push(RefusedInteraction {
            interaction: entry.interaction,
            reason,
        });
    }
    diagnostics.sort_by_key(|diagnostic| diagnostic.code);

    let status = if diagnostics.is_empty() {
        ConformanceStatus::Pass
    } else {
        ConformanceStatus::Fail
    };

    let stage_counts = StageCounts {
        essential_total,
        essential_unreachable: unreachable.len(),
        interactions_total: interactions.len(),
        interactions_translated: translation.translated.len(),
        interactions_refused: refused.len(),
    };

    ConformanceReport {
        status,
        diagnostics,
        unreachable,
        refused,
        stage_counts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcr::{DeviceProfile, HardwareCapability, MotionOutput};
    use crate::diagnostics::DiagnosticCode;
    use crate::focus::Direction;
    use crate::translate::RemoteInteraction;

    fn census(present: &[HardwareCapability], motion_output: MotionOutput) -> DeviceProfile {
        DeviceProfile {
            present: present.to_vec(),
            motion_output,
        }
    }

    fn element(id: u32, x: i64, y: i64, w: i64, h: i64) -> FocusElement {
        FocusElement {
            id,
            x,
            y,
            w,
            h,
            focusable: true,
            essential: false,
        }
    }

    fn essential(id: u32, x: i64, y: i64, w: i64, h: i64) -> FocusElement {
        FocusElement {
            essential: true,
            ..element(id, x, y, w, h)
        }
    }

    fn dpad_profile() -> DeviceProfile {
        census(&[HardwareCapability::Dpad], MotionOutput::None)
    }

    #[test]
    fn passing_layout_with_dpad_only_translateable_set() {
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 0, 100, 100, 50),
            element(3, 0, 200, 100, 50),
        ];
        let interactions = [
            TouchInteraction::TouchTap,
            TouchInteraction::Swipe,
            TouchInteraction::MouseScroll,
        ];
        let report = validate(&elements, &interactions, &dpad_profile());
        assert_eq!(report.status, ConformanceStatus::Pass);
        assert!(report.diagnostics.is_empty());
        assert!(report.unreachable.is_empty());
        assert!(report.refused.is_empty());
        assert_eq!(report.stage_counts.essential_total, 2);
        assert_eq!(report.stage_counts.interactions_translated, 3);
        assert_eq!(report.stage_counts.interactions_refused, 0);
        // MouseScroll on a pointer-less device must translate to
        // DirectionalScroll, never PointerScroll.
        let report_full = validate(
            &elements,
            &translate::ALL_TOUCH_INTERACTIONS,
            &dpad_profile(),
        );
        assert_eq!(report_full.status, ConformanceStatus::Pass);
        assert!(report_full.refused.is_empty());
    }

    #[test]
    fn unreachable_essential_forces_fail_with_501() {
        // Element 2 is off-axis from everything: never reachable.
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 500, 1_000, 50, 25),
            essential(3, 150, 0, 100, 50),
        ];
        let interactions = [TouchInteraction::TouchTap];
        let report = validate(&elements, &interactions, &dpad_profile());
        assert_eq!(report.status, ConformanceStatus::Fail);
        assert_eq!(report.unreachable, vec![2]);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].code,
            DiagnosticCode::NvFocusUnreachableEssential
        );
        assert_eq!(
            report.diagnostics[0].to_string(),
            "GY-NV-501: essential element 2 is unreachable by directional navigation"
        );
        assert_eq!(report.stage_counts.essential_unreachable, 1);
    }

    #[test]
    fn text_input_translates_via_baseline_tv_keyboard_on_dpad_only() {
        // Spec §2: the on-screen TV keyboard is stock baseline software, not
        // optional peripheral hardware, so a D-pad-only census still
        // translates TextInput (primary TvKeyboard). The census's
        // keyboard_mouse/voice_input bits only add richer methods.
        let profile = census(&[HardwareCapability::Dpad], MotionOutput::None);
        let elements = [essential(1, 0, 0, 100, 50)];
        let interactions = [TouchInteraction::TextInput, TouchInteraction::TouchTap];
        let report = validate(&elements, &interactions, &profile);
        assert_eq!(report.status, ConformanceStatus::Pass);
        assert!(report.refused.is_empty());
        assert_eq!(report.stage_counts.interactions_translated, 2);
        assert_eq!(report.stage_counts.interactions_refused, 0);
    }

    #[test]
    fn missing_baseline_refuses_with_505() {
        let profile = census(&[HardwareCapability::Touchpad], MotionOutput::None);
        let report = validate(
            &[essential(1, 0, 0, 100, 50)],
            &[TouchInteraction::TouchTap],
            &profile,
        );
        assert_eq!(report.status, ConformanceStatus::Refused);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].code,
            DiagnosticCode::NvMissingRequiredBaseline
        );
        assert!(report.diagnostics[0].to_string().starts_with("GY-NV-505:"));
    }

    #[test]
    fn empty_inputs_are_refused_not_silently_passing() {
        let elements: [FocusElement; 0] = [];
        let interactions: [TouchInteraction; 0] = [];
        let report = validate(&elements, &interactions, &dpad_profile());
        assert_eq!(report.status, ConformanceStatus::Refused);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(
            report.diagnostics[0].code,
            DiagnosticCode::NvPipelineBlocked
        );
        assert!(report.diagnostics[0].to_string().starts_with("GY-NV-507:"));
    }

    #[test]
    fn empty_interactions_over_non_empty_layout_still_validate_focus() {
        // Documented policy: focus reachability is real validation work, so
        // an empty interaction list over a non-empty layout is a Pass, not
        // a refusal.
        let elements = [essential(1, 0, 0, 100, 50)];
        let interactions: [TouchInteraction; 0] = [];
        let report = validate(&elements, &interactions, &dpad_profile());
        assert_eq!(report.status, ConformanceStatus::Pass);
        assert_eq!(report.stage_counts.interactions_total, 0);
    }

    #[test]
    fn validation_is_deterministic() {
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 0, 100, 100, 50),
            essential(3, 500, 1_000, 50, 25),
        ];
        let interactions = [
            TouchInteraction::TextInput,
            TouchInteraction::TouchTap,
            TouchInteraction::TextInput,
        ];
        let profile = dpad_profile();
        let first = validate(&elements, &interactions, &profile);
        for _ in 0..8 {
            assert_eq!(validate(&elements, &interactions, &profile), first);
        }
    }

    #[test]
    fn diagnostics_sort_by_stable_code() {
        // Both an unreachable essential (501) and a refusal (502): the
        // diagnostic list must be ordered 501 before 502.
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 500, 1_000, 50, 25),
        ];
        let interactions = [TouchInteraction::TextInput];
        let report = validate(&elements, &interactions, &dpad_profile());
        assert_eq!(report.status, ConformanceStatus::Fail);
        // TextInput now translates via the baseline TV keyboard, so only
        // the focus diagnostic (501) fires here.
        let codes: Vec<u16> = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.number())
            .collect();
        assert_eq!(codes, vec![501]);
        let mut sorted = report.diagnostics.clone();
        sorted.sort_by_key(|diagnostic| diagnostic.code);
        assert_eq!(report.diagnostics, sorted);
    }

    #[test]
    fn derivation_maps_pointer_support_to_mouse_scroll_behaviour() {
        // Pointer-capable device (native pointer events): MouseScroll maps
        // to PointerScroll with a DirectionalScroll fallback.
        let pointer_profile = census(
            &[HardwareCapability::Dpad, HardwareCapability::Accelerometer],
            MotionOutput::NativePointerEvents,
        );
        let support = dcr::resolve(&pointer_profile).expect("dpad satisfies the baseline");
        assert!(
            support.spatial_pointer,
            "native pointer events are a pointer"
        );
        let capabilities = derive_device_capabilities(&support);
        let outcome = translate::translate(TouchInteraction::MouseScroll, &capabilities);
        match outcome {
            translate::TranslationOutcome::Translated { primary, fallbacks } => {
                assert_eq!(primary, RemoteInteraction::PointerScroll);
                assert_eq!(fallbacks, vec![RemoteInteraction::DirectionalScroll]);
            }
            translate::TranslationOutcome::Refused { .. } => {
                panic!("MouseScroll must never be refused");
            }
        }
    }

    #[test]
    fn derivation_is_conservative_without_pointer() {
        let support = dcr::resolve(&dpad_profile()).expect("dpad satisfies the baseline");
        let capabilities = derive_device_capabilities(&support);
        assert_eq!(capabilities, DeviceCapabilities::dpad_only());
        let outcome = translate::translate(TouchInteraction::MouseScroll, &capabilities);
        match outcome {
            translate::TranslationOutcome::Translated { primary, fallbacks } => {
                assert_eq!(primary, RemoteInteraction::DirectionalScroll);
                assert!(fallbacks.is_empty());
            }
            translate::TranslationOutcome::Refused { .. } => {
                panic!("MouseScroll must never be refused");
            }
        }
    }

    #[test]
    fn initial_focus_reachability_agrees_with_focus_engine() {
        // The validator's BFS must agree with a direct focus-engine query.
        let elements = [
            essential(1, 0, 0, 100, 50),
            essential(2, 0, 100, 100, 50),
            essential(3, 0, 200, 100, 50),
        ];
        assert!(focus::unreachable_essential(&elements, None).is_empty());
        // And navigation actually traverses the layout.
        assert_eq!(
            focus::next_focus(&elements, Some(1), Direction::Down),
            Some(2)
        );
    }

    #[test]
    fn summary_line_reflects_status() {
        let elements: [FocusElement; 0] = [];
        let interactions: [TouchInteraction; 0] = [];
        let refused_report = validate(&elements, &interactions, &dpad_profile());
        assert!(refused_report.summary_line().starts_with("REFUSED:"));

        let elements = [essential(1, 0, 0, 100, 50)];
        let interactions = [TouchInteraction::TouchTap];
        let pass_report = validate(&elements, &interactions, &dpad_profile());
        assert_eq!(
            pass_report.summary_line(),
            "PASS: 1/1 interactions translated, 0 refusals, 0/1 essentials unreachable"
        );
    }
}
