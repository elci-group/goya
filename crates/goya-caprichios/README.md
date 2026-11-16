# Network Caprichio prototype

The `catalog` module contains 80 versioned draft capability definitions. Each
records a proposed contract identifier, intent, operations and invariants.
These are design-stage entries, not 80 implementations, conformance claims or
promises of platform support. The existing Network and Storage prototypes have
their own separate implementation APIs and qualification limits below. Use
`catalog::definitions()` to enumerate entries or `catalog::find(id)` to look up
an exact identifier.

The crate also has experimental process-local building blocks in
`compute_runtime`, `data_runtime`, and `memory_runtime`: a bounded queue,
retry policy and progress counter; an FNV-1a checksum and bounded byte envelope;
and an owner-scoped bounded byte arena. They are runtime components, not yet
implementations of the broader catalog entries. Narrow process-local memory,
bounded-queue and binary-envelope construction recipes have their own typed
declarations, target negotiation and generated Rust bindings. They do not prove
the broader draft contracts or platform portability. FNV-1a is not a
cryptographic hash or authentication mechanism. Their ignored unit benchmarks
can be run with `cargo test --release -p goya-caprichios --lib benchmark_ -- --ignored --nocapture --test-threads=1`;
the results are local microbenchmarks rather than platform conformance evidence.
Other narrow process-local recipes provide capped retries, fixed-total progress,
a bounded chunk stream, a bounded batcher, cooperative monotonic deadlines, a
bounded RLE codec, a bounded UTF-8 CSV dialect, and a bounded ASCII-set
validator. The stream is not an OS pipe; deadlines do not preempt work; RLE can
expand data and provides no integrity or authentication guarantee; the CSV
recipe does not claim general CSV compatibility; the validator checks length
and membership in an explicit ASCII byte set only.

`contract_validation` audits draft catalog metadata. The experimental
`registry` negotiates explicit implemented contract IDs; validation of a
catalog entry is not proof of an implementation. In particular the broad
draft HTTP contract is not fulfilled by the narrower HTTPS GET recipe.
`registry::supported_contracts()` lists the exact contracts with lowering
recipes; it does not claim that the 80 catalog drafts are implemented.
Separate narrow recipes include process-local bounded file reads and metadata,
and browser GET/POST status and CORS-visible response headers. File access
uses caller paths and OS permissions without root isolation; the HTTP recipe
does not return a response body and remains subject to browser restrictions.

The crate also contains an early `storage` contract module. It normalizes a
namespaced, size-bounded, best-effort key/value declaration and emits a browser
IndexedDB module when the caller reports IndexedDB support. Its `get`, `put`
and `deleteKey` operations work with string keys and byte values and wait for
transaction completion. Durability, multi-key transactions, encryption at rest,
Node and native targets are rejected. Maja exposes checked construction, but
does not yet integrate the adapter into application builds or its bundle format.
IndexedDB availability, quota, persistence and freedom from eviction are not
guaranteed by target negotiation. A Chrome loopback test exercises real
IndexedDB operations, namespace isolation and transaction completion; it does
not qualify other browsers, private browsing, quota failures or eviction.

This crate defines a typed construction boundary for one HTTPS GET.
A declaration normalizes into canonical network IR, selects a versioned browser
or Node.js Fetch recipe, and emits an ES module with its contract, recipe and
canonical inputs. Maja exposes a construction API; this does not change Goya
syntax or enable network effects in application builds.

`lower_checked` and Maja's `construct_network_checked` reject a target profile
that lacks Fetch, AbortController, URL, Uint8Array or timers. Profiles are
provided by the caller rather than probed. The original `lower` and
`construct_network` APIs assume those facilities are present; use the checked
forms when the target profile is not already established.

The emitted `get(url)` returns a status and response bytes. It rejects non-HTTPS
URLs and redirects, makes no retries or offline guarantees, and limits active
requests per module instance. Its timeout starts after admission and covers
response headers and body consumption; waiting for a slot is unbounded. HTTP
error statuses are returned normally. Browser CORS, CSP, network permissions,
host Fetch behavior and operating-system policies still apply. Node requires
global Fetch, URL and AbortController. Bodies and the admission queue are
unbounded; this prototype is unsuitable for untrusted production workloads.

