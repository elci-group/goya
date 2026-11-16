# Network Caprichio prototype

This crate defines a typed construction boundary for one HTTPS GET.
A declaration normalizes into canonical network IR, selects a versioned browser
or Node.js Fetch recipe, and emits an ES module with its contract, recipe and
canonical inputs. Maja exposes a construction API; this does not change Goya
syntax or enable network effects in application builds.

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
