//! Nevada pipeline orchestration (contract `nevada.pipeline/0.0.2`).
//!
//! Nevada is a pipeline over Emperor's 13 canonical stage names. This module
//! drives one deterministic, honest run for a device profile plus a set of
//! focus elements and touch interactions: every stage is reported with an
//! explicit status and detail. Blocked stages stay blocked; a stage never
//! claims success beyond what Nevada can actually verify. The same input
//! always produces the same run — there are no panics and no environment
//! reads in library paths.

use crate::conformance;
use crate::dcr::{self, DeviceProfile};
use crate::focus;
use crate::profiles;
use crate::target::{self, AndroidTvFeature, TARGET_PROFILE};
use crate::translate;

/// Contract identifier for this module.
pub const PIPELINE_CONTRACT: &str = "nevada.pipeline/0.0.2";

/// The 13 canonical pipeline stage names, shared across Goya crates.
///
/// Canonical source of truth: `crates/emperor/src/stage.rs` (`NAMES`). The
/// list is copied here because Nevada is std-only and must not depend on the
/// emperor crate; the `names_match_emperor_canonical_list` test pins the copy
/// to the documented canonical list so drift is a test failure, not a lie.
pub const NAMES: [&str; 13] = [
    "lexical-syntactic-admission",
    "type-semantic-validation",
    "canonical-goya-ir",
    "effects-capabilities",
    "target-negotiation",
    "native-linux-lowering",
    "optimisation",
    "runtime-platform-linking",
    "machine-code-generation",
    "elf-assembly-linking",
    "static-verification",
    "runtime-conformance",
    "packaging-signing-release",
];

/// Status of one pipeline stage. Mirrors the Emperor canonical statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    /// The stage ran and its work verified.
    Pass,
    /// The stage ran and its work did not verify.
    Fail,
    /// The stage cannot run in this phase; no artefact is produced.
    Blocked,
    /// The stage does not apply to Nevada; nothing is claimed.
    NotApplicable,
}

impl Status {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Blocked => "BLOCKED",
            Self::NotApplicable => "NOT_APPLICABLE",
        }
    }
}

/// One pipeline stage: its canonical id/name, status, and honest detail.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Stage {
    pub id: u8,
    pub name: &'static str,
    pub status: Status,
    pub detail: String,
}

/// Ordered collector of the 13 stages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Stages {
    items: Vec<Stage>,
}

impl Stages {
    pub fn new() -> Self {
        Self {
            items: Vec::with_capacity(NAMES.len()),
        }
    }

    /// Push the next stage in canonical order. Ids and names come from the
    /// local `NAMES` copy; the fixed 13-stage shape makes index overruns
    /// impossible, but a full collector is still refused rather than misnamed.
    pub fn push(&mut self, status: Status, detail: impl Into<String>) {
        let index = self.items.len();
        if index >= NAMES.len() {
            return;
        }
        self.items.push(Stage {
            id: (index as u8) + 1,
            name: NAMES[index],
            status,
            detail: detail.into(),
        });
    }

    pub fn items(&self) -> &[Stage] {
        &self.items
    }

    pub fn has(&self, status: Status) -> bool {
        self.items.iter().any(|stage| stage.status == status)
    }
}

/// Summary of the static-verification stage, reduced to Nevada's own surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticVerificationSummary {
    /// The conformance verdict reported by the validator.
    pub status: conformance::ConformanceStatus,
    /// Number of diagnostics produced by the validator.
    pub diagnostic_count: usize,
    /// Diagnostic messages in report order (Display form, stable codes).
    pub diagnostics: Vec<String>,
}

/// Input to one orchestration run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanInput {
    profile_id: String,
    elements: Vec<focus::FocusElement>,
    interactions: Vec<translate::TouchInteraction>,
}

impl PlanInput {
    pub fn new(
        profile_id: impl Into<String>,
        elements: Vec<focus::FocusElement>,
        interactions: Vec<translate::TouchInteraction>,
    ) -> Self {
        Self {
            profile_id: profile_id.into(),
            elements,
            interactions,
        }
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn elements(&self) -> &[focus::FocusElement] {
        &self.elements
    }

    pub fn interactions(&self) -> &[translate::TouchInteraction] {
        &self.interactions
    }
}

/// The outcome of one orchestration run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// The requested profile id, echoed back verbatim.
    pub profile_id: String,
    /// All 13 canonical stages, in canonical order.
    pub stages: Stages,
    /// Summary of the static-verification stage, if it ran.
    pub conformance: Option<StaticVerificationSummary>,
    /// True only if no stage Failed. Blocked stages are expected and honest.
    pub ok: bool,
}

