//! Diagnostic code registry (directive §21).
//!
//! Codes follow the `CASA-XXX-NNN` shape: a short subsystem tag and a
//! stable number. Codes are additive only — never renumber or repurpose.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A stable entry in the CASA diagnostic registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticCode {
    pub code: &'static str,
    pub description: &'static str,
    pub default_severity: Severity,
}

const fn entry(
    code: &'static str,
    description: &'static str,
    default_severity: Severity,
) -> DiagnosticCode {
    DiagnosticCode {
        code,
        description,
        default_severity,
    }
}

// Syntax (CASA-SYN)
pub const SYNTAX_INVALID: DiagnosticCode = entry(
    "CASA-SYN-001",
    "the CSS parser discarded an invalid rule, declaration or selector",
    Severity::Warning,
);

// Imports (CASA-IMP)
pub const IMPORT_CYCLE: DiagnosticCode = entry(
    "CASA-IMP-001",
    "@import graph contains a cycle; the repeated sheet is skipped",
    Severity::Error,
);
pub const IMPORT_UNRESOLVED: DiagnosticCode = entry(
    "CASA-IMP-002",
    "@import target could not be loaded",
    Severity::Error,
);
pub const IMPORT_MISPLACED: DiagnosticCode = entry(
    "CASA-IMP-003",
    "@import appears after a style rule; ignored per CSS cascade layers specification",
    Severity::Warning,
);

// Resources (CASA-RES)
pub const RESOURCE_DUPLICATE: DiagnosticCode = entry(
    "CASA-RES-002",
    "resource URL referenced more than once; registry keeps the first occurrence",
    Severity::Info,
);

// IR (CASA-IR)
pub const IR_SCHEMA_MISMATCH: DiagnosticCode = entry(
    "CASA-IR-001",
    "serialized CasaIR schema version differs from this build",
    Severity::Error,
);
pub const IR_DEPENDENCY_CYCLE: DiagnosticCode = entry(
    "CASA-IR-002",
    "property dependency graph contains a cycle",
    Severity::Warning,
);
pub const IR_UNSUPPORTED: DiagnosticCode = entry(
    "CASA-IR-003",
    "construct parsed but not yet represented in CasaIR; recorded and skipped",
    Severity::Info,
);

// Cascade (CASA-CAS)
pub const CASCADE_PROPERTY_CYCLE: DiagnosticCode = entry(
    "CASA-CAS-001",
    "custom property cycle detected; the value is invalid at computed-value time",
    Severity::Error,
);
pub const CASCADE_INVALID_AT_COMPUTED_VALUE_TIME: DiagnosticCode = entry(
    "CASA-CAS-002",
    "var() references an undefined custom property without a fallback",
    Severity::Warning,
);
pub const CASCADE_UNEVALUABLE: DiagnosticCode = entry(
    "CASA-CAS-003",
    "conditional rule could not be evaluated and was excluded",
    Severity::Warning,
);

// Selectors (CASA-SEL)
pub const SELECTOR_UNMATCHABLE: DiagnosticCode = entry(
    "CASA-SEL-001",
    "selector cannot be compiled for matching; its rule is excluded",
    Severity::Warning,
);

// Geometry (CASA-GEO)
pub const GEOMETRY_UNRESOLVABLE: DiagnosticCode = entry(
    "CASA-GEO-001",
    "a length or percentage could not be resolved; the property keeps its initial geometry",
    Severity::Warning,
);
pub const GEOMETRY_CALC_INVALID: DiagnosticCode = entry(
    "CASA-GEO-002",
    "a calc()/min()/max()/clamp() expression is invalid or divides by zero",
    Severity::Warning,
);

// Shadow DOM (CASA-SHD)
pub const SHADOW_HOST_OUTSIDE_SHADOW_ROOT: DiagnosticCode = entry(
    "CASA-SHD-001",
    ":host or ::slotted selector found outside a shadow-root sheet",
    Severity::Warning,
);

/// Every registered code, in subsystem order.
pub const ALL: &[DiagnosticCode] = &[
    SYNTAX_INVALID,
    IMPORT_CYCLE,
    IMPORT_UNRESOLVED,
    IMPORT_MISPLACED,
    RESOURCE_DUPLICATE,
    IR_SCHEMA_MISMATCH,
    IR_DEPENDENCY_CYCLE,
    IR_UNSUPPORTED,
    CASCADE_PROPERTY_CYCLE,
    CASCADE_INVALID_AT_COMPUTED_VALUE_TIME,
    CASCADE_UNEVALUABLE,
    SELECTOR_UNMATCHABLE,
    GEOMETRY_UNRESOLVABLE,
    GEOMETRY_CALC_INVALID,
    SHADOW_HOST_OUTSIDE_SHADOW_ROOT,
];

pub fn lookup(code: &str) -> Option<&'static DiagnosticCode> {
    ALL.iter().find(|entry| entry.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_wellformed() {
        for (i, entry) in ALL.iter().enumerate() {
            assert!(entry.code.starts_with("CASA-"), "{}", entry.code);
            assert!(lookup(entry.code).is_some());
            assert!(
                ALL[i + 1..].iter().all(|other| other.code != entry.code),
                "duplicate code {}",
                entry.code
            );
        }
    }

    #[test]
    fn lookup_is_exact() {
        assert_eq!(lookup("CASA-SYN-001").unwrap().code, "CASA-SYN-001");
        assert!(lookup("CASA-SYN-999").is_none());
    }
}
