# Emperor CI

Workflow: `.github/workflows/emperor.yml` (`emperor-ci`).

It runs on push to `main` and on pull requests. Concurrency group `emperor-${{ github.ref }}` cancels an in-progress run for the same ref. The toolchain pin is `dtolnay/rust-toolchain@1.95.0`, the same channel as `rust-toolchain.toml`. The workflow requests no secrets and adds no third-party security scanner.

This file is separate from `.github/workflows/ci.yml`. That split is deliberate. Until `emperor` and `emperor-runtime` are workspace members, the compile and test jobs fail. That is the only accepted transition risk of keeping Emperor off the existing workflow. Do not paper over a missing package with `continue-on-error`, and do not weaken clippy `-D warnings`.

## Jobs that compile and test

These jobs fail the workflow when their command exits non-zero. None of them set `continue-on-error`.

| Job | Needs | What it runs |
| --- | --- | --- |
| `fmt-and-lint` | nothing | `cargo fmt` for `crates/emperor` and `crates/emperor-runtime`, then `cargo clippy -p emperor -p emperor-runtime --all-targets -- -D warnings` |
| `unit-and-property-tests` | `fmt-and-lint` | `cargo test -p emperor -p emperor-runtime --offline --lib --tests` |
| `frontend-contract-tests` | `unit-and-property-tests` | `cargo test -p emperor --offline --lib --tests` |
| `native-ir-validation` | `unit-and-property-tests` | `cargo test -p emperor-runtime --offline --lib` |
| `x86_64-elf-build` | `unit-and-property-tests` | `cargo build -p emperor --offline`, then the admission check below |

`frontend-contract-tests` is its own job even though it repeats the emperor package tests. Those package tests are the contract tests, including any oracle tests in the package. The job does not run a workspace test suite.

`x86_64-elf-build` installs nothing extra and does not require a Wayland display. The check step is named `admission check only; ELF launch is local`. It runs:

```sh
cargo run -p emperor --offline -- check crates/sash/fixtures/interactive-counter.gscene --target x86_64-unknown-linux-gnu --format json
```

That check is expected to exit 0 for an admitted scene. The job does not run `emperor build`. That build exits 3 when `WAYLAND_DISPLAY` is unset, so CI does not treat a missing display as a launch. ELF launch stays on a local machine that has a display. Absence of a display is not a separate CI failure.

`fmt` and `clippy` may use the network. The test and `x86_64` build commands are `--offline`. They do not download crates. A runner whose cargo registry does not already contain the needed crates fails those jobs closed. That is not a qualification result, and it is not hidden.

## Jobs that only record BLOCKED

These jobs do not compile, do not cross-compile, do not launch a window, do not start Wine, and do not scan. Each one writes a JSON file with `"status": "BLOCKED"` and uploads it. They exit 0 because the evidence status is BLOCKED, not because the missing qualification succeeded. The JSON must not say PASS.

A green blocked-evidence job is not a qualification pass. GitHub marks the job success only because the file was written and the process exited 0.

| Job | Evidence | Artifact | Why it is BLOCKED |
| --- | --- | --- | --- |
| `arm64-elf-build` | `evidence/emperor/linux-arm64/ci-blocked.json` | `emperor-arm64-blocked` | This runner neither launches an aarch64 machine nor treats cross-compilation as qualification. No cross toolchain is installed. |
| `host-wayland-integration` | `evidence/emperor/compositor/ci-blocked.json` | `emperor-wayland-blocked` | GitHub-hosted Ubuntu has no Wayland display, and a headless test is not a window launch. |
| `compositor-matrix` | `evidence/emperor/compositor/matrix-blocked.json` | `emperor-compositor-matrix-blocked` | Mutter, KWin, wlroots, and COSMIC are not pinned or executed. |
| `performance-regression` | `evidence/emperor/performance/ci-blocked.json` | `emperor-performance-blocked` | No benchmark numbers. Provisional targets in GY-EMP-001 are not measurements. |
| `security-analysis` | `evidence/emperor/security/ci-blocked.json` | `emperor-security-blocked` | Dependency review, seccomp, and Landlock are not executed here. No CVE results are recorded. |
| `package-verification` | `evidence/emperor/packaging/ci-blocked.json` | `emperor-packaging-blocked` | AppImage, Flatpak, deb, and rpm are BLOCKED. `linux-archive` is not produced in CI. |
| `sash-parity` | `evidence/emperor/parity/ci-blocked.json` | `emperor-parity-blocked` | Sash Windows execution remains blocked on this runner. A Linux test does not close the Windows gate. Wine is not started. |
| `release-admission` | `evidence/emperor/release/ci-blocked.json` | `emperor-release-blocked` | Decision `not admitted`. See below. |

`release-admission` needs `fmt-and-lint`, `unit-and-property-tests`, and `x86_64-elf-build`. It does not need the blocked-evidence jobs. Those jobs are evidence-only and always green, so they are not a gate. The release JSON status is `BLOCKED` and the decision is `not admitted`, for these reasons:

- no signed artifact
- no clean-machine install
- no Wayland launch in CI
- no Windows parity launch
- phases E2-E6 unmet

The release job's conclusion in GitHub will be success. The JSON decision must still be BLOCKED. That success is not release admission.
