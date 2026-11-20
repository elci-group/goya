//! ie-cpu — bounded CPU reference primitives (WP-003 seed, G2 target).
//!
//! Executes the contract v0.1 golden-op subset over validated FP32 tensors.
//! The `graph` and `local` modules provide compiled local graph execution;
//! `ledger` remains the legacy request-ID compatibility layer and is not the
//! compiled local runtime. Admission, leases and cancellation policy belong
//! to the runtime; primitive execution accepts cooperative checkpoints.
//!
//! Semantics fixed by the contract (IE-X01):
//! - deterministic FP32 arithmetic in fixed iteration order, no threading;
//! - all validation (shapes, payloads, op support) happens before compute;
//! - typed errors only; no panics on peer input.

pub mod graph;
pub mod ledger;
pub mod local;
mod onnx;
pub mod package;

use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenOp;
use ie_contract::tensors::{DType, TensorData, TensorDesc};

/// Operators the CPU reference supports (allowlist; everything else is
/// rejected with UNSUPPORTED_OP before execution, PR-08/T02).
pub const SUPPORTED_OPS: &[&str] = &["add", "matmul"];

fn invalid(reason: &str) -> TypedError {
    TypedError::InvalidTensor {
        reason: reason.into(),
    }
}

/// Validate a descriptor and payload even when callers constructed the
/// public TensorData fields directly instead of using TensorData::new.
fn validate_f32(t: &TensorData) -> Result<(), TypedError> {
    if t.desc.dtype != DType::F32 {
        return Err(invalid("CPU reference is FP32-only"));
    }
    t.desc.validate(t.data.len())
}

fn f32_desc(dims: &[u64]) -> TensorDesc {
    TensorDesc {
        dtype: DType::F32,
        dims: dims.to_vec(),
        quant_scale: None,
        quant_zero_point: None,
    }
}

