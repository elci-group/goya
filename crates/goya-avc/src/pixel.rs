//! Pixel-state record (directive §2) and per-record exactness (ADR-AVC-001 F3, decision 3).
//!
//! A logical pixel carries up to eight attributes: one fill colour, zero to
//! six trim colours (spatial transition anchors) and one independent light
//! value. Pixel coordinates are implicit in row binaries and are not part
//! of the logical record.
//!
//! Exactness is per-record from Phase 0 (F3): trim-assisted reconstruction
//! is approximate by construction, and no record may be certified exact
//! because it merely *looks* like its source (the AVR-C09 principle
//! applied to video).

/// Index into the compiled colour inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColourIndex(pub u16);

/// Maximum trim colours per logical pixel (directive §2).
pub const MAX_TRIMS: usize = 6;

/// Independent luminance modulation, Q8.8 fixed point (0 = black, 0x0100
/// = unity). Light never re-encodes chroma: a darkened object changes only
/// its light value (directive §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Light(pub u16);

impl Light {
    /// Unity intensity.
    pub const UNITY: Light = Light(0x0100);

    /// Multiply two light values in Q8.8 with saturation, deterministically.
    pub const fn mul(self, rhs: Light) -> Light {
        let v = ((self.0 as u32) * (rhs.0 as u32) + 0x80) >> 8;
        Light(if v > u16::MAX as u32 {
            u16::MAX
        } else {
            v as u16
        })
    }
}

/// Why a record's reconstruction is approximate (decision 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApproximationCause {
    /// The record was reconstructed through trim interpolation anchors
    /// (directive §7): approximate by construction.
    TrimAssisted,
    /// The record was reconstructed within a declared tolerance budget.
    ToleranceBudget,
    /// The record was reduced to fit the bounded colour inventory.
    PaletteClosure,
}

/// Per-record exactness class (F3). Trim-assisted records are never exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Exactness {
    /// Byte-identical to the canonical source after decoding.
    Exact,
    /// Reconstructed approximately for the stated cause.
    Approximate(ApproximationCause),
}

impl Exactness {
    pub const fn is_exact(&self) -> bool {
        matches!(self, Exactness::Exact)
    }
}

/// The full logical pixel state (directive §2). `trims[d]` is the anchor
/// for `TrimDirection::d`, present only where the boundary is non-uniform
/// (optional residency, directive §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelState {
    pub fill: ColourIndex,
    pub trims: [Option<ColourIndex>; MAX_TRIMS],
    pub light: Light,
    pub exactness: Exactness,
}

impl PixelState {
    /// A fill-only, exact pixel state: solid regions carry no trims.
    pub const fn exact_fill(fill: ColourIndex) -> Self {
        Self {
            fill,
            trims: [None; MAX_TRIMS],
            light: Light::UNITY,
            exactness: Exactness::Exact,
        }
    }

    /// A fill pixel whose boundary anchors are trim-assisted: classified
    /// approximate by construction, regardless of how good the
    /// interpolation looks (F3).
    pub const fn trim_assisted(fill: ColourIndex, trims: [Option<ColourIndex>; MAX_TRIMS]) -> Self {
        Self {
            fill,
            trims,
            light: Light::UNITY,
            exactness: Exactness::Approximate(ApproximationCause::TrimAssisted),
        }
    }

    pub const fn any_trim(&self) -> bool {
        let mut i = 0;
        while i < MAX_TRIMS {
            if self.trims[i].is_some() {
                return true;
            }
            i += 1;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_assisted_is_never_exact() {
        let mut trims = [None; MAX_TRIMS];
        trims[0] = Some(ColourIndex(7));
        let px = PixelState::trim_assisted(ColourIndex(1), trims);
        assert!(!px.exactness.is_exact());
        assert!(px.any_trim());
        assert_eq!(
            px.exactness,
            Exactness::Approximate(ApproximationCause::TrimAssisted)
        );
    }

    #[test]
    fn exact_fill_has_no_trims_and_unity_light() {
        let px = PixelState::exact_fill(ColourIndex(3));
        assert!(px.exactness.is_exact());
        assert!(!px.any_trim());
        assert_eq!(px.light, Light::UNITY);
    }

    #[test]
    fn light_multiplication_saturates_deterministically() {
        let half = Light(0x0080);
        assert_eq!(half.mul(Light::UNITY), half);
        assert_eq!(half.mul(half), Light(0x0040));
        assert_eq!(Light(u16::MAX).mul(Light(u16::MAX)), Light(u16::MAX));
    }
}
