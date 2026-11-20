# Iberia local CPU MVP

The MVP gives a Rust application an in-process CPU endpoint for a small, statically shaped FP32 graph. An application registers a graph, submits tensors against its SHA-256 digest, and polls for the result. The first reference graph is an affine layer using matrix multiplication followed by addition.

## Roadmap

1. **Validate the CPU primitives.** Check tensor descriptors, input lengths, output size, and work before allocation. Preserve the FP32 golden outputs for `add` and `matmul`.
2. **Pin and run local graphs.** Register a bounded graph of `add` and `matmul` nodes by digest. Reject invalid graph references and input shapes. Bound registered models, queued requests, retained results, tensor payload bytes, and operations. Enforce receiver-monotonic deadlines, expose queued cancellation, and report measured queue and execution time.
3. **Demonstrate integration.** Provide a runnable Rust example that executes an affine layer through the public API and preserves its baseline output when a requested model is unavailable. Exercise the same API in regression tests.
4. **Verify the MVP.** Run the six Iberia/España regression packages and require clean formatting and strict Clippy on the local Iberia crate. Record the commands and results below.

The local API is independent of leases, discovery, and transport. The frozen `ie-contract` codec stays intact. Iberia accepts pinned local JSON packages and a restricted ONNX profile. Package signing, quantized execution, accelerators, España delegation, and Goya compiler integration are follow-on work.

## Run the reference app

```sh
cargo run --locked -p ie-cpu --example iberia_local
```

The example uses synthetic FP32 inputs and prints the expected affine result `[14.0, 15.0]`, measured queue and execution times, and the preserved baseline `[1.0, 2.0]` after a typed `MODEL_MISSING` response.

For an embedding application, compile a `Graph` from input shapes and ordered nodes, register it with `local::Runtime`, and use the returned digest to submit a request. Drive the queue with `run_one` on the application's chosen worker thread and retrieve the completion with `poll`. The digest covers the graph operations, references, and input shapes. Weights and other tensor values supplied at execution time are request inputs.

The executor runs one job at a time. Queued jobs can be cancelled, and a running graph checks its elapsed deadline periodically inside each kernel. By default, a runtime admits up to 16 graphs, eight queued requests, and eight retained completions. Each job is limited to 8 MiB of tensor payload across its inputs, intermediate values, and output, and 16,777,216 work units. An elementwise addition or a matrix multiply-accumulate counts as one unit. A retrieved or evicted result is unavailable on subsequent polls.

## Load a packaged model

A v1 Iberia package is a JSON file up to 1 MiB. It names the input shapes and graph nodes, and can bind fixed row-major FP32 values to any input slot. An input with no `values` field is supplied at inference time. Each node uses `add` or `matmul` and refers to earlier values by index. The final node supplies the output.

Give `ModelPackage::load` the artifact and an expected SHA-256 digest from trusted application configuration. The loader checks the exact file bytes before parsing. For example, reformatting the JSON produces a different digest. Register the verified package with `Runtime::register_package`, then submit its digest with the unbound input tensors in slot order. The runtime supplies the package's fixed tensors and applies the existing job budgets.

Run the checked-in affine package with its pinned digest.

```sh
cargo run --locked -p ie-cpu --example iberia_package -- \
  crates/ie-cpu/tests/fixtures/affine.iberia.json \
  98ed1c2233a350010a51ca86139247f793a9f251845f559c5a49816327cde191 1 2
```

The command prints `[14.0, 15.0]`. Changing the inputs to `0 0` prints `[1.0, -1.0]`. An incorrect digest fails with `POLICY_DENIED`.

## Import ONNX

Use `ModelPackage::load_onnx` with the expected SHA-256 digest of the original `.onnx` artifact. Iberia applies the same 1 MiB limit and verifies the digest before decoding. The resulting package registers with the same local runtime, and embedded initializers fill fixed slots.

The importer targets ONNX IR 8–10 and the standard domain at opset 13. It supports statically shaped FP32 values, embedded dense FP32 initializers, equal-shape `Add`, and rank-2 `MatMul`. The graph can use up to 16 inputs and initializers combined and up to 32 nodes. Its last node supplies the single declared output. Broadcasting, symbolic dimensions, external or sparse tensors, and custom operators are outside this profile. [ONNX schema](https://github.com/onnx/onnx/blob/v1.16.2/onnx/onnx.proto) · [Add](https://onnx.ai/onnx/operators/onnx__Add.html) · [MatMul](https://onnx.ai/onnx/operators/onnx__MatMul.html)

```sh
cargo run --locked -p ie-cpu --example iberia_package -- --onnx \
  crates/ie-cpu/tests/fixtures/affine.onnx \
  9b7131e35fe1280244d2dc4c97e6b8d3a460583291eb0e1361e916c4c3215fca 1 2
```

This model produces the same `[14.0, 15.0]` result as the JSON fixture. The checked-in artifact was generated and validated with the ONNX 1.16.2 Python API.

## Verification

The six-package regression run passed 125 tests on Linux with Rust 1.95.0. The Unix socket tests require permission to bind local sockets. Formatting and strict Clippy passed for `ie-cpu`. Both ONNX inputs matched the official reference evaluator, an incorrect ONNX digest was rejected, and the JSON example still passed. The initial MVP's broader `uni` analysis scored 73/100 against a threshold of 80 and reported incomplete analyzer results.

```sh
cargo test --locked --lib --tests -p ie-contract -p ie-cpu -p ie-loopback -p ie-uds -p ie-gate -p ie-conformance
cargo fmt --check -p ie-cpu
cargo clippy --no-deps --locked -p ie-cpu --all-targets -- -D warnings
```
