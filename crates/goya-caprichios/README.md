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
draft HTTP contract is not fulfilled by the narrower HTTPS GET recipe, and the
broad draft `security.input_validation` contract is not fulfilled by the
narrower process-local ASCII set-and-length recipe.
`registry::supported_contracts()` lists the exact contracts with lowering
recipes; it does not claim that the 80 catalog drafts are implemented. A
draft's proposed `goya.<id>.v1` string is not reused by a narrower recipe.
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

The hardware caprichios (Touch ID, Face ID, Windows Hello, Apple Pay, Google
Pay, Samsung Pay) and the Blue Moon Bluetooth contracts follow the same shape:
a typed declaration normalizes into canonical IR, negotiation fails closed on
every unreported host facility, and the artifact is a generated Rust factory
that constructs a process-local runtime (`apple_runtime`, `bluetooth_runtime`).
They do not call platform frameworks, drive hardware, or prove that any native
stack performed an operation; hosts on real hardware report their real
facilities through runtime capabilities, and constructed objects prove nothing
about sensors, payment networks or peripherals.

The mobile-SoC neural-acceleration caprichios follow the accelerator-catalog
shape: `snapdragon_accelerator_caprichio`
(`goya.hardware.neural_accel.snapdragon.v1`) catalogs published Hexagon/NPU
facts for 55 Snapdragon generations across the 8, 7, 6 and 4 series;
`exynos_accelerator_caprichio` (`goya.hardware.neural_accel.exynos.v1`)
catalogs the contemporary Samsung flagship generations;
`mediatek_accelerator_caprichio` (`goya.hardware.neural_accel.mediatek.v1`)
catalogs Dimensity 700/800/900-series parts; `tensor_accelerator_caprichio`
(`goya.hardware.neural_accel.tensor.v1`) catalogs Google Tensor G1 through G5;
and `kirin_accelerator_caprichio` (`goya.hardware.neural_accel.kirin.v1`)
catalogs Huawei Kirin 970 through 9020. Each entry records
only figures the vendor published (TOPS figures are `None` where the vendor
published none, in tenths of TOPS where it did); conflicting Hexagon version
sources are recorded in the label rather than guessed. A declaration names a
generation from the catalog and bounds layers, batch and weight bytes;
normalization builds a checked delegate plan in the vendor's `*_runtime`
module; lowering fails closed unless the host reports the vendor DSP/NPU, a
neural delegate and an Int8 model. The artifacts are
plan factories only: no model is compiled, no DSP is contacted and no
inference runs.

`nvidia_accelerator_caprichio`
(`goya.hardware.gpu_compute.nvidia_cuda.v1`) applies the same catalog shape
to NVIDIA GPUs: GTX 10/16/20-series (Pascal/Turing) and RTX 30/40-series
(Ampere/Ada Lovelace) SKUs plus integrated Blackwell parts, each with
published chip, architecture, compute capability, core and VRAM facts. A
declaration names a GPU from the catalog and bounds threads-per-block,
blocks and per-block shared memory; normalization builds a checked
`CudaLaunchPlan` in `nvidia_runtime`; lowering fails closed unless the host
reports a CUDA device, the CUDA runtime API and an Int8-quantized kernel
graph, and GPUs without Tensor Cores reject Int8 tensor delegation. The
artifact is a launch-plan factory only: no kernel is compiled and no GPU
work is launched.

The AMD CPU-compute caprichios apply the same catalog shape to x86-64 CPU
generations: `amd_desktop_caprichio`
(`goya.hardware.cpu_compute.amd_desktop.v1`) catalogs 11 socketed desktop
families from Phenom II (2008) through Ryzen 9000 (2024);
`amd_apu_caprichio` (`goya.hardware.cpu_compute.amd_apu.v1`) catalogs 21 APU
families from Llano (2011) through Strix Halo (2025), with integrated-GPU
compute-unit ranges recorded only where AMD published them;
`amd_threadripper_caprichio` (`goya.hardware.cpu_compute.amd_threadripper.v1`)
catalogs 8 HEDT generations from Threadripper 1000 through Threadripper Pro
9000WX; and `amd_epyc_caprichio` (`goya.hardware.cpu_compute.amd_epyc.v1`)
catalogs 8 server generations from EPYC 7001 (Naples) through EPYC 9005
(Turin). Each entry records only silicon-documented facts (sockets, core
counts, thread technology, ISA support, process node). A declaration names a
generation from the catalog and bounds worker threads, batch size and
workspace bytes against a chosen ISA level (SSE3, AVX, AVX2 or AVX-512);
normalization rejects worker-thread counts above the generation's documented
hardware thread count (`ThreadsExceedGeneration`). Lowering fails closed
unless the host reports x86-64 execution with an OS thread API, the declared
ISA's host facility, UTF-8 strings and silicon-documented ISA support for the
named generation. The artifact is a generated worker-plan factory only: no
threads spawn, no SIMD executes and nothing proves a CPU ran work.

