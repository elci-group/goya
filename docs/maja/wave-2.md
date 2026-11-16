# Wave 2 status

M2 is **blocked and not complete**. There is no generated browser application, D1 browser runtime, authored view binding or verified Frampton browser integration. No browser execution, keyboard/focus, accessibility, reload or state-equivalence result is claimed. The previous M0 and M1 gates remain open; see [Wave 1](wave-1.md) and [ADR-001](adr-001-frampton.md).

The semantic inspection boundary now retains the exact original UTF-8 source together with the strictly admitted GSSH/1 snapshot. It recomputes SHA-256, checks every entry digest against the snapshot, and rejects invalid or non-UTF-8-boundary ranges. Available origins remain canonical and may be enclosing or empty; this does not provide a generated source map or invent missing fine-grained origins. All production builds remain rejected.

Verification: `cargo test --offline -p maja-semantic -p maja-frampton -p maja` passed six tests, including source preservation, Unicode ranges and fail-closed admission. `cargo clippy --offline -p maja -p maja-semantic -p maja-frampton --all-targets --no-deps -- -D warnings` passed. The existing canonical oracle tests `cargo test --locked --offline -p goya-lang -p goya-state -p goya-executable --lib` passed 128 tests. These checks are not browser differential evidence or production qualification.

The standalone host-only corpus in `tests/maja/differential` ran `cargo test --offline --manifest-path tests/maja/differential/Cargo.toml --target-dir /tmp/maja-differential-target`: two tests passed. It compares SDK state ownership with the compiled canonical interaction over sequences, integer limits, invalid inputs and revision overflow. Both paths depend on the canonical executor; this is preparation for a future browser comparison, not an independent implementation or proof of browser equivalence. Initial values and inputs are test-provided and do not define authored binding semantics. Browser prerequisites and the required observations are recorded in `tests/maja/browsers/README.md`.

`uni --fail-under 80 .` exited 1 with 73/100 and reported incomplete checks. It is not a passing qualification gate.

Unblocking the vertical slice requires an authoritative Frampton browser implementation and pinned API, a reviewed initial-state and input/view-binding contract, supported target admission, and an actual generated application tested against the canonical D1 oracle in a real browser. No direct HTML fallback is approved by the profile.
