//! Process-local building blocks for Snapdragon neural-acceleration plans.
//!
//! [`HexagonDelegatePlan`] performs the checked arithmetic for a bounded
//! neural-delegate execution plan. This type plans the delegation; it never
//! compiles models, contacts a Hexagon DSP, or executes inference.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HexagonError {
    ZeroLayers,
    LayersTooLarge,
    ZeroBatch,
    BatchTooLarge,
    ZeroWeightBytes,
    WeightBytesTooLarge,
    LayerBatchOverflow,
}

impl fmt::Display for HexagonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroLayers => "layer count must be positive",
            Self::LayersTooLarge => "layer count exceeds the supported bound",
            Self::ZeroBatch => "batch size must be positive",
            Self::BatchTooLarge => "batch size exceeds the supported bound",
            Self::ZeroWeightBytes => "weight bound must be positive",
            Self::WeightBytesTooLarge => "weight bound exceeds the supported bound",
            Self::LayerBatchOverflow => "total layer executions do not fit a u64 bound",
        })
    }
}

impl std::error::Error for HexagonError {}

pub const MAX_LAYERS: u32 = 4_096;
pub const MAX_BATCH: u32 = 65_536;
pub const MAX_WEIGHT_BYTES: u64 = 1 << 30;

/// Checked execution arithmetic for one bounded neural-delegate plan.
/// This type plans the delegation; it never executes inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HexagonDelegatePlan {
    layers: u32,
    batch: u32,
    max_weight_bytes: u64,
}

impl HexagonDelegatePlan {
    pub fn new(layers: u32, batch: u32, max_weight_bytes: u64) -> Result<Self, HexagonError> {
        if layers == 0 {
            return Err(HexagonError::ZeroLayers);
        }
        if layers > MAX_LAYERS {
            return Err(HexagonError::LayersTooLarge);
        }
        if batch == 0 {
            return Err(HexagonError::ZeroBatch);
        }
        if batch > MAX_BATCH {
            return Err(HexagonError::BatchTooLarge);
        }
        if max_weight_bytes == 0 {
            return Err(HexagonError::ZeroWeightBytes);
        }
        if max_weight_bytes > MAX_WEIGHT_BYTES {
            return Err(HexagonError::WeightBytesTooLarge);
        }
        if u64::from(layers) * u64::from(batch) > u64::from(u32::MAX) {
            return Err(HexagonError::LayerBatchOverflow);
        }
        Ok(Self {
            layers,
            batch,
            max_weight_bytes,
        })
    }

    pub fn layers(&self) -> u32 {
        self.layers
    }

    pub fn batch(&self) -> u32 {
        self.batch
    }

    pub fn max_weight_bytes(&self) -> u64 {
        self.max_weight_bytes
    }

    /// Total layer executions across the batch.
    pub fn total_executions(&self) -> u64 {
        u64::from(self.layers) * u64::from(self.batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_accepts_full_bounds_and_rejects_violations() {
        let plan = HexagonDelegatePlan::new(MAX_LAYERS, MAX_BATCH, MAX_WEIGHT_BYTES).unwrap();
        assert_eq!(plan.layers(), MAX_LAYERS);
        assert_eq!(
            plan.total_executions(),
            u64::from(MAX_LAYERS) * u64::from(MAX_BATCH)
        );
        assert_eq!(
            HexagonDelegatePlan::new(0, 1, 1024),
            Err(HexagonError::ZeroLayers)
        );
        assert_eq!(
            HexagonDelegatePlan::new(MAX_LAYERS + 1, 1, 1024),
            Err(HexagonError::LayersTooLarge)
        );
        assert_eq!(
            HexagonDelegatePlan::new(1, 0, 1024),
            Err(HexagonError::ZeroBatch)
        );
        assert_eq!(
            HexagonDelegatePlan::new(1, MAX_BATCH + 1, 1024),
            Err(HexagonError::BatchTooLarge)
        );
        assert_eq!(
            HexagonDelegatePlan::new(1, 1, 0),
            Err(HexagonError::ZeroWeightBytes)
        );
        assert_eq!(
            HexagonDelegatePlan::new(1, 1, MAX_WEIGHT_BYTES + 1),
            Err(HexagonError::WeightBytesTooLarge)
        );
    }
}
