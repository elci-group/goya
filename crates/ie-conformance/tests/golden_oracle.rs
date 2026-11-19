//! WP-009 seed — golden-oracle cross-check (assurance, S6).
//!
//! ie_cpu::execute is one independent implementation and
//! ie_contract::GoldenFixture::reference_output is another. Over a
//! deterministic grid of shapes and values the two must agree exactly
//! (bit-for-bit) and both must pass the frozen golden tolerance check.

use ie_contract::fixture::{GoldenFixture, GoldenOp};
use ie_cpu;

/// splitmix64 — local deterministic value generator (no external crates).
struct SplitMix64(u64);

impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Deterministic f32 in a well-behaved magnitude range (avoid inf/nan
    /// so exact IEEE equality between the two implementations is stable).
    fn next_f32(&mut self) -> f32 {
        let v = (self.next_u64() % 4_000_001) as i64 - 2_000_000;
        (v as f32) / 1_000.0
    }
}

fn unpack(t: &ie_contract::tensors::TensorData) -> Vec<f32> {
    t.data.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn fixture(op: GoldenOp, inputs: Vec<ie_contract::tensors::TensorData>, shape: Vec<u64>) -> GoldenFixture {
    GoldenFixture {
        name: format!("grid-{}", op.name()),
        op,
        inputs,
        expected: Vec::new(), // filled from the reference oracle below
        expected_shape: shape,
    }
}

/// Compare the two independent implementations exactly and within
/// tolerance over `cases` deterministic random grids.
#[test]
fn add_cross_check_exact_and_tolerance_grid() {
    let mut rng = SplitMix64(0x0ADD_5EED_0000_0001);
    for len in [1usize, 2, 3, 8, 17, 64, 257] {
        for _case in 0..4 {
            let a: Vec<f32> = (0..len).map(|_| rng.next_f32()).collect();
            let b: Vec<f32> = (0..len).map(|_| rng.next_f32()).collect();
            let inputs = vec![
                GoldenFixture::tensor(&a, &[len as u64]),
                GoldenFixture::tensor(&b, &[len as u64]),
            ];
            let mut f = fixture(GoldenOp::Add, inputs, vec![len as u64]);
            let reference = f.reference_output().expect("oracle add");
            f.expected = reference.clone();

            let out = ie_cpu::execute("add", &f.inputs).expect("cpu add");
            assert_eq!(out.desc.dims, vec![len as u64]);
            let got = unpack(&out);
            // Exact equality between the two independent implementations.
            assert_eq!(
                got.len(),
                reference.len(),
                "add len mismatch at len={len}"
            );
            for (i, (g, r)) in got.iter().zip(&reference).enumerate() {
                assert_eq!(g.to_bits(), r.to_bits(), "add exact mismatch at len={len} i={i}");
            }
            // Tolerance pass against the frozen golden check.
            f.check(&got).unwrap_or_else(|e| panic!("add tolerance fail at len={len}: {e}"));
        }
    }
}

#[test]
fn matmul_cross_check_exact_and_tolerance_grid() {
    let mut rng = SplitMix64(0x0C13_5EED_0000_0002);
    let shapes = [
        (1u64, 1u64, 1u64),
        (1, 4, 3),
        (3, 4, 1),
        (2, 2, 2),
        (5, 7, 9),
        (16, 16, 16),
        (8, 33, 5),
        (17, 3, 29),
    ];
    for (m, k, n) in shapes {
        for _case in 0..3 {
            let a: Vec<f32> = (0..(m * k) as usize).map(|_| rng.next_f32()).collect();
            let b: Vec<f32> = (0..(k * n) as usize).map(|_| rng.next_f32()).collect();
            let inputs = vec![
                GoldenFixture::tensor(&a, &[m, k]),
                GoldenFixture::tensor(&b, &[k, n]),
            ];
            let mut f = fixture(GoldenOp::MatMul, inputs, vec![m, n]);
            let reference = f.reference_output().expect("oracle matmul");
            assert_eq!(reference.len(), (m * n) as usize);
            f.expected = reference.clone();

            let out = ie_cpu::execute("matmul", &f.inputs).expect("cpu matmul");
            assert_eq!(out.desc.dims, vec![m, n]);
            let got = unpack(&out);
            for (i, (g, r)) in got.iter().zip(&reference).enumerate() {
                assert_eq!(
                    g.to_bits(),
                    r.to_bits(),
                    "matmul exact mismatch at {m}x{k}x{n} i={i}"
                );
            }
            f.check(&got)
                .unwrap_or_else(|e| panic!("matmul tolerance fail at {m}x{k}x{n}: {e}"));
        }
    }
}

/// Negative control for the harness itself: a corrupted output must fail
/// the golden check (guards against a vacuous always-pass oracle).
#[test]
fn golden_check_rejects_corrupted_output() {
    let inputs = vec![
        GoldenFixture::tensor(&[1.0, 2.0], &[2]),
        GoldenFixture::tensor(&[3.0, 4.0], &[2]),
    ];
    let mut f = fixture(GoldenOp::Add, inputs, vec![2]);
    f.expected = f.reference_output().unwrap();
    assert!(f.check(&[4.0, 6.5]).is_err());
    assert!(f.check(&[4.0]).is_err());
}

/// The CPU backend must reject inputs the golden oracle cannot service:
/// unsupported op names are UNSUPPORTED_OP before any compute.
#[test]
fn unsupported_ops_rejected_consistently() {
    let t = GoldenFixture::tensor(&[1.0], &[1]);
    for name in ["gelu", "softmax", "conv2d", ""] {
        let e = ie_cpu::execute(name, &[t.clone(), t.clone()]).unwrap_err();
        assert_eq!(e.name(), "UNSUPPORTED_OP", "op {name:?}");
    }
}
