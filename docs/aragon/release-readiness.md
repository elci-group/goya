# Aragon v0.0.1 readiness

Status: **BLOCKED**. This is a preliminary A0 source inspection, not an
implementation or release evidence. No Kotlin emission, APK, AAB, benchmark,
device qualification or complete toolchain lock is asserted.

| Gate | Status | Evidence or missing work |
| --- | --- | --- |
| ARG-G01 Frontend | BLOCKED | Strict source-preserving path and negative corpus not implemented. |
| ARG-G02 Semantics | BLOCKED | No Android lowering or differential/mutation corpus. |
| ARG-G03 Determinism | BLOCKED | No generated outputs or R1 comparison. |
| ARG-G04 Android compile | BLOCKED | No generated app or verified pinned Android build environment. |
| ARG-G05 Emulator | BLOCKED | No emulator execution or recorded artifact. |
| ARG-G06 Physical device | BLOCKED | No device attached at inspection; no execution. |
| ARG-G07 Lifecycle | BLOCKED | No recreation, process-death or migration tests. |
| ARG-G08 Input/accessibility | BLOCKED | No Android IME or TalkBack evidence. |
| ARG-G09 Native | BLOCKED | Profile and package contents not established; absence of native libraries has not been verified. |
| ARG-G10 Packaging | BLOCKED | No release APK, bundle-derived APK set, signature or update test. |
| ARG-G11 Security | BLOCKED | No application manifest, isolated worker or dependency evidence. |
| ARG-G12 Performance | BLOCKED | No reviewed budgets, benchmark plan or measurements. |
| ARG-G13 Documentation | BLOCKED | Usage, deployment and supported-feature documentation require implementation and qualification. |

The initial inspection and outstanding semantic decisions are recorded in
`reconnaissance.md` and `semantic-path.md`. A missing evaluator or unavailable
device must remain BLOCKED; it is not a passing or inapplicable result.
