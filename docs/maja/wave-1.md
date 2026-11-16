# Wave 1 implementation status

M1 is **blocked and not complete**. The current Frampton checkout does not establish a browser presentation interface, and no browser-rendered application exists. M0's complete semantic profile, binding decisions and full CI baseline remain unresolved; see [Wave 0](wave-0.md) and [ADR-001](adr-001-frampton.md). The following is a fail-closed skeleton, not a static web compiler or an approved production profile.

* `maja-semantic` calls the canonical strict GSSH/1 frontend for inspection, exposes the admitted handoff and structured source diagnostics, and rejects every web admission. Its draft IR marker does not implement lossless lowering or a serialized contract. A subsequent Wave 2 change retains the original UTF-8 bytes alongside the handoff and checks its source-map digests and ranges.
* `maja-frampton` provides a typed adapter boundary whose current implementation always returns an unavailable error and has an uninhabited output. It does not implement Frampton mapping.
* `maja` exposes `inspect`, `check`, and `build`. Inspect reports strict source recognition only. Check and build reject the unqualified profile; failed build does not create an output directory. Diagnostic codes and byte ranges are printed as text, not a finalized machine-readable service protocol. Other CLI commands are not implemented.
* `tests/maja` contains source fixtures and a black-box fail-closed suite. These tests do not claim browser execution, accessibility, security qualification, output determinism or D1 parity.

Verification: `cargo test --offline -p maja -p maja-semantic -p maja-frampton` passed five Rust tests; `cargo clippy --offline -p maja -p maja-semantic -p maja-frampton --all-targets --no-deps -- -D warnings` passed. After `cargo build --offline -p maja`, `MAJA_BIN=/home/sal/goya/target/debug/maja PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/maja -v` passed three black-box tests, including repeated rejection and no published files. These local tests are not independent release certification. Existing full-workspace failures are recorded in [baseline](baseline.md).

`uni --fail-under 80 .` on the shared working tree exited 1 with 73/100 and reported incomplete checks; it is not a passing quality or security gate.

HTML/CSS emission, browser state/event execution, browser host contracts, packaging, source maps and real-browser rendering remain outstanding. M1 requires approved Frampton browser integration, a ratified semantic profile, a static view through that integration and real-browser evidence. It must not be reported green because the fail-closed skeleton compiles.