fn read_f32(data: &[u8], index: usize) -> f32 {
    let offset = index * 4;
    f32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Validate an operation and derive its bounded output shape and scalar work.
pub(crate) fn plan_pair(
    op: GoldenOp,
    left: &TensorDesc,
    right: &TensorDesc,
) -> Result<(TensorDesc, u64), TypedError> {
    for desc in [left, right] {
        if desc.dtype != DType::F32 {
            return Err(invalid("CPU reference is FP32-only"));
        }
        desc.payload_len()?;
    }
    match op {
        GoldenOp::Add => {
            if left.dims != right.dims {
                return Err(invalid("add shape mismatch"));
            }
            let count = left
                .dims
                .iter()
                .try_fold(1u64, |n, d| n.checked_mul(*d))
                .ok_or_else(|| invalid("add work overflow"))?;
            let output = f32_desc(&left.dims);
            output.payload_len()?;
            Ok((output, count))
        }
        GoldenOp::MatMul => {
            if left.dims.len() != 2 || right.dims.len() != 2 {
                return Err(invalid("matmul requires rank-2 inputs"));
            }
            let (m, k, rk, n) = (left.dims[0], left.dims[1], right.dims[0], right.dims[1]);
            if rk != k {
                return Err(invalid("matmul inner dimension mismatch"));
            }
            let work = m
                .checked_mul(k)
                .and_then(|v| v.checked_mul(n))
                .ok_or_else(|| invalid("matmul work overflow"))?;
            let output = f32_desc(&[m, n]);
            output.payload_len()?;
            Ok((output, work))
        }
    }
}

/// Execute a validated pair without materializing decoded input or FP32
/// output scratch vectors. Checkpoints run at most 1024 scalar operations apart.
pub(crate) fn execute_pair(
    op: GoldenOp,
    left: &TensorData,
    right: &TensorData,
    checkpoint: &mut impl FnMut() -> Result<(), TypedError>,
) -> Result<TensorData, TypedError> {
    validate_f32(left)?;
    validate_f32(right)?;
    let (desc, _) = plan_pair(op, &left.desc, &right.desc)?;
    let output_bytes = usize::try_from(desc.payload_len()?)
        .map_err(|_| invalid("output does not fit address space"))?;
    let mut data = Vec::new();
    data.try_reserve_exact(output_bytes)
        .map_err(|_| TypedError::ResourceExhausted {
            reason: "CPU output allocation failed".into(),
        })?;
    let mut since_checkpoint = 0u64;
    match op {
        GoldenOp::Add => {
            let count = usize::try_from(left.desc.payload_len()? / 4)
                .map_err(|_| invalid("input does not fit address space"))?;
            for i in 0..count {
                if since_checkpoint == 0 {
                    checkpoint()?;
                }
                let value = read_f32(&left.data, i) + read_f32(&right.data, i);
                data.extend_from_slice(&value.to_le_bytes());
                since_checkpoint = (since_checkpoint + 1) % 1024;
            }
        }
        GoldenOp::MatMul => {
            let m =
                usize::try_from(left.desc.dims[0]).map_err(|_| invalid("dimension overflow"))?;
            let k =
                usize::try_from(left.desc.dims[1]).map_err(|_| invalid("dimension overflow"))?;
            let n =
                usize::try_from(right.desc.dims[1]).map_err(|_| invalid("dimension overflow"))?;
            for i in 0..m {
                for j in 0..n {
                    let mut acc = 0f32;
                    for t in 0..k {
                        if since_checkpoint == 0 {
                            checkpoint()?;
                        }
                        acc += read_f32(&left.data, i * k + t) * read_f32(&right.data, t * n + j);
                        since_checkpoint = (since_checkpoint + 1) % 1024;
                    }
                    data.extend_from_slice(&acc.to_le_bytes());
                }
            }
        }
    }
    checkpoint()?;
    TensorData::new(desc, data)
}

fn op_from_name(name: &str) -> Result<GoldenOp, TypedError> {
    match name {
        "add" => Ok(GoldenOp::Add),
        "matmul" => Ok(GoldenOp::MatMul),
        other => Err(TypedError::UnsupportedOp {
            op: other.to_string(),
        }),
    }
}

/// Execute one op over validated inputs; returns the typed result tensor.
/// This is the numerical oracle the conformance harness compares against.
pub fn execute(op_name: &str, inputs: &[TensorData]) -> Result<TensorData, TypedError> {
    let op = op_from_name(op_name)?;
    if inputs.len() != 2 {
        return Err(invalid("operation requires 2 inputs"));
    }
    execute_pair(op, &inputs[0], &inputs[1], &mut || Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ie_contract::fixture::{GoldenFixture, TOLERANCE};

    #[test]
    fn executes_golden_ops_within_tolerance() {
        let a = GoldenFixture::tensor(&[1.0, 2.0, 3.0, 4.0], &[2, 2]);
        let b = GoldenFixture::tensor(&[5.0, 6.0, 7.0, 8.0], &[2, 2]);
        let out = execute("matmul", &[a, b]).unwrap();
        let got: Vec<f32> = out
            .data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let _ = TOLERANCE;
        assert_eq!(got, vec![19.0, 22.0, 43.0, 50.0]);
        assert_eq!(out.desc.dims, vec![2, 2]);
    }

    #[test]
    fn unsupported_op_rejected_typed() {
        let t = GoldenFixture::tensor(&[1.0], &[1]);
        let e = execute("gelu", &[t]).unwrap_err();
        assert_eq!(e.name(), "UNSUPPORTED_OP");
    }

    #[test]
    fn int8_rejected_fp32_only() {
        let desc = ie_contract::TensorDesc {
            dtype: DType::I8,
            dims: vec![2],
            quant_scale: Some(0.5),
            quant_zero_point: Some(0),
        };
        let t = TensorData::new(desc, vec![1, 2]).unwrap();
        assert!(matches!(
            execute("add", &[t.clone(), t]),
            Err(TypedError::InvalidTensor { .. })
        ));
    }

    #[test]
    fn shape_mismatch_rejected_before_compute() {
        let a = GoldenFixture::tensor(&[1.0, 2.0], &[2]);
        let b = GoldenFixture::tensor(&[1.0, 2.0, 3.0], &[3]);
        assert!(matches!(
            execute("add", &[a, b]),
            Err(TypedError::InvalidTensor { .. })
        ));
    }

    #[test]
    fn malformed_public_tensor_fields_are_rejected() {
        let malformed = TensorData {
            desc: ie_contract::TensorDesc {
                dtype: DType::F32,
                dims: vec![1],
                quant_scale: None,
                quant_zero_point: None,
            },
            data: vec![0, 0, 0, 0, 99],
        };
        let valid = GoldenFixture::tensor(&[1.0], &[1]);
        assert!(matches!(
            execute("add", &[malformed, valid]),
            Err(TypedError::InvalidTensor { .. })
        ));
    }

    #[test]
    fn oversized_matmul_output_is_rejected_during_planning() {
        let left = ie_contract::TensorDesc {
            dtype: DType::F32,
            dims: vec![8192, 2],
            quant_scale: None,
            quant_zero_point: None,
        };
        let right = ie_contract::TensorDesc {
            dtype: DType::F32,
            dims: vec![2, 8192],
            quant_scale: None,
            quant_zero_point: None,
        };
        assert!(matches!(
            plan_pair(GoldenOp::MatMul, &left, &right),
            Err(TypedError::InvalidTensor { .. })
        ));
    }

    #[test]
    fn checkpoint_aborts_inside_long_matmul_dot_product() {
        let left_desc = ie_contract::TensorDesc {
            dtype: DType::F32,
            dims: vec![1, 2048],
            quant_scale: None,
            quant_zero_point: None,
        };
        let right_desc = ie_contract::TensorDesc {
            dtype: DType::F32,
            dims: vec![2048, 1],
            quant_scale: None,
            quant_zero_point: None,
        };
        let left = TensorData::new(left_desc, vec![0; 2048 * 4]).unwrap();
        let right = TensorData::new(right_desc, vec![0; 2048 * 4]).unwrap();
        let mut calls = 0;
        let result = execute_pair(GoldenOp::MatMul, &left, &right, &mut || {
            calls += 1;
            if calls == 2 {
                Err(TypedError::DeadlineExceeded)
            } else {
                Ok(())
            }
        });
        assert_eq!(result, Err(TypedError::DeadlineExceeded));
        assert_eq!(calls, 2, "deadline must be checked within the dot product");
    }
}
