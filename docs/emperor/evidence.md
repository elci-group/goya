# Emperor evidence

Programme GY-EMP-001, revision 1.0.0. Date 16 November 2026. Baseline commit `3f7fa955568b778455da348b1b80ed98c66156e5`.

Every qualification run SHALL write an immutable record tied to a source revision. A missing record leaves the claim incomplete. These documents are not themselves a qualification run.

## Directory layout

Records live under `evidence/emperor/` in exactly these directories:

```
evidence/emperor/
  baseline/
  compiler/
  native-ir/
  linux-x64/
  linux-arm64/
  compositor/
  input/
  accessibility/
  security/
  performance/
  parity/
  packaging/
  release/
```

`baseline/` holds the workspace snapshot for commit `3f7fa955568b778455da348b1b80ed98c66156e5` and later comparisons against it. `compiler/` holds CLI envelopes and stage lists. `native-ir/` holds canonical `GY-EMP-NIR/1` JSON and its SHA-256. `linux-x64/` and `linux-arm64/` hold ELF summaries and launch logs for those machines. `compositor/` holds one record per pinned compositor, or a BLOCKED record when none ran. `input/` holds pointer, keyboard, and clipboard observations. `accessibility/` holds screen-reader or AT-SPI logs. `security/` holds sandbox, SBOM, and signature results. `performance/` holds raw benchmark samples. `parity/` holds shared-oracle results and states which platform was actually launched. `packaging/` holds archive and installer logs. `release/` holds the admission decision.

A CI job that lacks the required machine SHALL write `status` `BLOCKED` in the matching directory. A job that exits 0 after writing that JSON has not passed the gate. The JSON must not say PASS.

## What a run record contains

Each record SHALL include these fields:

- Source SHA. The programme baseline is `3f7fa955568b778455da348b1b80ed98c66156e5`. A later worktree records `git rev-parse HEAD` when that command succeeds, otherwise the string `unknown`. Do not substitute the baseline SHA for an unknown worktree.
- Compiler version. The first cycle's string is `0.0.1`.
- Target triple.
- Toolchain, including the Rust version reported by `rustc --version` when it runs. The repository pin is 1.95.0. Also record the dependency lock digest when one is computed. This cycle does not require that digest for EMP-M1 file emission, and a missing digest stays a gap, not a silent pass.
- Command, as an argument vector, not a shell line.
- Exit status.
- Artifact hash, SHA-256 of the ELF when an ELF exists, and byte length.
- Wayland display presence: set or unset. Do not record a display name that was not in the environment. Presence is not a compositor identity.
- Qualification decision: one of PASS, FAIL, BLOCKED, or NOT_APPLICABLE, with a justification when the decision is not PASS.

Where the run has them, also record the kernel, distribution, compositor name and version, environment fingerprint, raw measurements, failures, and limitations. Do not invent a compositor, a GPU, or a sample count.

## Measured, inferred, proposed

A report SHALL keep three groups separate.

Measured values are outputs of a command that ran: exit status, hashes, oracle stdout, smoke stdout, ELF header fields, byte lengths. Copy them. Do not round a time that was not measured into a target.

Inferred conclusions are judgments from those values. Example: oracle count 1 revision 1 means the shared add ran once in process. It does not mean a window opened.

Proposed targets are the provisional numbers in GY-EMP-001 section 11. They are not results. A performance record that only repeats those numbers is not a measurement. Write them under proposed targets and leave measured values empty or marked not run.

## What must not be reported as release success

A mocked display, a unit test that does not open a window, `App --oracle`, or `parity-counter` SHALL NOT be reported as release success. They also SHALL NOT be reported as a Wayland launch.

If `WAYLAND_DISPLAY` is empty, stage 12 of `build` is BLOCKED and the process exits 3 even when the oracle passed. `verify` without a recorded smoke status `passed`, and without `--launch` against a live display, exits 3. Headless-only results stay in `compiler/` or `parity/` with decision BLOCKED for the window gate.

Windows evidence does not move to PASS because a Linux ELF ran. A Sash PE hash is not a Windows launch. Cross-compilation is not an architecture launch. A green GitHub job whose artifact is a BLOCKED JSON file is evidence of a gap, not admission.

Release admission stays BLOCKED until the twelve completion gates in GY-EMP-001 section 18 are satisfied. No record in this cycle is authorised to set that decision to PASS.
