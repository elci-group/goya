# Iberia local CPU MVP

The MVP gives a Rust application an in-process CPU endpoint for a small, statically shaped FP32 graph. An application registers a graph, submits tensors against its SHA-256 digest, and polls for the result. The first reference graph is an affine layer using matrix multiplication followed by addition.

## Roadmap

1. **Validate the CPU primitives.** Check tensor descriptors, input lengths, output size, and work before allocation. Preserve the FP32 golden outputs for `add` and `matmul`.
2. **Pin and run local graphs.** Register a bounded graph of `add` and `matmul` nodes by digest. Reject invalid graph references and input shapes. Bound registered models, queued requests, retained results, tensor payload bytes, and operations. Enforce receiver-monotonic deadlines, expose queued cancellation, and report measured queue and execution time.
3. **Demonstrate integration.** Provide a runnable Rust example that executes an affine layer through the public API and preserves its baseline output when a requested model is unavailable. Exercise the same API in regression tests.
4. **Verify the MVP.** Run the six Iberia/España regression packages and require clean formatting and strict Clippy on the local Iberia crate. Record the commands and results below.

The local API is independent of leases, discovery, and transport. The frozen `ie-contract` codec stays intact. ONNX import, signed model packages, quantized execution, accelerators, España delegation, and Goya compiler integration are follow-on work.
