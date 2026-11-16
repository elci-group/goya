# Baseline evidence

Goya revision `38c2f6132ef532e5b9b58b6a017e1f03cc1b3f5a`. Initial targeted tests were run in the existing dirty checkout, which contains unrelated Android changes. `cargo test --locked -p goya-lang -p goya-state -p goya-ir --offline` passed 83, 6 and 6 unit tests, plus doc tests. Independent reconnaissance ran `cargo test --offline -p goya-lang -p goya-state -p goya-executable --lib`: 83, 6 and 39 tests passed. These commands are not full CI and the checkout is not a clean-revision baseline.

`cargo fmt --all -- --check` in that dirty checkout failed with existing differences including `goya-lang/src/lib.rs`, `transition.rs` and `goya-window-management`. Do not reformat unrelated files to make this report green. The CI workflow calls `cargo build --workspace`, `cargo test --workspace --lib --tests`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`; its `uni` step is non-blocking. Full results must be recorded separately against a controlled checkout. Cargo 1.95.0 and Google Chrome 157.0.8103.78 were observed. A Maja browser harness and artifact do not yet exist.

## Clean revision CI baseline

An independent worker extracted an archive of the exact HEAD above into `/tmp/maja-ci-baseline-38c2f613`, excluding all uncommitted changes, and ran the four CI gates independently. Offline mode was added for the first three commands; this environment is not the hosted CI runner.

| Command | Result | Evidence |
|---|---|---|
| `cargo build --workspace --offline` | FAIL, exit 101 | Linker cannot find `-lxkbcommon`; `/tmp/maja-ci-baseline-build.log`. |
| `cargo test --workspace --lib --tests --offline` | FAIL, exit 101 | Same missing native library, including `goya-document`; `/tmp/maja-ci-baseline-test.log`. No claim that full tests passed. |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | FAIL, exit 101 | Existing `clippy::inherent_to_string` for `AuthoredIdError` in `crates/goya-lang/src/parser.rs:400`; `/tmp/maja-ci-baseline-clippy.log`. |
| `cargo fmt --all -- --check` | FAIL, exit 1 | Committed formatting differences include `goya-lang/src/lib.rs` and `transition.rs`; `/tmp/maja-ci-baseline-fmt.log`. |

The `/tmp` logs are local, ephemeral evidence, not a durable release bundle. A successful run on the actual CI environment and resolution of independently reproduced source failures remain open; these failures were not introduced or repaired by Wave 0.

The inherited local development gate `uni --fail-under 80 .` ran in the dirty working tree and exited 1 with a reported 73/100 and incomplete checks. Its output reports a Poka prerequisite error for unknown tool `traci` and a RustSec cache lock preventing advisory refresh. This is separate from the clean-revision CI command results above; it is not a complete security assessment.
