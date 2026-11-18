//! ie-cpu — CPU reference backend (WP-003 seed, G2 target).
//!
//! Executes the contract v0.1 golden-op subset over validated FP32 tensors.
//! It is the numerical oracle implementation, not a production scheduler:
//! admission, leases and cancellation live with the runtime (WP-004).
//!
//! Semantics fixed by the contract (IE-X01):
//! - deterministic FP32 arithmetic in fixed iteration order, no threading;
//! - all validation (shapes, payloads, op support) happens before compute;
//! - typed errors only; no panics on peer input.

use ie_contract::errors::TypedError;
use ie_contract::fixture::GoldenOp;
use ie_contract::tensors::{DType, TensorData};

/// Operators the CPU reference supports (allowlist; everything else is
/// rejected with UNSUPPORTED_OP before execution, PR-08/T02).
pub const SUPPORTED_OPS: &[&str] = &["add", "matmul"];

/// Decode f32 elements from a validated FP32 tensor payload.
fn as_f32(t: &TensorData) -> Result<Vec<f32>, TypedError> {
    if t.desc.dtype != DType::F32 {
        return Err(TypedError::InvalidTensor { reason: "CPU reference is FP32-only".into() });
    }
    Ok(t.data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

fn pack_f32(vals: &[f32], dims: &[u64]) -> Result<TensorData, TypedError> {
    let desc = ie_contract::TensorDesc {
        dtype: DType::F32,
        dims: dims.to_vec(),
        quant_scale: None,
        quant_zero_point: None,
    };
    let mut data = Vec::with_capacity(vals.len() * 4);
    for v in vals {
        data.extend_from_slice(&v.to_le_bytes());
    }
    TensorData::new(desc, data)
}

fn op_from_name(name: &str) -> Result<GoldenOp, TypedError> {
    match name {
        "add" => Ok(GoldenOp::Add),
        "matmul" => Ok(GoldenOp::MatMul),
        other => Err(TypedError::UnsupportedOp { op: other.to_string() }),
    }
}

/// Execute one op over validated inputs; returns the typed result tensor.
/// This is the numerical oracle the conformance harness compares against.
pub fn execute(op_name: &str, inputs: &[TensorData]) -> Result<TensorData, TypedError> {
    let op = op_from_name(op_name)?;
    match op {
        GoldenOp::Add => {
            if inputs.len() != 2 {
                return Err(TypedError::InvalidTensor { reason: "add requires 2 inputs".into() });
            }
            let a = as_f32(&inputs[0])?;
            let b = as_f32(&inputs[1])?;
            if inputs[0].desc.dims != inputs[1].desc.dims {
                return Err(TypedError::InvalidTensor { reason: "add shape mismatch".into() });
            }
            let out: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
            pack_f32(&out, &inputs[0].desc.dims)
        }
        GoldenOp::MatMul => {
            if inputs.len() != 2 {
                return Err(TypedError::InvalidTensor { reason: "matmul requires 2 inputs".into() });
            }
            if inputs[0].desc.dims.len() != 2 || inputs[1].desc.dims.len() != 2 {
                return Err(TypedError::InvalidTensor {
                    reason: "matmul requires rank-2 inputs".into(),
                });
            }
            let m = inputs[0].desc.dims[0];
            let k = inputs[0].desc.dims[1];
            if inputs[1].desc.dims[0] != k {
                return Err(TypedError::InvalidTensor {
                    reason: "matmul inner dimension mismatch".into(),
                });
            }
            let n = inputs[1].desc.dims[1];
            let a = as_f32(&inputs[0])?;
            let b = as_f32(&inputs[1])?;
            let (m, k, n) = (m as usize, k as usize, n as usize);
            let mut out = vec![0f32; m * n];
            for i in 0..m {
                for j in 0..n {
                    let mut acc = 0f32;
                    for t in 0..k {
                        acc += a[i * k + t] * b[t * n + j];
                    }
                    out[i * n + j] = acc;
                }
            }
            pack_f32(&out, &[m as u64, n as u64])
        }
    }
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
        let got: Vec<f32> =
            out.data.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
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
}
