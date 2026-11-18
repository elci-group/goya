//! Golden fixture format and FP32 tolerance oracle (IE-X01 "Numerical
//! profile"). Golden FP32 comparisons use abs tol 1e-5, rel tol 1e-4 for the
//! reference fixture only; per-model/operator tolerances are established
//! later, before acceleration.

use crate::tensors::{DType, TensorData, TensorDesc};

pub const TOLERANCE: (f32, f32) = (1e-5, 1e-4);

/// Golden reference fixture for conformance test T01: a graph profile is a
/// single op over little-endian FP32 tensors (v0.1 reference scope).
#[derive(Clone, Debug, PartialEq)]
pub struct GoldenFixture {
    pub name: String,
    pub op: GoldenOp,
    pub inputs: Vec<TensorData>,
    pub expected: Vec<f32>,
    pub expected_shape: Vec<u64>,
}

/// Operator subset of the v0.1 golden graph profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoldenOp {
    /// Elementwise add, same shape.
    Add,
    /// Row-major matrix multiply: [m,k] x [k,n] -> [m,n].
    MatMul,
}

impl GoldenOp {
    pub fn name(self) -> &'static str {
        match self {
            GoldenOp::Add => "add",
            GoldenOp::MatMul => "matmul",
        }
    }
}

fn as_f32(t: &TensorData) -> Result<Vec<f32>, String> {
    if t.desc.dtype != DType::F32 {
        return Err("golden oracle is FP32-only".into());
    }
    Ok(t.data.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

impl GoldenFixture {
    /// Evaluate the reference oracle. Deterministic: no RNG, no threading,
    /// IEEE-754 f32 arithmetic in fixed order.
    pub fn reference_output(&self) -> Result<Vec<f32>, String> {
        match self.op {
            GoldenOp::Add => {
                let a = as_f32(&self.inputs[0])?;
                let b = as_f32(&self.inputs[1])?;
                if a.len() != b.len() {
                    return Err("add shape mismatch".into());
                }
                Ok(a.iter().zip(b).map(|(x, y)| x + y).collect())
            }
            GoldenOp::MatMul => {
                let a = as_f32(&self.inputs[0])?;
                let b = as_f32(&self.inputs[1])?;
                let (m, k) = (self.inputs[0].desc.dims[0] as usize, self.inputs[0].desc.dims[1] as usize);
                let k2 = self.inputs[1].desc.dims[0] as usize;
                let n = self.inputs[1].desc.dims[1] as usize;
                if k != k2 {
                    return Err("matmul inner dimension mismatch".into());
                }
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
                Ok(out)
            }
        }
    }

    /// Golden comparison: |a-b| <= abs OR |a-b| <= rel*|b| per element
    /// (IE-X01 proposed reference tolerances, frozen at v0.1).
    pub fn check(&self, actual: &[f32]) -> Result<(), String> {
        if actual.len() != self.expected.len() {
            return Err(format!("output length {} != expected {}", actual.len(), self.expected.len()));
        }
        let (abs_tol, rel_tol) = TOLERANCE;
        for (i, (&a, &e)) in actual.iter().zip(&self.expected).enumerate() {
            let d = (a - e).abs();
            if !(d <= abs_tol || d <= rel_tol * e.abs()) {
                return Err(format!("element {i}: {a} vs expected {e} (|d|={d})"));
            }
        }
        Ok(())
    }

    /// Deterministic little-endian FP32 tensor from values.
    pub fn tensor(vals: &[f32], dims: &[u64]) -> TensorData {
        let desc = TensorDesc {
            dtype: DType::F32,
            dims: dims.to_vec(),
            quant_scale: None,
            quant_zero_point: None,
        };
        let mut data = Vec::with_capacity(vals.len() * 4);
        for v in vals {
            data.extend_from_slice(&v.to_le_bytes());
        }
        TensorData::new(desc, data).expect("golden fixture tensor must be valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_oracle_and_tolerance() {
        let f = GoldenFixture {
            name: "t01-add".into(),
            op: GoldenOp::Add,
            inputs: vec![
                GoldenFixture::tensor(&[1.0, 2.0], &[2]),
                GoldenFixture::tensor(&[3.0, 4.0], &[2]),
            ],
            expected: vec![4.0, 6.0],
            expected_shape: vec![2],
        };
        let out = f.reference_output().unwrap();
        assert!(f.check(&out).is_ok());
        // Slightly off result inside relative tolerance passes.
        assert!(f.check(&[4.0005, 6.0]).is_ok());
        // Clearly wrong result fails.
        assert!(f.check(&[9.0, 6.0]).is_err());
    }

    #[test]
    fn matmul_oracle_deterministic() {
        let f = GoldenFixture {
            name: "t01-matmul".into(),
            op: GoldenOp::MatMul,
            inputs: vec![
                GoldenFixture::tensor(&[1.0, 2.0, 3.0, 4.0], &[2, 2]),
                GoldenFixture::tensor(&[5.0, 6.0, 7.0, 8.0], &[2, 2]),
            ],
            expected: vec![19.0, 22.0, 43.0, 50.0],
            expected_shape: vec![2, 2],
        };
        assert!(f.check(&f.reference_output().unwrap()).is_ok());
    }
}
