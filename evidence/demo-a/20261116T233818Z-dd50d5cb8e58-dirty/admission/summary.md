# GY-DEMO-A-001 admission 20261116T233818Z-dd50d5cb8e58-dirty

Qualification status: FAIL.
Programme summary: PARTIAL.

The completion statement is not issued.

| Gate | Status | Detail |
|---|---|---|
| DA-G01 | FAIL | cargo build --workspace -> 101, cargo test --workspace --lib --tests -> 101, cargo clippy --workspace --all-targets -- -D warnings -> 101, cargo fmt --all -- --check -> 101 |
| DA-G02 | PASS | c550e4608d8217b84c946732fd2ecd9944c800d8979dc332dbea7d87fa9d61fd |
| DA-G03 | PASS | PE_STRUCTURE_PASS / WINDOWS_RUNTIME_BLOCKED. The static verifier did not launch Windows. |
| DA-G04 | PASS | ELF x64 type 3 entry 1101344 relro True |
| DA-G05 | BLOCKED | This process is not running in an interactive Windows x64 session. Wine, QEMU, and a Linux cross-link are not a Windows launch. |
| DA-G06 | BLOCKED | Process evidence does not establish a visible native window. |
| DA-G07 | BLOCKED | not passed: DA-T01, DA-T02, DA-T03, DA-T04, DA-T05, DA-T06, DA-T07, DA-T08, DA-T09, DA-T10 |
| DA-G08 | BLOCKED | not passed: DA-T01, DA-T02, DA-T03, DA-T04, DA-T05, DA-T06, DA-T07, DA-T08, DA-T09, DA-T10 |
| DA-G09 | BLOCKED | Independent launch was not shown on both operating systems. |
| DA-G10 | PASS | Independent review matches the computed gate decision. |

## Unqualified capabilities

Full visual parity, accessibility certification, IME, multiwindow, persistence, theme switching, installers, signing, production release, x86 and ARM64 execution, X11, Android, iOS, and browser targets are outside this demonstration.

Emperor still does not qualify full IME, sandboxing, advanced accessibility, or dynamic resize reflow. Those gaps are recorded and are not claimed as passed.

A scripted interaction suite, a Wayland smoke frame, and a static PE verifier are not native input passes.