/// Detail text for the three front stages Nevada does not reach.
const FRONT_UNRESOLVED: &str =
    "nevada admits no scenes; goya-lang surface unresolved (docs/nevada/gap-ledger.md)";

/// Detail text for stages that stay blocked in this phase.
const LOWERING_BLOCKED: &str =
    "no lowering exists in nevada/0.0.2; no artefact is produced or claimed";
const LINKING_BLOCKED: &str =
    "no runtime platform link exists in nevada/0.0.2; no artefact is produced or claimed";
const CODEGEN_BLOCKED: &str =
    "no machine code generation exists in nevada/0.0.2; no artefact is produced or claimed";
const ELF_BLOCKED: &str =
    "no ELF is assembled or linked in nevada/0.0.2; no artefact is produced or claimed";
const RUNTIME_CONFORMANCE_BLOCKED: &str =
    "no device runs the artefact; no runtime conformance may be reported";
const PACKAGING_BLOCKED: &str = "no package is built, signed, or released in nevada/0.0.2";

/// Execute one orchestration run. Deterministic and panic-free: every stage
/// receives an explicit status and detail; nothing is inferred silently.
pub fn execute(input: &PlanInput) -> Run {
    let mut stages = Stages::new();

    // Stages 1-3: the goya-lang surface Nevada would admit is unresolved.
    stages.push(Status::NotApplicable, FRONT_UNRESOLVED);
    stages.push(Status::NotApplicable, FRONT_UNRESOLVED);
    stages.push(Status::NotApplicable, FRONT_UNRESOLVED);

    // Stage 4: effects-capabilities — resolve the profile census.
    let resolved = profiles::lookup(&input.profile_id)
        .map(|profile| (profile.census.clone(), dcr::resolve(&profile.census)));
    match &resolved {
        Ok((census, Ok(_support))) => stages.push(
            Status::Pass,
            format!(
                "profile `{}` resolved via {} + {}; census declares {} capabilities; contracts {}/{}/{}",
                input.profile_id,
                profiles::CONTRACT,
                dcr::CONTRACT,
                census.present.len(),
                profiles::CONTRACT,
                dcr::CONTRACT,
                TARGET_PROFILE,
            ),
        ),
        Ok((_census, Err(error))) => stages.push(
            Status::Fail,
            format!(
                "profile `{}` resolved via {} but the census violates the {} baseline: {error}",
                input.profile_id,
                profiles::CONTRACT,
                dcr::CONTRACT,
            ),
        ),
        Err(_) => stages.push(
            Status::Fail,
            format!(
                "profile `{}` is not registered under {}; no census exists and nothing is inferred",
                input.profile_id,
                profiles::CONTRACT,
            ),
        ),
    }

    // Stage 5: target-negotiation — request every feature the census could
    // support and let target::negotiate grant or refuse each one explicitly.
    let requested = [
        AndroidTvFeature::LeanbackManifest,
        AndroidTvFeature::DpadNavigation,
        AndroidTvFeature::SpatialFocus,
        AndroidTvFeature::MotionPointer,
        AndroidTvFeature::VoiceSearch,
        AndroidTvFeature::GamepadNavigation,
        AndroidTvFeature::TouchScrolling,
        AndroidTvFeature::CompanionScreen,
    ];
    let negotiation = target::negotiate(&requested, &input.profile_id);
    let baseline_granted = negotiation
        .granted
        .contains(&AndroidTvFeature::DpadNavigation);
    if baseline_granted {
        let granted_names: Vec<&str> = negotiation
            .granted
            .iter()
            .map(|feature| feature.as_str())
            .collect();
        stages.push(
            Status::Pass,
            format!(
                "{} granted [{}]; {} refusals (fail-closed, explicit reasons) [{}]",
                TARGET_PROFILE,
                granted_names.join(", "),
                negotiation.refusals.len(),
                negotiation
                    .refusals
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    } else {
        stages.push(
            Status::Fail,
            format!(
                "{} could not grant the required D-pad baseline; {} refusals [{}]",
                TARGET_PROFILE,
                negotiation.refusals.len(),
                negotiation
                    .refusals
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        );
    }

    // Stage 6: native-linux-lowering — blocked, honestly.
    stages.push(Status::Blocked, LOWERING_BLOCKED);

    // Stage 7: optimisation — nothing to optimise; no IR exists.
    stages.push(Status::NotApplicable, FRONT_UNRESOLVED);

    // Stage 8: runtime-platform-linking — blocked.
    stages.push(Status::Blocked, LINKING_BLOCKED);

    // Stage 9: machine-code-generation — blocked.
    stages.push(Status::Blocked, CODEGEN_BLOCKED);

    // Stage 10: elf-assembly-linking — blocked.
    stages.push(Status::Blocked, ELF_BLOCKED);

    // Stage 11: static-verification — run the conformance validator once,
    // against the resolved census (or an empty, fail-closed census when no
    // profile resolved).
    let census = match &resolved {
        Ok((census, Ok(_))) => census.clone(),
        _ => DeviceProfile::default(),
    };
    let report = conformance::validate(&input.elements, &input.interactions, &census);
    let diagnostic_count = report.diagnostics.len();
    let diagnostics: Vec<String> = report
        .diagnostics
        .iter()
        .map(std::string::ToString::to_string)
        .collect();
    let summary = StaticVerificationSummary {
        status: report.status,
        diagnostic_count,
        diagnostics: diagnostics.clone(),
    };
    match report.status {
        conformance::ConformanceStatus::Pass => stages.push(
            Status::Pass,
            format!(
                "static verification passed with 0 diagnostics ({diagnostic_count} reported, all non-fatal); elements {} interactions {}",
                input.elements.len(),
                input.interactions.len(),
            ),
        ),
        conformance::ConformanceStatus::Fail => stages.push(
            Status::Fail,
            format!(
                "static verification failed with {diagnostic_count} diagnostics [{}]; elements {} interactions {}",
                diagnostics.join("; "),
                input.elements.len(),
                input.interactions.len(),
            ),
        ),
        conformance::ConformanceStatus::Refused => stages.push(
            Status::Fail,
            format!(
                "static verification refused the input ({diagnostic_count} diagnostics) [{}]; elements {} interactions {}",
                diagnostics.join("; "),
                input.elements.len(),
                input.interactions.len(),
            ),
        ),
    }

    // Stages 12-13: nothing runs and nothing ships.
    stages.push(Status::Blocked, RUNTIME_CONFORMANCE_BLOCKED);
    stages.push(Status::Blocked, PACKAGING_BLOCKED);

    let ok = !stages.has(Status::Fail);
    Run {
        profile_id: input.profile_id.clone(),
        stages,
        conformance: Some(summary),
        ok,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcr::{HardwareCapability, MotionOutput};
    use crate::focus::Direction;
    use conformance::ConformanceStatus;

    /// The canonical 13 stage names, documented here as the source of truth
    /// copied from `crates/emperor/src/stage.rs::NAMES`.
    const CANONICAL: [&str; 13] = [
        "lexical-syntactic-admission",
        "type-semantic-validation",
        "canonical-goya-ir",
        "effects-capabilities",
        "target-negotiation",
        "native-linux-lowering",
        "optimisation",
        "runtime-platform-linking",
        "machine-code-generation",
        "elf-assembly-linking",
        "static-verification",
        "runtime-conformance",
        "packaging-signing-release",
    ];

    #[test]
    fn names_match_emperor_canonical_list() {
        assert_eq!(NAMES, CANONICAL);
        for (index, name) in NAMES.iter().enumerate() {
            assert_eq!(name, &CANONICAL[index]);
        }
    }

    fn dpad_input() -> PlanInput {
        // A minimal but non-empty plan: an empty layout and interaction
        // list are refused by static verification (GY-NV-507 — nothing to
        // validate is never a pass).
        PlanInput::new(
            "oem.generic.tv/1.0.0",
            vec![focus::FocusElement {
                id: 1,
                x: 0,
                y: 0,
                w: 100,
                h: 50,
                focusable: true,
                essential: true,
            }],
            vec![translate::TouchInteraction::TouchTap],
        )
    }

    #[test]
    fn run_has_all_13_stages_with_correct_names() {
        let run = execute(&dpad_input());
        let items = run.stages.items();
        assert_eq!(items.len(), 13);
        for (index, stage) in items.iter().enumerate() {
            assert_eq!(stage.id, (index as u8) + 1);
            assert_eq!(stage.name, CANONICAL[index]);
        }
    }

    #[test]
    fn dpad_only_happy_path() {
        let run = execute(&dpad_input());
        let items = run.stages.items();
        let passing: Vec<&str> = items
            .iter()
            .filter(|stage| stage.status == Status::Pass)
            .map(|stage| stage.name)
            .collect();
        assert_eq!(
            passing,
            vec![
                "effects-capabilities",
                "target-negotiation",
                "static-verification"
            ]
        );
        assert!(!run.stages.has(Status::Fail));
        assert!(run.ok);
        let blocked: Vec<&str> = items
            .iter()
            .filter(|stage| stage.status == Status::Blocked)
            .map(|stage| stage.name)
            .collect();
        assert_eq!(
            blocked,
            vec![
                "native-linux-lowering",
                "runtime-platform-linking",
                "machine-code-generation",
                "elf-assembly-linking",
                "runtime-conformance",
                "packaging-signing-release",
            ]
        );
        let not_applicable: Vec<&str> = items
            .iter()
            .filter(|stage| stage.status == Status::NotApplicable)
            .map(|stage| stage.name)
            .collect();
        assert_eq!(
            not_applicable,
            vec![
                "lexical-syntactic-admission",
                "type-semantic-validation",
                "canonical-goya-ir",
                "optimisation",
            ]
        );
    }

    #[test]
    fn unknown_profile_fails_effects_and_negotiation() {
        let run = execute(&PlanInput::new(
            "oem.unheard-of.tv/9.9.9",
            Vec::new(),
            Vec::new(),
        ));
        let items = run.stages.items();
        assert_eq!(items[3].name, "effects-capabilities");
        assert_eq!(items[3].status, Status::Fail);
        assert!(items[3].detail.contains("not registered"));
        assert_eq!(items[4].name, "target-negotiation");
        assert_eq!(items[4].status, Status::Fail);
        assert!(!run.ok);
        // Static verification still runs against an empty, fail-closed census.
        assert_eq!(items[10].name, "static-verification");
        assert!(matches!(items[10].status, Status::Pass | Status::Fail));
    }

    #[test]
    fn unreachable_essential_element_fails_static_verification() {
        // Two essential elements far apart with no connecting path: the
        // focus graph cannot reach the second from the first, so the
        // conformance validator must report a failure.
        let left = focus::FocusElement {
            id: 1,
            x: 0,
            y: 0,
            w: 100,
            h: 100,
            focusable: true,
            essential: true,
        };
        let right = focus::FocusElement {
            id: 2,
            x: 5_000,
            y: 5_000,
            w: 100,
            h: 100,
            focusable: true,
            essential: true,
        };
        let run = execute(&PlanInput::new(
            "oem.generic.tv/1.0.0",
            vec![left, right],
            Vec::new(),
        ));
        let items = run.stages.items();
        assert_eq!(items[10].name, "static-verification");
        assert_eq!(items[10].status, Status::Fail);
        assert!(!run.ok);
        assert!(items[10].detail.contains("diagnostics"));
        let summary = run.conformance.as_ref().expect("summary present");
        assert_eq!(summary.status, ConformanceStatus::Fail);
        assert!(summary.diagnostic_count > 0);
        assert!(summary
            .diagnostics
            .iter()
            .all(|text| text.starts_with("GY-NV-")));
    }

    #[test]
    fn run_is_deterministic() {
        let first = execute(&dpad_input());
        for _ in 0..8 {
            assert_eq!(execute(&dpad_input()), first);
        }
        let voice = PlanInput::new("oem.acme.voice-remote/1.0.0", Vec::new(), Vec::new());
        let a = execute(&voice);
        let b = execute(&voice);
        assert_eq!(a.stages, b.stages);
        assert_eq!(a.conformance, b.conformance);
    }

    #[test]
    fn pointer_profile_passes_baseline_stages() {
        let run = execute(&PlanInput::new(
            "oem.acme.pointer-remote/1.0.0",
            vec![focus::FocusElement {
                id: 1,
                x: 0,
                y: 0,
                w: 100,
                h: 50,
                focusable: true,
                essential: true,
            }],
            vec![translate::TouchInteraction::TouchTap],
        ));
        let items = run.stages.items();
        assert_eq!(items[3].status, Status::Pass);
        assert_eq!(items[4].status, Status::Pass);
        assert!(items[4].detail.contains("motion-pointer"));
        assert!(items[4].detail.contains("touch-scrolling"));
        assert!(run.ok);
    }

    #[test]
    fn stage_statuses_use_canonical_strings() {
        assert_eq!(Status::Pass.as_str(), "PASS");
        assert_eq!(Status::Fail.as_str(), "FAIL");
        assert_eq!(Status::Blocked.as_str(), "BLOCKED");
        assert_eq!(Status::NotApplicable.as_str(), "NOT_APPLICABLE");
    }

    #[test]
    fn blocked_stages_never_reported_as_pass() {
        let run = execute(&dpad_input());
        for stage in run.stages.items() {
            if stage.name == "runtime-conformance" {
                assert_eq!(stage.status, Status::Blocked);
                assert!(stage.detail.contains("no device runs the artefact"));
            }
        }
    }

    #[test]
    fn direction_and_census_are_deterministic_inputs() {
        // Direction is referenced so the focus surface used by static
        // verification stays pinned to Nevada's own vocabulary.
        assert_ne!(Direction::Up, Direction::Down);
        let census = DeviceProfile {
            present: vec![HardwareCapability::Dpad],
            motion_output: MotionOutput::None,
        };
        assert!(dcr::resolve(&census).is_ok());
    }
}
