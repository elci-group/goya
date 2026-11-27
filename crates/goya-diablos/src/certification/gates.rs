//! Acceptance gates for Diablos (per GOYA-DIA-TD-001 §20).

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Acceptance gate definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceGate {
    /// Gate identifier (e.g., "DIA-G01")
    pub id: String,
    /// Human-readable requirement
    pub requirement: String,
    /// Severity: critical, high, conditional
    pub severity: GateSeverity,
    /// Current status
    #[serde(skip)]
    pub status: GateStatus,
    /// Evidence reference
    #[serde(skip)]
    pub evidence: Option<String>,
}

/// Gate severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateSeverity {
    Critical,
    High,
    Conditional,
}

/// Gate evaluation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum GateStatus {
    #[default]
    NotEvaluated,
    Pass,
    Fail,
    Blocked,
}

/// Evaluate all acceptance gates for a target.
pub fn evaluate_gates(
    _target: &str,
    _profile: &str,
    _artefact_path: &str,
) -> Result<Vec<AcceptanceGate>, Fault> {
    let mut gates = dia_gates();

    for gate in &mut gates {
        gate.status = GateStatus::NotEvaluated;
        gate.evidence = None;
    }

    Ok(gates)
}

/// Check if all critical gates pass.
pub fn critical_gates_pass(gates: &[AcceptanceGate]) -> bool {
    gates.iter()
        .filter(|g| g.severity == GateSeverity::Critical)
        .all(|g| g.status == GateStatus::Pass)
}

/// Check if all high gates pass.
pub fn high_gates_pass(gates: &[AcceptanceGate]) -> bool {
    gates.iter()
        .filter(|g| g.severity == GateSeverity::High)
        .all(|g| g.status == GateStatus::Pass)
}

/// Determine if a Diablo can be declared production-ready.
pub fn production_ready(gates: &[AcceptanceGate]) -> bool {
    critical_gates_pass(gates) && high_gates_pass(gates)
}

/// The 15 acceptance gates from GOYA-DIA-TD-001 §20.
pub fn dia_gates() -> Vec<AcceptanceGate> {
    vec![
        AcceptanceGate {
            id: "DIA-G01".to_string(),
            requirement: "Canonical .gscene semantic preservation".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G02".to_string(),
            requirement: "Correct native executable generation".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G03".to_string(),
            requirement: "Successful execution on actual target OS".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G04".to_string(),
            requirement: "ABI and dependency correctness".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G05".to_string(),
            requirement: "Mandatory security policy enforcement".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G06".to_string(),
            requirement: "Deterministic build verification under pinned conditions".to_string(),
            severity: GateSeverity::High,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G07".to_string(),
            requirement: "Graphical correctness".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G08".to_string(),
            requirement: "Resource budget compliance".to_string(),
            severity: GateSeverity::High,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G09".to_string(),
            requirement: "Native package installation and removal".to_string(),
            severity: GateSeverity::High,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G10".to_string(),
            requirement: "Adios lifecycle correctness".to_string(),
            severity: GateSeverity::High,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G11".to_string(),
            requirement: "romántico state compatibility".to_string(),
            severity: GateSeverity::Conditional,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G12".to_string(),
            requirement: "Neural execution feasibility".to_string(),
            severity: GateSeverity::Conditional,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G13".to_string(),
            requirement: "Performance regression assessment".to_string(),
            severity: GateSeverity::High,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G14".to_string(),
            requirement: "Artefact provenance and integrity".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
        AcceptanceGate {
            id: "DIA-G15".to_string(),
            requirement: "Target capability declaration accuracy".to_string(),
            severity: GateSeverity::Critical,
            status: GateStatus::NotEvaluated,
            evidence: None,
        },
    ]
}

/// Certification states (per GOYA-DIA-TD-001 §20.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CertificationState {
    Experimental,
    BuildCapable,
    ExecutionCapable,
    FunctionallyVerified,
    ReleaseCandidate,
    ProductionCertified,
}

/// Map gate results to certification state.
pub fn certification_state(gates: &[AcceptanceGate]) -> CertificationState {
    if production_ready(gates) {
        CertificationState::ProductionCertified
    } else if critical_gates_pass(gates) && high_gates_pass(gates) {
        CertificationState::ReleaseCandidate
    } else if critical_gates_pass(gates) {
        CertificationState::FunctionallyVerified
    } else if gates.iter().any(|g| g.id == "DIA-G02" && g.status == GateStatus::Pass) {
        CertificationState::ExecutionCapable
    } else if gates.iter().any(|g| g.id == "DIA-G02" && g.status != GateStatus::NotEvaluated) {
        CertificationState::BuildCapable
    } else {
        CertificationState::Experimental
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gates_count() {
        assert_eq!(dia_gates().len(), 15);
    }

    #[test]
    fn critical_gates_identified() {
        let critical: Vec<_> = dia_gates().into_iter().filter(|g| g.severity == GateSeverity::Critical).collect();
        assert_eq!(critical.len(), 8); // G01, G02, G03, G04, G05, G07, G14, G15
    }

    #[test]
    fn certification_states() {
        let mut gates = dia_gates();
        // All not evaluated -> Experimental
        assert_eq!(certification_state(&gates), CertificationState::Experimental);

        // G02 not evaluated -> BuildCapable
        gates.iter_mut().find(|g| g.id == "DIA-G02").unwrap().status = GateStatus::Fail;
        assert_eq!(certification_state(&gates), CertificationState::BuildCapable);

        // G02 passes -> ExecutionCapable
        gates.iter_mut().find(|g| g.id == "DIA-G02").unwrap().status = GateStatus::Pass;
        assert_eq!(certification_state(&gates), CertificationState::ExecutionCapable);
    }
}