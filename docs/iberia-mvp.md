# Iberia local CPU MVP

The MVP gives a Rust application an in-process CPU endpoint for a small, statically shaped FP32 graph. An application registers a graph, submits tensors against its SHA-256 digest, and polls for the result. The first reference graph is an affine layer using matrix multiplication followed by addition.

## Roadmap

1. **Validate the CPU primitives.** Check tensor descriptors, input lengths, output size, and work before allocation. Preserve the FP32 golden outputs for `add` and `matmul`.
2. **Pin and run local graphs.** Register a bounded graph of `add` and `matmul` nodes by digest. Reject invalid graph references and input shapes. Bound registered models, queued requests, retained results, tensor payload bytes, and operations. Enforce receiver-monotonic deadlines, expose queued cancellation, and report measured queue and execution time.
3. **Demonstrate integration.** Provide a runnable Rust example that executes an affine layer through the public API and preserves its baseline output when a requested model is unavailable. Exercise the same API in regression tests.
4. **Verify the MVP.** Run the six Iberia/España regression packages and require clean formatting and strict Clippy on the local Iberia crate. Record the commands and results below.

The local API is independent of leases, discovery, and transport. The frozen `ie-contract` codec stays intact. ONNX import, signed model packages, quantized execution, accelerators, España delegation, and Goya compiler integration are follow-on work.

## Run the reference app

```sh
cargo run --locked -p ie-cpu --example iberia_local
```

The example uses synthetic FP32 inputs and prints the expected affine result `[14.0, 15.0]`, measured queue and execution times, and the preserved baseline `[1.0, 2.0]` after a typed `MODEL_MISSING` response.

For an embedding application, compile a `Graph` from input shapes and ordered nodes, register it with `local::Runtime`, and use the returned digest to submit a request. Drive the queue with `run_one` on the application's chosen worker thread and retrieve the completion with `poll`. The digest covers the graph operations, references, and input shapes. Weights and other tensor values supplied at execution time are request inputs.

The executor runs one job at a time. Queued jobs can be cancelled, and a running graph checks its elapsed deadline periodically inside each kernel. By default, a runtime admits up to 16 graphs, eight queued requests, and eight retained completions. Each job is limited to 8 MiB of tensor payload across its inputs, intermediate values, and output, and 16,777,216 scalar operations. A retrieved or evicted result is unavailable on subsequent polls.
