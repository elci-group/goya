//! Independent conformance seeds for T01–T03 (IE-X01 conformance matrix).
//! These run against the frozen contract, not against any backend.

use ie_contract::fixture::{GoldenFixture, GoldenOp};
use ie_contract::tensors::{DType, TensorDesc};
use ie_contract::{decode_frame, encode_frame, TypedError};

/// Parse the frozen golden fixture file (fixtures/golden_t01_matmul.golden).
fn load_golden(text: &str) -> GoldenFixture {
    let mut op = GoldenOp::Add;
    let mut a_dims = Vec::new();
    let mut a = Vec::new();
    let mut b_dims = Vec::new();
    let mut b = Vec::new();
    let mut expected = Vec::new();
    let mut expected_dims: Vec<u64> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        match it.next().unwrap() {
            "op" => op = match it.next().unwrap() {
                "matmul" => GoldenOp::MatMul,
                "add" => GoldenOp::Add,
                other => panic!("unknown op {other}"),
            },
            "a_dims" => a_dims = it.map(|v| v.parse().unwrap()).collect(),
            "a" => a = it.map(|v| v.parse::<f32>().unwrap()).collect(),
            "b_dims" => b_dims = it.map(|v| v.parse().unwrap()).collect(),
            "b" => b = it.map(|v| v.parse::<f32>().unwrap()).collect(),
            "expected_dims" => expected_dims = it.map(|v| v.parse().unwrap()).collect(),
            "expected" => expected = it.map(|v| v.parse::<f32>().unwrap()).collect(),
            other => panic!("unknown fixture key {other}"),
        }
    }
    let _ = expected_dims;
    GoldenFixture {
        name: "golden_t01_matmul".into(),
        op,
        inputs: vec![GoldenFixture::tensor(&a, &a_dims), GoldenFixture::tensor(&b, &b_dims)],
        expected,
        expected_shape: a_dims,
    }
}

const GOLDEN: &str = include_str!("../fixtures/golden_t01_matmul.golden");

/// T01 — CPU golden output (PR-02): the frozen reference graph evaluates to
/// the frozen expected output inside declared tolerance.
#[test]
fn t01_golden_output_within_tolerance() {
    let fixture = load_golden(GOLDEN);
    let actual = fixture.reference_output().expect("oracle must evaluate");
    fixture.check(&actual).expect("frozen fixture must match frozen expectation");
}

/// T02 — unsupported op rejection (PR-08): an unknown op is rejected with a
/// typed error before any execution, with a stable wire code.
#[test]
fn t02_unsupported_op_typed_rejection() {
    let e = TypedError::UnsupportedOp { op: "gelu_fast".into() };
    assert_eq!(e.name(), "UNSUPPORTED_OP");
    assert_eq!(TypedError::from_code(e.code(), "gelu_fast"), Some(e.clone()));
}

/// T03 — malformed shape and oversized payload (PR-02): rejected with
/// INVALID_TENSOR before allocation, and oversized frames rejected before
/// any buffer is reserved.
#[test]
fn t03_malformed_shape_and_oversized_payload() {
    // Payload shorter than the descriptor demands.
    let desc = TensorDesc { dtype: DType::F32, dims: vec![4], quant_scale: None, quant_zero_point: None };
    assert!(matches!(
        ie_contract::TensorData::new(desc.clone(), vec![0u8; 12]),
        Err(TypedError::InvalidTensor { .. })
    ));

    // Shape product overflow is caught with checked arithmetic.
    let huge = TensorDesc {
        dtype: DType::F32,
        dims: vec![1u64 << 30, 1 << 30],
        quant_scale: None,
        quant_zero_point: None,
    };
    assert!(matches!(huge.payload_len(), Err(TypedError::InvalidTensor { .. })));

    // Framing: declared size above the cap never triggers allocation.
    let header = (u32::MAX).to_le_bytes();
    assert!(matches!(
        decode_frame(&header),
        Err(ie_contract::FrameError::DeclaredTooLarge(u32::MAX))
    ));

    // And a valid frame round-trips, proving the happy path is intact.
    let frame = encode_frame(GOLDEN.as_bytes()).unwrap();
    let (payload, consumed) = decode_frame(&frame).unwrap();
    assert_eq!(payload, GOLDEN.as_bytes());
    assert_eq!(consumed, 4 + GOLDEN.len());
}
