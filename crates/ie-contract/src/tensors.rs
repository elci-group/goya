//! Tensor descriptors and checked shape math (IE-X01 "Numerical profile").
//! Payload length is always validated against the checked shape product
//! before any allocation.

use crate::errors::TypedError;

/// Declared dtypes of the v0.1 numerical profile. Only FP32 and INT8 are in
/// the initial allowlist (IE-A01 "Model execution").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DType {
    F32,
    I8,
}

impl DType {
    pub fn element_size(self) -> usize {
        match self {
            DType::F32 => 4,
            DType::I8 => 1,
        }
    }

    pub fn code(self) -> u8 {
        match self {
            DType::F32 => 1,
            DType::I8 => 2,
        }
    }

    pub fn from_code(code: u8) -> Option<DType> {
        match code {
            1 => Some(DType::F32),
            2 => Some(DType::I8),
            _ => None,
        }
    }
}

/// Static-shape tensor descriptor. Byte order is little-endian, row-major
/// (C order) layout — frozen for v0.1; quantisation metadata is INT8-only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TensorDesc {
    pub dtype: DType,
    pub dims: Vec<u64>,
    /// Optional INT8 quantisation scale; only valid when dtype is I8.
    pub quant_scale: Option<f32>,
    /// Optional INT8 zero point.
    pub quant_zero_point: Option<i32>,
}

/// Hard limits of the v0.1 profile (rejection happens before allocation).
pub const MAX_RANK: usize = 8;
pub const MAX_DIM: u64 = 1 << 30;
pub const MAX_TENSOR_BYTES: u64 = 64 * 1024 * 1024;

impl TensorDesc {
    /// Validate rank/dims and return the payload length in bytes, with all
    /// arithmetic overflow-checked.
    pub fn payload_len(&self) -> Result<u64, TypedError> {
        if self.dims.len() > MAX_RANK {
            return Err(TypedError::InvalidTensor {
                reason: format!("rank {} exceeds MAX_RANK {MAX_RANK}", self.dims.len()),
            });
        }
        let mut product: u64 = 1;
        for &d in &self.dims {
            if d == 0 {
                return Err(TypedError::InvalidTensor { reason: "zero dimension".into() });
            }
            if d > MAX_DIM {
                return Err(TypedError::InvalidTensor {
                    reason: format!("dimension {d} exceeds MAX_DIM {MAX_DIM}"),
                });
            }
            product = product
                .checked_mul(d)
                .ok_or_else(|| TypedError::InvalidTensor { reason: "shape product overflow".into() })?;
        }
        let bytes = product
            .checked_mul(self.dtype.element_size() as u64)
            .ok_or_else(|| TypedError::InvalidTensor { reason: "byte length overflow".into() })?;
        if bytes > MAX_TENSOR_BYTES {
            return Err(TypedError::InvalidTensor {
                reason: format!("{bytes} bytes exceeds MAX_TENSOR_BYTES {MAX_TENSOR_BYTES}"),
            });
        }
        if self.dtype == DType::F32 && (self.quant_scale.is_some() || self.quant_zero_point.is_some())
        {
            return Err(TypedError::InvalidTensor {
                reason: "quantisation metadata on F32 tensor".into(),
            });
        }
        Ok(bytes)
    }

    /// Validate that `data_len` matches this descriptor exactly.
    pub fn validate(&self, data_len: usize) -> Result<(), TypedError> {
        let expected = self.payload_len()?;
        if expected != data_len as u64 {
            return Err(TypedError::InvalidTensor {
                reason: format!("payload {data_len} bytes, descriptor requires {expected}"),
            });
        }
        Ok(())
    }
}

/// Validated tensor payload paired with its descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct TensorData {
    pub desc: TensorDesc,
    pub data: Vec<u8>,
}

impl TensorData {
    /// Construct only after validation; rejects oversized/mismatched payloads
    /// without retaining them.
    pub fn new(desc: TensorDesc, data: Vec<u8>) -> Result<TensorData, TypedError> {
        desc.validate(data.len())?;
        Ok(TensorData { desc, data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f32_desc(dims: &[u64]) -> TensorDesc {
        TensorDesc { dtype: DType::F32, dims: dims.to_vec(), quant_scale: None, quant_zero_point: None }
    }

    #[test]
    fn payload_len_checked() {
        assert_eq!(f32_desc(&[2, 3]).payload_len().unwrap(), 24);
        assert!(f32_desc(&[0]).payload_len().is_err());
        assert!(f32_desc(&[u64::MAX, 2]).payload_len().is_err());
        assert!(f32_desc(&[MAX_DIM, MAX_DIM]).payload_len().is_err());
    }

    #[test]
    fn oversized_rejected() {
        let d = TensorDesc {
            dtype: DType::F32,
            dims: vec![MAX_TENSOR_BYTES / 4 + 1],
            quant_scale: None,
            quant_zero_point: None,
        };
        assert!(matches!(d.payload_len(), Err(TypedError::InvalidTensor { .. })));
    }

    #[test]
    fn length_mismatch_rejected() {
        assert!(f32_desc(&[2]).validate(7).is_err());
        assert!(f32_desc(&[2]).validate(8).is_ok());
    }

    #[test]
    fn quant_metadata_only_on_int8() {
        let bad = TensorDesc {
            dtype: DType::F32,
            dims: vec![1],
            quant_scale: Some(0.5),
            quant_zero_point: None,
        };
        assert!(bad.payload_len().is_err());
    }
}
