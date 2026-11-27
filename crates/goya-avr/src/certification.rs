//! Inquisition verification classes (§15.1) and certification classes (§15.2).

/// Mandatory verification classes AVR-C01–C14 (directive §15.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerificationClass {
    /// Observation domain validity.
    DomainValidity,
    /// Reachable discrete state completeness.
    StateCompleteness,
    /// Valid observation identifiers.
    ValidObservationIds,
    /// Reconstruction dependency acyclicity.
    DependencyAcyclicity,
    /// Maximum reconstruction depth.
    DependencyDepth,
    /// Navigation connectivity where required.
    NavigationConnectivity,
    /// Neighbourhood resource bounds.
    NeighbourhoodBounds,
    /// Representation integrity.
    RepresentationIntegrity,
    /// Exact reconstruction equivalence.
    ExactReconstruction,
    /// Approximate reconstruction fidelity.
    ApproximateReconstruction,
    /// Target derivation provenance.
    DerivationProvenance,
    /// Substitution authorisation.
    SubstitutionAuthorisation,
    /// Parent element compositional correctness.
    CompositionalCorrectness,
    /// Unsupported observation rejection.
    UnsupportedObservationRejection,
}

impl VerificationClass {
    /// Canonical identifier `AVR-C01`…`AVR-C14`.
    pub fn id(self) -> &'static str {
        match self {
            VerificationClass::DomainValidity => "AVR-C01",
            VerificationClass::StateCompleteness => "AVR-C02",
            VerificationClass::ValidObservationIds => "AVR-C03",
            VerificationClass::DependencyAcyclicity => "AVR-C04",
            VerificationClass::DependencyDepth => "AVR-C05",
            VerificationClass::NavigationConnectivity => "AVR-C06",
            VerificationClass::NeighbourhoodBounds => "AVR-C07",
            VerificationClass::RepresentationIntegrity => "AVR-C08",
            VerificationClass::ExactReconstruction => "AVR-C09",
            VerificationClass::ApproximateReconstruction => "AVR-C10",
            VerificationClass::DerivationProvenance => "AVR-C11",
            VerificationClass::SubstitutionAuthorisation => "AVR-C12",
            VerificationClass::CompositionalCorrectness => "AVR-C13",
            VerificationClass::UnsupportedObservationRejection => "AVR-C14",
        }
    }

    /// All mandatory classes, in directive order.
    pub fn all() -> &'static [VerificationClass] {
        &[
            VerificationClass::DomainValidity,
            VerificationClass::StateCompleteness,
            VerificationClass::ValidObservationIds,
            VerificationClass::DependencyAcyclicity,
            VerificationClass::DependencyDepth,
            VerificationClass::NavigationConnectivity,
            VerificationClass::NeighbourhoodBounds,
            VerificationClass::RepresentationIntegrity,
            VerificationClass::ExactReconstruction,
            VerificationClass::ApproximateReconstruction,
            VerificationClass::DerivationProvenance,
            VerificationClass::SubstitutionAuthorisation,
            VerificationClass::CompositionalCorrectness,
            VerificationClass::UnsupportedObservationRejection,
        ]
    }
}

/// Certification class (§15.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificationClass {
    /// Exhaustively verified or mathematically established equivalence.
    Exact,
    /// Reconstruction error established within a specified bound.
    Bounded,
    /// Fidelity assessed through sampled verification. Empirical
    /// verification MUST NOT be represented as mathematical proof of
    /// continuous-domain completeness.
    Empirical,
    /// Observation cannot be certified.
    Unsupported,
}

impl CertificationClass {
    /// Canonical identifier from §15.2.
    pub fn id(self) -> &'static str {
        match self {
            CertificationClass::Exact => "EXACT",
            CertificationClass::Bounded => "BOUNDED",
            CertificationClass::Empirical => "EMPIRICAL",
            CertificationClass::Unsupported => "UNSUPPORTED",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_ids_are_contiguous_c01_to_c14() {
        let all = VerificationClass::all();
        assert_eq!(all.len(), 14);
        for (i, class) in all.iter().enumerate() {
            assert_eq!(class.id(), format!("AVR-C{:02}", i + 1));
        }
    }

    #[test]
    fn certification_ids_match_directive_table() {
        assert_eq!(CertificationClass::Exact.id(), "EXACT");
        assert_eq!(CertificationClass::Bounded.id(), "BOUNDED");
        assert_eq!(CertificationClass::Empirical.id(), "EMPIRICAL");
        assert_eq!(CertificationClass::Unsupported.id(), "UNSUPPORTED");
    }
}
