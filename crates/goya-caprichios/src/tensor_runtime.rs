//! Process-local building blocks for Tensor neural-acceleration plans.
//!
//! [`EdgeTpuPlan`] performs the checked arithmetic for a bounded
//! neural-delegate execution plan. This type plans the delegation; it never
//! compiles models, contacts a npu DSP, or executes inference.

use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EdgeTpuError {
    ZeroLayers,
    LayersTooLarge,
    ZeroBatch,
    BatchTooLarge,
    ZeroWeightBytes,
    WeightBytesTooLarge,
    LayerBatchOverflow,
}

impl fmt::Display for EdgeTpuError {
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

impl std::error::Error for EdgeTpuError {}

pub const MAX_LAYERS: u32 = 4_096;
pub const MAX_BATCH: u32 = 65_536;
pub const MAX_WEIGHT_BYTES: u64 = 1 << 30;

/// Checked execution arithmetic for one bounded neural-delegate plan.
/// This type plans the delegation; it never executes inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EdgeTpuPlan {
    layers: u32,
    batch: u32,
    max_weight_bytes: u64,
}

impl EdgeTpuPlan {
    pub fn new(layers: u32, batch: u32, max_weight_bytes: u64) -> Result<Self, EdgeTpuError> {
        if layers == 0 {
            return Err(EdgeTpuError::ZeroLayers);
        }
        if layers > MAX_LAYERS {
            return Err(EdgeTpuError::LayersTooLarge);
        }
        if batch == 0 {
            return Err(EdgeTpuError::ZeroBatch);
        }
        if batch > MAX_BATCH {
            return Err(EdgeTpuError::BatchTooLarge);
        }
        if max_weight_bytes == 0 {
            return Err(EdgeTpuError::ZeroWeightBytes);
        }
        if max_weight_bytes > MAX_WEIGHT_BYTES {
            return Err(EdgeTpuError::WeightBytesTooLarge);
        }
        if u64::from(layers) * u64::from(batch) > u64::from(u32::MAX) {
            return Err(EdgeTpuError::LayerBatchOverflow);
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
        let plan = EdgeTpuPlan::new(MAX_LAYERS, MAX_BATCH, MAX_WEIGHT_BYTES).unwrap();
        assert_eq!(plan.layers(), MAX_LAYERS);
        assert_eq!(plan.total_executions(), u64::from(MAX_LAYERS) * u64::from(MAX_BATCH));
        assert_eq!(
            EdgeTpuPlan::new(0, 1, 1024),
            Err(EdgeTpuError::ZeroLayers)
        );
        assert_eq!(
            EdgeTpuPlan::new(MAX_LAYERS + 1, 1, 1024),
            Err(EdgeTpuError::LayersTooLarge)
        );
        assert_eq!(
            EdgeTpuPlan::new(1, 0, 1024),
            Err(EdgeTpuError::ZeroBatch)
        );
        assert_eq!(
            EdgeTpuPlan::new(1, MAX_BATCH + 1, 1024),
            Err(EdgeTpuError::BatchTooLarge)
        );
        assert_eq!(
            EdgeTpuPlan::new(1, 1, 0),
            Err(EdgeTpuError::ZeroWeightBytes)
        );
        assert_eq!(
            EdgeTpuPlan::new(1, 1, MAX_WEIGHT_BYTES + 1),
            Err(EdgeTpuError::WeightBytesTooLarge)
        );
    }
}
