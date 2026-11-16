# Emperor programme status

Programme GY-EMP-001, revision 1.0.0, recorded 16 November 2026. Baseline commit `3f7fa955568b778455da348b1b80ed98c66156e5`. This directory is the repository copy of the contract. It does not certify that milestone EMP-M1 has passed, and it does not grant a production designation.

Emperor is the Goya-to-Linux compiler. Sash remains the Windows compiler. There is no Emperor dialect. Both compilers are required to share Sash and Goya semantics. Release authority is executable evidence. A successful compile, a headless oracle, or a document is not that evidence.

## Binding decisions

Rust ahead-of-time compilation comes first. The first release does not require LLVM or Cranelift. Direct machine-code generation stays experimental and is not started.

Wayland comes first. X11 is not started. Running under XWayland is not native X11 support.

Presentation for this cycle is deterministic software drawing through the existing `goya-wayland` host. Vulkan is not started.

## Milestone EMP-M1

EMP-M1 is one admitted `crates/sash/fixtures/interactive-counter.gscene` lowered through `crates/emperor` into an ELF that links `crates/emperor-runtime` and can open a Wayland window. The acceptance text requires a real toplevel, input and button activation, authoritative state, the authored clipboard policy, resize and focus as declared, clean-build and runtime verification, and the same semantic oracle Sash uses.

These documents describe that contract. They do not record a Wayland launch, a passed smoke test, or a release admission.

## What this cycle is

Phases E0 and E1 are the current implementation cycle. Their exit criteria are not met. Phases E2, E3, E4, E5, and E6 are unmet. Parity Lab phases 1 through 6 stay unmet. Windows execution stays BLOCKED. No phase is recorded as met.

The cycle specifies two crates: `crates/emperor` (compiler version `0.0.1`, profile `emperor.linux.interactive/0.0.1`) and `crates/emperor-runtime`. The pinned Rust toolchain in `rust-toolchain.toml` is 1.95.0. Specifying those crates is not proof that they are present on every branch or that their outputs have been qualified.

## Where the rest of the contract lives

- Programme specification: [GY-EMP-001.md](GY-EMP-001.md)
- Gaps: [gap-ledger.md](gap-ledger.md)
- Threat model: [threat-model.md](threat-model.md)
- Native IR: [contracts/gy-emp-nir-1.md](contracts/gy-emp-nir-1.md)
- Presentation: [contracts/gy-emp-render-1.md](contracts/gy-emp-render-1.md)
- Linux desktop integration: [contracts/gy-emp-linux-1.md](contracts/gy-emp-linux-1.md)
- Shared oracle: [contracts/gy-cross-parity-1.md](contracts/gy-cross-parity-1.md)
- Evidence rules: [evidence.md](evidence.md)

The governing rule is to compile first, prove correctness second, optimise under measurement third, and claim superiority only after comparative evidence. No comparison against Electron, Tauri, GTK4, or Qt6 has been measured. Emperor is not designated state of the art.
