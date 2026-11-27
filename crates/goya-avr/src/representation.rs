//! Representation classes (directive §4.2).
//!
//! An observation is an abstract graphical outcome, not inherently a
//! raster image. The compiler evaluates eligible representations against
//! fidelity, storage, decoding and runtime requirements; no class is
//! privileged here.

/// Abstract class of a stored or reconstructed observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepresentationClass {
    /// Projected vector geometry.
    Vec,
    /// Rasterised observation.
    Ras,
    /// Differential observation.
    Dif,
    /// Generatively reconstructed observation (Albit).
    Alb,
    /// Colour-and-depth representation.
    Dep,
    /// Layered graphical representation.
    Lay,
    /// Partial or complete spatial geometry.
    Msh,
    /// Composite representation.
    Hyb,
}

impl RepresentationClass {
    /// Canonical four-character identifier from directive §4.2.
    pub fn id(self) -> &'static str {
        match self {
            RepresentationClass::Vec => "VEC",
            RepresentationClass::Ras => "RAS",
            RepresentationClass::Dif => "DIF",
            RepresentationClass::Alb => "ALB",
            RepresentationClass::Dep => "DEP",
            RepresentationClass::Lay => "LAY",
            RepresentationClass::Msh => "MSH",
            RepresentationClass::Hyb => "HYB",
        }
    }

    /// True for representations that resolve to a concrete outcome only
    /// through another stored component (`DIF`, `ALB`, `HYB`). Such
    /// representations always carry reconstruction dependencies and must
    /// be accounted for in the reconstruction graph (AVR-C04/C05).
    pub fn is_derived(self) -> bool {
        matches!(self, RepresentationClass::Dif | RepresentationClass::Alb | RepresentationClass::Hyb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_match_directive_table() {
        assert_eq!(RepresentationClass::Vec.id(), "VEC");
        assert_eq!(RepresentationClass::Ras.id(), "RAS");
        assert_eq!(RepresentationClass::Dif.id(), "DIF");
        assert_eq!(RepresentationClass::Alb.id(), "ALB");
        assert_eq!(RepresentationClass::Dep.id(), "DEP");
        assert_eq!(RepresentationClass::Lay.id(), "LAY");
        assert_eq!(RepresentationClass::Msh.id(), "MSH");
        assert_eq!(RepresentationClass::Hyb.id(), "HYB");
    }

    #[test]
    fn derived_representations_need_dependencies() {
        assert!(RepresentationClass::Dif.is_derived());
        assert!(RepresentationClass::Alb.is_derived());
        assert!(RepresentationClass::Hyb.is_derived());
        assert!(!RepresentationClass::Ras.is_derived());
        assert!(!RepresentationClass::Msh.is_derived());
    }
}