The recipes share code but identify their host separately. Their listing is
intended support, not proof of conformance. A shared controlled-Fetch behavior
test runs in Bun and Chrome, including body-timeout and concurrency checks.
A separate Chrome loopback HTTPS integration test covers responses, HTTP error
statuses, redirect rejection, timeout and CORS allow/deny behavior. It uses a
temporary self-signed certificate with Chrome certificate checks disabled for
positive cases. A separate fresh Chrome profile confirms that the same untrusted
certificate is rejected without that bypass. This does not establish trusted
certificate success, production TLS, other browsers, or Node conformance.
Maja exposes an experimental construction API with SHA-256 hashes of the source
and canonical IR and a canonical provenance record. The record is a snapshot,
not a signed attestation, and does not pin or record the build toolchain. Maja's
`verified_bundle` reconstructs the current browser recipe from canonical IR and
checks all output and provenance fields before returning three ordered in-memory
entries: `network.mjs`, `network.ir`, and `provenance.txt`. This checks consistency
with the linked compiler, not the authenticity of the compiler or its inputs.
`write_bundle` writes those entries to a newly created directory and refuses to
overwrite an existing path. `verify_bundle_directory` independently reconstructs
the recipe and checks the exact files and their bytes. Bundle verification assumes
the directory is not being modified concurrently, and directory publication is
not atomic. Output bytes are deterministic for identical linked compiler and IR;
filesystem metadata and pinned-toolchain reproducibility are not covered.
The API does not enable network effects in Goya applications.
There is no pinned toolchain, ABI verification, native platform adapter, or
reproducible binary packaging yet. Determinism is limited to emitted source
and canonical IR from the same crate version and inputs. Runtime delivery and
scheduling are not deterministic. Retries and offline queuing fail normalization.

Run `cargo test --manifest-path crates/goya-caprichios/Cargo.toml`.
Run the controlled-host suite with `cargo test --manifest-path crates/goya-caprichios/Cargo.toml --test fetch_contract -- --ignored` (requires Bun).
Run the same suite in Chrome with `cargo test -p goya-caprichios --test browser_contract -- --ignored` (requires Chrome and `bank`).
Run the loopback HTTPS integration with `cargo test -p goya-caprichios --test https_contract -- --ignored` (requires Chrome, OpenSSL, `bank`, Python `websocket-client`, and loopback access).
Run the CORS-limited HTTP recipe check with `cargo test -p goya-caprichios --test http_contract -- --ignored` (requires Chrome, rustc, OpenSSL, Python `websocket-client`, and loopback access; the test bypasses certificate checks for its local self-signed server).
Run the IndexedDB integration with `cargo test -p goya-caprichios --test storage_contract -- --ignored` (requires Chrome, `bank`, Python `websocket-client`, and loopback access).
Measure Chrome IndexedDB put/get latency with `cargo test --release -p goya-caprichios --test storage_benchmark -- --ignored --nocapture`. It uses an isolated profile, a loopback origin and fixed-size payloads; it is a smoke benchmark, not a durability or cross-browser qualification.
Compare generated reads with a matching direct IndexedDB reference using `cargo test --release -p goya-caprichios --test storage_reference_benchmark -- --ignored --nocapture`.
Measure the generated browser GET adapter against local HTTPS with `cargo test --release -p goya-caprichios --test network_benchmark -- --ignored --nocapture`. The isolated benchmark browser bypasses certificate checks for its self-signed loopback server. See [BENCHMARKS.md](BENCHMARKS.md) for sample results and limitations.
Measure compiler-side normalization and source emission with `cargo run --release -p goya-caprichios --example construction_benchmark -- 100000`. This is a local microbenchmark, not a runtime network or storage throughput test; compare measurements only with toolchain and environment recorded.
