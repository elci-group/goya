# Caprichios benchmark notes

These are single local smoke measurements, not regression thresholds,
cross-platform comparisons, conformance evidence, or measurements of all 80
draft catalog entries. Results change with machine load and environment.

Recorded 2026-11-16 on Linux x86_64, rustc 1.95.0 (59807616e 2026-04-14),
Chrome 157.0.8103.78. Rust tests used the release profile and one test thread.

Catalog validation, queue factory construction, memory factory construction,
progress factory construction, retry factory construction, and mixed registry
negotiation were sampled again after the catalog audit started rejecting
repeated operation and invariant text and after four recipe contract
identifiers grew longer. Those six rows are that sample. The other rows are
the earlier sample from the same machine and day.

`cargo test --offline --release -p goya-caprichios --lib benchmark_ -- --ignored --nocapture --test-threads=1`

| Workload | Iterations | Elapsed |
| --- | ---: | ---: |
| Queue factory construction | 100,000 | 28.90 ms |
| Queue push/pop round trip | 1,000,000 | 32.51 ms |
| Catalog metadata validation (80 drafts) | 10,000 | 346.76 ms |
| Bounded byte-envelope recipe construction | 100,000 | 17.90 ms |
| Byte-envelope encode/decode, 1 MiB payload | 100 | 254.60 ms |
| Bounded file-read adapter construction | 100,000 | 33.29 ms |
| Bounded file reads, 16 KiB | 10,000 | 52.48 ms |
| CORS-limited HTTP recipe construction | 100,000 | 84.85 ms |
| Memory factory construction | 100,000 | 26.73 ms |
| Memory allocate/write/read/release, 64 KiB | 10,000 | 57.50 ms |
| Progress factory construction | 100,000 | 18.11 ms |
| Retry factory construction | 100,000 | 18.35 ms |
| FNV-1a recipe construction | 100,000 | 24.89 ms |
| Mixed registry negotiation (14 narrow contracts) | 10,000 | 53.66 ms |
| Bounded stream factory construction | 100,000 | 23.57 ms |
| Bounded stream write/read, 1 KiB | 100,000 | 5.09 ms |
| RLE encode/decode, repeated 1 MiB | 20 | 16.19 ms |
| RLE encode/decode, literal-heavy 1 MiB | 20 | 38.32 ms |
| Batch factory construction | 100,000 | 26.25 ms |
| Batch add/flush, 128 items | 10,000 | 77.44 ms |
| Deadline construction and two checks | 100,000 | 8.71 ms |
| Bounded parallel callback execution | 10,000 | 1.41 ms |
| Priority queue, 32 enqueues and dequeues of 64 bytes | 1,000 | 3.52 ms |
| CSV encode/decode, 10,000 three-field records | 20 | 70.06 ms |
| ASCII set validation | 1,000,000 | 121.44 ms |

`cargo test --offline --release -p goya-caprichios --test storage_benchmark -- --ignored --nocapture`

In a separate Chrome run with a loopback origin, isolated profile, 10 warmups,
200 measured operations and 256-byte values, median committed put latency was
0.3 ms (p95 0.5 ms); median get latency was 0.3 ms (p95 0.4 ms). This is a
local smoke measurement and does not test persistence across restarts, quota,
eviction, native storage, other browsers, or any reference implementation.

`cargo test --offline --release -p goya-caprichios --test storage_reference_benchmark -- --ignored --nocapture`
compares generated IndexedDB reads with direct IndexedDB reads using the same
database, per-call open/close, transaction completion and copied 4 KiB value.
In one isolated Chrome run (50 warmups and 500 alternating samples per path),
both medians were 0.3 ms. p95 was 0.9 ms for the generated adapter and 1.0 ms
for the direct reference. These are descriptive
observations, not a claim of statistical equivalence or a performance threshold.

`cargo run --offline --release -p goya-caprichios --example construction_benchmark -- 100000`
measures compiler-side network and storage source construction only. One local
run measured 206.7 ns and 208.1 ns per construction respectively. It does not
measure network or storage throughput.

`cargo test --offline --release -p goya-caprichios --test network_benchmark -- --ignored --nocapture`
measured generated browser GET calls against a loopback HTTPS server, with an
isolated Chrome profile, self-signed certificate checks bypassed, 10 warmups,
200 sequential measured calls and 256-byte responses. One run measured median
3.8 ms and p95 6.0 ms through complete response-body consumption. This does not
measure production network, certificate validation, external latency or other
platforms.
