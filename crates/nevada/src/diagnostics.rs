//! Stable Nevada diagnostic codes — contract `nevada.diag/0.0.2`.
//!
//! These codes are part of Nevada's public, machine-readable surface: once
//! published, a code's number and its display form (`GY-NV-<n>: <message>`)
//! must never change meaning. Follows the fail-closed house style of
//! `goya-caprichios::registry` and the diagnostic naming convention of
//! `goya-lang` (`GY-E1011` and friends); Nevada occupies the `GY-NV-5xx`
//! series.
//!
//! The variant set is deliberately exhaustive (no wildcard matches are
//! permitted on it) so that adding a code is a visible, reviewed change.

/// Contract identifier for this module.
pub const CONTRACT: &str = "nevada.diag/0.0.2";

/// Stable Nevada diagnostic codes.
///
/// Discriminants are explicit and frozen. Semantics:
///
/// - `501` — an essential layout element cannot be reached by
///   directional navigation;
/// - `502` — the translator refused a touch interaction (no remote-native
///   equivalent, no text input method, ...);
/// - `503` — the device lacks a capability an enhanced feature requires;
/// - `504` — the supplied device profile is unknown/unresolvable;
/// - `505` — the capability census is missing a required baseline (D-pad);
/// - `506` — a feature was refused by policy, independent of capability;
/// - `507` — the validation pipeline itself was blocked (e.g. nothing to
///   validate); never a silent pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum DiagnosticCode {
    /// An essential element is unreachable by directional focus.
    NvFocusUnreachableEssential = 501,
    /// The interaction translator refused an interaction.
    NvTranslationRefused = 502,
    /// A required capability is unsupported on this device.
    NvCapabilityUnsupported = 503,
    /// The device profile is unknown or cannot be resolved.
    NvUnknownProfile = 504,
    /// The capability census is missing a required baseline capability.
    NvMissingRequiredBaseline = 505,
    /// A feature was refused by policy.
    NvFeatureRefused = 506,
    /// The validation pipeline is blocked; nothing was validated.
    NvPipelineBlocked = 507,
}

impl DiagnosticCode {
    /// The stable numeric code, suitable for machine-readable output.
    pub const fn number(self) -> u16 {
        self as u16
    }

    /// The stable `GY-NV-<n>` prefix used in [`Diagnostic`]'s display form.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::NvFocusUnreachableEssential => "GY-NV-501",
            Self::NvTranslationRefused => "GY-NV-502",
            Self::NvCapabilityUnsupported => "GY-NV-503",
            Self::NvUnknownProfile => "GY-NV-504",
            Self::NvMissingRequiredBaseline => "GY-NV-505",
            Self::NvFeatureRefused => "GY-NV-506",
            Self::NvPipelineBlocked => "GY-NV-507",
        }
    }
}

/// One Nevada diagnostic: a stable code plus a human-readable message.
///
/// The display form `GY-NV-<n>: <message>` is part of the contract and is
/// covered by a stability test.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    /// The stable diagnostic code.
    pub code: DiagnosticCode,
    /// Human-readable detail. Never carries meaning a consumer must parse;
    /// the code alone is the contract.
    pub message: String,
}

impl Diagnostic {
    /// Build a diagnostic from a code and any message-like value.
    pub fn new(code: DiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.prefix(), self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exhaustiveness pin: adding a variant must break this match.
    #[test]
    fn codes_are_exhaustive() {
        let code = DiagnosticCode::NvPipelineBlocked;
        let name = match code {
            DiagnosticCode::NvFocusUnreachableEssential => "focus-unreachable-essential",
            DiagnosticCode::NvTranslationRefused => "translation-refused",
            DiagnosticCode::NvCapabilityUnsupported => "capability-unsupported",
            DiagnosticCode::NvUnknownProfile => "unknown-profile",
            DiagnosticCode::NvMissingRequiredBaseline => "missing-required-baseline",
            DiagnosticCode::NvFeatureRefused => "feature-refused",
            DiagnosticCode::NvPipelineBlocked => "pipeline-blocked",
        };
        assert_eq!(name, "pipeline-blocked");
    }

    #[test]
    fn display_format_is_stable() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::NvFocusUnreachableEssential,
            "element 7 unreachable",
        );
        assert_eq!(diagnostic.to_string(), "GY-NV-501: element 7 unreachable");
    }

    #[test]
    fn display_prefix_tracks_discriminant() {
        let expected: [(DiagnosticCode, u16, &str); 7] = [
            (
                DiagnosticCode::NvFocusUnreachableEssential,
                501,
                "GY-NV-501",
            ),
            (DiagnosticCode::NvTranslationRefused, 502, "GY-NV-502"),
            (DiagnosticCode::NvCapabilityUnsupported, 503, "GY-NV-503"),
            (DiagnosticCode::NvUnknownProfile, 504, "GY-NV-504"),
            (DiagnosticCode::NvMissingRequiredBaseline, 505, "GY-NV-505"),
            (DiagnosticCode::NvFeatureRefused, 506, "GY-NV-506"),
            (DiagnosticCode::NvPipelineBlocked, 507, "GY-NV-507"),
        ];
        for (code, number, prefix) in expected {
            assert_eq!(code.number(), number);
            assert_eq!(code.prefix(), prefix);
            let diagnostic = Diagnostic::new(code, "m");
            assert_eq!(diagnostic.to_string(), format!("{prefix}: m"));
        }
    }

    #[test]
    fn codes_order_by_discriminant() {
        assert!(DiagnosticCode::NvFocusUnreachableEssential < DiagnosticCode::NvPipelineBlocked);
    }
}
