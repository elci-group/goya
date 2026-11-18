//! G2 conformance seed: the CPU reference backend must reproduce the frozen
//! golden fixture (T01) byte-for-byte deterministically, and reject
//! unsupported ops/malformed inputs with typed errors (T02/T03 seeds).

use ie_contract::fixture::{GoldenFixture, GoldenOp};
use ie_contract::tensors::TensorData;
use ie_cpu::execute;

const GOLDEN: &str = include_str!("../../ie-contract/fixtures/golden_t01_matmul.golden");

#[test]
fn t01_cpu_backend_matches_frozen_golden() {
    let mut a_dims = Vec::new();
    let mut a = Vec::new();
    let mut b_dims = Vec::new();
    let mut b = Vec::new();
    let mut expected = Vec::new();
    for line in GOLDEN.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        match it.next().unwrap() {
            "op" => {}
            "a_dims" => a_dims = it.map(|v| v.parse().unwrap()).collect(),
            "a" => a = it.map(|v| v.parse::<f32>().unwrap()).collect(),
            "b_dims" => b_dims = it.map(|v| v.parse().unwrap()).collect(),
            "b" => b = it.map(|v| v.parse::<f32>().unwrap()).collect(),
            "expected_dims" => {}
            "expected" => expected = it.map(|v| v.parse::<f32>().unwrap()).collect(),
            other => panic!("unknown key {other}"),
        }
    }
    let fixture = GoldenFixture {
        name: "golden_t01_matmul".into(),
        op: GoldenOp::MatMul,
        inputs: vec![
            GoldenFixture::tensor(&a, &a_dims),
            GoldenFixture::tensor(&b, &b_dims),
        ],
        expected: expected.clone(),
        expected_shape: vec![2, 2],
    };

    let out: TensorData = execute("matmul", &fixture.inputs).expect("matmul must execute");
    let got: Vec<f32> =
        out.data.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
    fixture.check(&got).expect("CPU reference must match frozen golden within tolerance");
    assert_eq!(got, expected, "deterministic oracle: exact equality on this fixture");
}

#[test]
fn determinism_identical_repeated_runs() {
    let a = GoldenFixture::tensor(&[0.1, 0.2, 0.3, 0.4], &[2, 2]);
    let b = GoldenFixture::tensor(&[0.5, 0.6, 0.7, 0.8], &[2, 2]);
    let first = execute("matmul", &[a.clone(), b.clone()]).unwrap();
    for _ in 0..10 {
        assert_eq!(execute("matmul", &[a.clone(), b.clone()]).unwrap(), first);
    }
}