`bluetooth_caprichio` (`goya.hardware.bluetooth.central.v1`, recipe
`apple-ble-central-bounded-scan`) covers BLE central discovery and connection
lifecycle accounting for iOS and Apple Silicon Mac targets. Scans are
restricted to one to eight declared service UUID filters; the tracked
peripheral set is bounded; detected, recognised, connected, operational and
disconnected are distinct states, and only filter-matched peripherals connect;
reconnection is bounded with deterministic exponential backoff from 100 ms,
saturating at 5000 ms; RSSI observations are validated to the Bluetooth
−127..0 dBm range; duplicates are suppressed.

`bluetooth_control_caprichio` (`goya.hardware.bluetooth.gatt.v1`, recipe
`apple-ble-gatt-bounded-commands`) covers GATT command accounting against
profile-defined characteristic policies (permitted operations and value
bounds). Reads are planned; writes run in acknowledged or unacknowledged
modes with a bounded pending-acknowledgement ledger resolved only through
explicit acknowledgements or cancellations; observed values are validated
against the characteristic bound. A planned or acknowledged write is
transport accounting only and is never reported as physical execution.

`bluetooth_stream_caprichio` (`goya.hardware.bluetooth.stream.v1`, recipe
`apple-ble-stream-bounded-notifications`) covers characteristic-notification
intake with fixed per-characteristic transports. The buffer is bounded and
applies fail-closed backpressure: over-full intake is rejected with drop
accounting, never silent eviction; over-limit payloads fail closed;
subscriptions cancel deterministically, discarding buffered items separately
from drops; only indications create pending confirmations, and notifications
never carry one; ordinals make intake order observable. Delivery is never
guaranteed: notifications are unacknowledged at the attribute layer, and the
canonical IR states `delivery_guarantee=none`.

`bluetooth_guard_caprichio` (`goya.hardware.bluetooth.guard.v1`, recipe
`apple-ble-guard-bounded-authorisation`) covers the application-level
security contract. Connection decisions fail closed on undetermined or
denied permissions and on any peripheral the application did not explicitly
select; authorisation is idempotent explicit selection with a bounded
ledger; privacy usage descriptions that are empty, generic or placeholder
text are rejected at normalization and explicit selection cannot be
disabled; legacy advertisement payloads are bounded at 31 bytes; granted
and denied decisions are both counted for audit. No identity is verified
and no usage-description string is generated.

`bluetooth_continuity_caprichio` (`goya.hardware.bluetooth.continuity.v1`,
recipe `apple-ble-continuity-bounded-restoration`) covers lifecycle
restoration accounting. A bounded ledger records user-approved associations
with restoration intent and compatible subscriptions; Bluetooth-disabled and
app-suspended transitions suspend active associations; restoration is
*planned* from the persisted record and is never an outcome; reconciliation
restores a suspended association only for an observed device; the
association set is bounded. Cross-launch persistence requires state-
restoration and persistent-storage facilities only when the declaration
asks for it, and the canonical IR states `relaunch_promised=none`: Apple's
Bluetooth state restoration is conditional and no relaunch is promised.

All five do not drive a radio, subscribe to a real peripheral, perform real
GATT operations, advertise, persist anything, restore state, or generate
entitlements or usage descriptions. A constructed session is an accounting
model, not evidence about any Bluetooth adapter. See
[docs/blue-moon/BLUE-MOON-1.md](../../docs/blue-moon/BLUE-MOON-1.md),
[docs/blue-moon/BLUE-MOON-2.md](../../docs/blue-moon/BLUE-MOON-2.md),
[docs/blue-moon/BLUE-MOON-3.md](../../docs/blue-moon/BLUE-MOON-3.md),
[docs/blue-moon/BLUE-MOON-4.md](../../docs/blue-moon/BLUE-MOON-4.md),
[docs/blue-moon/BLUE-MOON-5.md](../../docs/blue-moon/BLUE-MOON-5.md) and
[docs/blue-moon/roadmap.md](../../docs/blue-moon/roadmap.md).

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
