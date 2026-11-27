//! Relational encoding classes (directive §8.5).

/// How a node in a relational neighbourhood resolves to a graphical outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncodingClass {
    /// Independently reconstructable observation.
    Anchor,
    /// Reference to an identical observation.
    Copy,
    /// Differential reconstruction.
    Delta,
    /// Generative reconstruction (Albit).
    Albit,
    /// Depth-assisted observation reconstruction.
    Reproject,
    /// Reference to another neighbourhood's observation.
    Shared,
    /// Combination of independently stored graphical components.
    Composite,
    /// Authorised runtime rendering fallback.
    Live,
}

impl EncodingClass {
    /// Canonical identifier from directive §8.5.
    pub fn id(self) -> &'static str {
        match self {
            EncodingClass::Anchor => "ANCHOR",
            EncodingClass::Copy => "COPY",
            EncodingClass::Delta => "DELTA",
            EncodingClass::Albit => "ALBIT",
            EncodingClass::Reproject => "REPROJECT",
            EncodingClass::Shared => "SHARED",
            EncodingClass::Composite => "COMPOSITE",
            EncodingClass::Live => "LIVE",
        }
    }

    /// True when choosing this encoding requires the compiler to evaluate
    /// reconstruction expense before selection (directive §8.5): storage
    /// reduction alone never justifies a representation that violates the
    /// target execution budget.
    pub fn requires_reconstruction_cost_review(self) -> bool {
        matches!(
            self,
            EncodingClass::Delta
                | EncodingClass::Albit
                | EncodingClass::Reproject
                | EncodingClass::Composite
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_directive_table() {
        for (class, id) in [
            (EncodingClass::Anchor, "ANCHOR"),
            (EncodingClass::Copy, "COPY"),
            (EncodingClass::Delta, "DELTA"),
            (EncodingClass::Albit, "ALBIT"),
            (EncodingClass::Reproject, "REPROJECT"),
            (EncodingClass::Shared, "SHARED"),
            (EncodingClass::Composite, "COMPOSITE"),
            (EncodingClass::Live, "LIVE"),
        ] {
            assert_eq!(class.id(), id);
        }
    }

    #[test]
    fn only_reconstruction_encodings_need_cost_review() {
        assert!(EncodingClass::Albit.requires_reconstruction_cost_review());
        assert!(EncodingClass::Reproject.requires_reconstruction_cost_review());
        assert!(!EncodingClass::Anchor.requires_reconstruction_cost_review());
        assert!(!EncodingClass::Live.requires_reconstruction_cost_review());
    }
}
