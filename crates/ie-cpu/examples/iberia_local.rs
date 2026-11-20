//! Small, deterministic Iberia local CPU demo.
//!
//! Run with `cargo run -p ie-cpu --example iberia_local`. The graph is an
//! affine layer (MatMul followed by Add); its inputs and expected output are
//! synthetic and are included here so developers can check the complete
//! local request path without a peer, network, credentials, or model files.

use std::time::Duration;

use ie_contract::fixture::{GoldenFixture, GoldenOp};
use ie_contract::tensors::TensorData;
use ie_cpu::graph::{Graph, Node};
use ie_cpu::local::{Limits, Runtime};

fn values(tensor: &TensorData) -> Vec<f32> {
    tensor
        .data
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("four-byte FP32 element")))
        .collect()
}

fn main() {
    // input [1, 2], weights [2, 2], bias [1, 2]
    // [1, 2] × [[3, 4], [5, 6]] + [1, -1] = [14, 15].
    let input = GoldenFixture::tensor(&[1.0, 2.0], &[1, 2]);
    let weights = GoldenFixture::tensor(&[3.0, 4.0, 5.0, 6.0], &[2, 2]);
    let bias = GoldenFixture::tensor(&[1.0, -1.0], &[1, 2]);
    let graph = Graph::compile(
        &[&[1, 2], &[2, 2], &[1, 2]],
        &[
            Node {
                op: GoldenOp::MatMul,
                left: 0,
                right: 1,
            },
            Node {
                op: GoldenOp::Add,
                left: 3,
                right: 2,
            },
        ],
    )
    .expect("valid affine graph");

    let mut runtime = Runtime::new(Limits::default());
    let model = runtime.register(graph).expect("register local graph");
    println!("Iberia local CPU demo");
    println!("model: {}", hex_digest(&model));
    println!("graph: affine layer (MatMul + Add), synthetic FP32 inputs");

    let request = runtime
        .submit(
            model,
            vec![input.clone(), weights.clone(), bias.clone()],
            Duration::from_secs(2),
        )
        .expect("submit local inference");
    let completed = runtime.run_one();
    assert_eq!(
        completed,
        Some(request),
        "the single queued request should run"
    );
    let completion = runtime
        .poll(request)
        .expect("poll local inference")
        .expect("completed request should be available");

    match completion.result {
        Ok(output) => {
            let actual = values(&output);
            let expected = [14.0, 15.0];
            assert_eq!(
                actual, expected,
                "affine output must match the hand-computed result"
            );
            println!("local result: {actual:?} (expected {expected:?})");
            println!(
                "queue time: {:.3} ms",
                completion.queue_time.as_secs_f64() * 1_000.0
            );
            println!(
                "execution time: {:.3} ms",
                completion.execution_time.as_secs_f64() * 1_000.0
            );
        }
        Err(error) => panic!("local inference failed: {error:?}"),
    }

    // Exercise graceful baseline behavior when this model is unavailable.
    // The app keeps its original input and reports the typed reason rather
    // than replacing the baseline with a fabricated neural result.
    let missing = [0xFF; 32];
    match runtime.submit(
        missing,
        vec![input.clone(), weights, bias],
        Duration::from_secs(2),
    ) {
        Err(error) if error.name() == "MODEL_MISSING" => {
            println!("optional feature unavailable: {}", error.name());
            println!("baseline fallback: preserve input {:?}", values(&input));
        }
        Err(error) => panic!("unexpected typed error: {error:?}"),
        Ok(_) => panic!("unregistered model unexpectedly accepted"),
    }
}

fn hex_digest(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
