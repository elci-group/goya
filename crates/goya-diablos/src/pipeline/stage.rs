//! Pipeline stage definitions and status tracking.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Status of a compilation stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// Stage completed successfully.
    Pass,
    /// Stage failed.
    Fail,
    /// Stage is blocked (e.g., missing display, cross-compilation).
    Blocked,
    /// Stage is not applicable for this command/profile.
    NotApplicable,
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Pass => write!(f, "PASS"),
            Status::Fail => write!(f, "FAIL"),
            Status::Blocked => write!(f, "BLOCKED"),
            Status::NotApplicable => write!(f, "NOT_APPLICABLE"),
        }
    }
}

/// Result of a single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    /// Stage identifier (1-12 for DIA-01 through DIA-12).
    pub id: u8,
    /// Stage name.
    pub name: String,
    /// Stage status.
    pub status: Status,
    /// Human-readable detail.
    pub detail: String,
}

impl StageResult {
    pub fn pass(id: u8, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            status: Status::Pass,
            detail: String::new(),
        }
    }

    pub fn fail(id: u8, name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            status: Status::Fail,
            detail: detail.into(),
        }
    }

    pub fn blocked(id: u8, name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            status: Status::Blocked,
            detail: detail.into(),
        }
    }

    pub fn not_applicable(id: u8, name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            status: Status::NotApplicable,
            detail: reason.into(),
        }
    }
}

/// The 12 DIA pipeline stages (per GOYA-DIA-TD-001 §5).
pub const DIA_STAGES: &[(&str, &str)] = &[
    ( "DIA-01", "source-ingestion-canonical-validation" ),
    ( "DIA-02", "application-intent-extraction" ),
    ( "DIA-03", "target-capability-resolution" ),
    ( "DIA-04", "bsd-compatible-semantic-lowering" ),
    ( "DIA-05", "variant-specific-transformation" ),
    ( "DIA-06", "graphics-resource-optimisation" ),
    ( "DIA-07", "security-policy-synthesis" ),
    ( "DIA-08", "native-machine-code-generation" ),
    ( "DIA-09", "linking-dependency-verification" ),
    ( "DIA-10", "packaging-deployment-preparation" ),
    ( "DIA-11", "inquisition-correctness-assessment" ),
    ( "DIA-12", "artefact-certification" ),
];

/// Build the base stage list for a compilation report.
pub fn base_stages() -> Vec<StageResult> {
    DIA_STAGES
        .iter()
        .enumerate()
        .map(|(i, (id, name))| StageResult::not_applicable(i as u8 + 1, *name, "not executed"))
        .collect()
}

/// Update a stage in the list by ID.
pub fn update_stage(stages: &mut [StageResult], id: u8, result: StageResult) {
    if let Some(slot) = stages.iter_mut().find(|s| s.id == id) {
        *slot = result;
    }
}

/// Check if all mandatory stages passed.
pub fn all_mandatory_passed(stages: &[StageResult]) -> bool {
    stages.iter().all(|s| matches!(s.status, Status::Pass | Status::NotApplicable))
}

/// Determine overall exit code from stage results.
pub fn exit_code_from_stages(stages: &[StageResult], has_runtime_failure: bool) -> u8 {
    if stages.iter().any(|s| matches!(s.status, Status::Fail)) {
        crate::fault::exit::ADMISSION_FAILED
    } else if has_runtime_failure {
        crate::fault::exit::RUNTIME_FAILED
    } else if stages.iter().any(|s| matches!(s.status, Status::Blocked)) {
        crate::fault::exit::BLOCKED
    } else {
        crate::fault::exit::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_stages_count() {
        assert_eq!(base_stages().len(), 12);
    }

    #[test]
    fn stage_ids_sequential() {
        let stages = base_stages();
        for (i, stage) in stages.iter().enumerate() {
            assert_eq!(stage.id as usize, i + 1);
        }
    }

    #[test]
    fn exit_code_all_pass() {
        let stages = vec![
            StageResult::pass(1, "test"),
            StageResult::pass(2, "test"),
        ];
        assert_eq!(exit_code_from_stages(&stages, false), 0);
    }

    #[test]
    fn exit_code_blocked() {
        let stages = vec![
            StageResult::pass(1, "test"),
            StageResult::blocked(2, "test", "no display"),
        ];
        assert_eq!(exit_code_from_stages(&stages, false), 3);
    }

    #[test]
    fn exit_code_fail() {
        let stages = vec![
            StageResult::pass(1, "test"),
            StageResult::fail(2, "test", "error"),
        ];
        assert_eq!(exit_code_from_stages(&stages, false), 1);
    }

    #[test]
    fn exit_code_runtime_fail() {
        let stages = vec![
            StageResult::pass(1, "test"),
            StageResult::pass(2, "test"),
        ];
        assert_eq!(exit_code_from_stages(&stages, true), 5);
    }
}