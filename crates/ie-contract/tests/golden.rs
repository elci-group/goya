//! WP-002 expansion: golden fixture set growth and INT8 exactness vectors
//! (IE-X01). Loads every fixture file and verifies it against the frozen
//! reference oracle with deterministic exact-or-tolerance checks. No RNG,
//! fixed iteration order.

use ie_contract::codec::{decode_tensor, encode_tensor};
use ie_contract::fixture::{GoldenFixture, GoldenOp};
use ie_contract::tensors::{DType, TensorData, TensorDesc};

/// Parse a golden fixture file (same line-based format as the frozen
/// golden_t01_matmul.golden). `name` is passed explicitly so determinism
/// does not depend on file paths.
fn load_golden(name: &str, text: &str) -> GoldenFixture {
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
            "op" => {
                op = match it.next().unwrap() {
                    "matmul" => GoldenOp::MatMul,
                    "add" => GoldenOp::Add,
                    other => panic!("unknown op {other}"),
                }
            }
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
        name: name.into(),
        op,
        inputs: vec![GoldenFixture::tensor(&a, &a_dims), GoldenFixture::tensor(&b, &b_dims)],
        expected,
        expected_shape: expected_dims,
    }
}

/// All golden fixture files, in fixed order (no directory iteration —
/// deterministic by construction).
const GOLDENS: &[(&str, &str)] = &[
    ("golden_t01_matmul", include_str!("../fixtures/golden_t01_matmul.golden")),
    ("golden_t01_add", include_str!("../fixtures/golden_t01_add.golden")),
    ("golden_t01_matmul_3x2", include_str!("../fixtures/golden_t01_matmul_3x2.golden")),
];

/// T01-golden — every golden fixture file evaluates to its frozen expected
/// output EXACTLY (bit-for-bit; the fixtures were hand-computed with values
/// exact in IEEE-754 f32), and therefore trivially inside the frozen
/// tolerance (abs 1e-5, rel 1e-4).
#[test]
fn t01_all_golden_fixtures_exact_and_within_tolerance() {
    for (name, text) in GOLDENS {
        let fixture = load_golden(name, text);
        let actual = fixture.reference_output().expect("oracle must evaluate");
        // Stronger than the frozen contract: exact equality (deterministic
        // oracle over exactly-representable fixture values).
        assert_eq!(&actual, &fixture.expected, "fixture {name}: exact mismatch");
        assert_eq!(actual.len(), fixture.expected_shape.iter().product::<u64>() as usize);
        fixture.check(&actual).unwrap_or_else(|e| panic!("fixture {name}: {e}"));
    }
}

/// T01-golden-negative — a perturbed result outside the frozen tolerance is
/// rejected, proving the checks are not vacuous. Perturbation of 1e-3 on a
/// value of 11.0 exceeds both abs 1e-5 and rel 1e-4 * 11 = 1.1e-3... it
/// does not, so use 5e-3: exceeds abs and rel (5e-3 > 1.1e-3).
#[test]
fn t01_golden_perturbation_rejected() {
    let (_, text) = GOLDENS[2];
    let fixture = load_golden("golden_t01_matmul_3x2", text);
    let mut perturbed = fixture.expected.clone();
    perturbed[0] += 5e-3; // |d| = 5e-3 > max(1e-5, 1e-4 * 11.0)
    assert!(fixture.check(&perturbed).is_err());
    // And a perturbation inside tolerance (1e-5 <= 1e-4 * 11.0) is accepted.
    let mut tiny = fixture.expected.clone();
    tiny[0] += 1e-5;
    assert!(fixture.check(&tiny).is_ok());
}

/// IE-X01 INT8 exactness vector: byte-exact round-trip of an I8 TensorData
/// through the canonical codec, including quant_scale / zero_point fields
/// (exactness required only for identical declared rounding/saturation
/// semantics; here the bytes are preserved verbatim).
#[test]
fn ie_x01_int8_tensor_byte_exact_roundtrip() {
    let desc = TensorDesc {
        dtype: DType::I8,
        dims: vec![2, 4],
        quant_scale: Some(0.0625),     // exact in f32
        quant_zero_point: Some(-3),
    };
    let data: Vec<u8> = vec![0, 1, 127, 128, 255, 129, 42, 7]; // raw i8 bit patterns
    let t = TensorData::new(desc, data.clone()).expect("i8 tensor must validate");
    assert_eq!(t.desc.quant_scale, Some(0.0625));
    assert_eq!(t.desc.quant_zero_point, Some(-3));

    let mut buf = Vec::new();
    encode_tensor(&mut buf, &t).expect("encode must succeed");
    let mut r = ie_contract::codec::Reader::new(&buf);
    let back = decode_tensor(&mut r).expect("decode must succeed");
    assert_eq!(back.desc.dtype, DType::I8);
    assert_eq!(back.desc.dims, vec![2, 4]);
    assert_eq!(back.desc.quant_scale, t.desc.quant_scale);
    assert_eq!(back.desc.quant_zero_point, t.desc.quant_zero_point);
    // Byte-exactness of the payload.
    assert_eq!(back.data, data);
}
