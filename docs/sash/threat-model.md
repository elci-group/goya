# Sash threat model — static-window slice

Status: scope note for S1. Not a completed security review.

## Assets

The authored `.gscene`, the generated Rust, the linked `App.exe`, the private PDB, and the evidence JSON. Signing keys are out of scope because this slice does not sign.

## Adversaries considered

Malicious or accidental scene text, paths in `--out`, and a toolchain that downloads or promotes a bad image. Network attackers, update servers, notification payloads, and IPC peers are outside this slice because those features are not compiled.

## Controls in this slice

Admission rejects unknown syntax and unsupported reachable features before emission. Authored title, text, and id are written as decimal byte arrays, not interpolated into Rust strings, the resource script, or argument vectors. The PE reader bounds size, sections, imports, and resource walks, and it does not echo a hostile DLL name. Promotion requires a PE32 i686, PE32+ x64, or PE32+ ARM64 GUI image whose manifest `processorArchitecture` is `x86`, `amd64`, or `arm64` for that machine, a Sash assembly identity for one presentation profile, a non-zero entry, `asInvoker`, per-monitor V2, a version resource, ASLR, DEP, control-flow guard, and an allowlist of system DLL names. `verify` reports that machine and profile. High-entropy ASLR is required on the two 64-bit machines. CRT redistributable imports fail the build. The link does not download an SDK. `verify` cannot sign or launch.

## Limits

The linked executable is a full-trust desktop image for whoever later runs it. Rust memory safety in the compiler is not a Windows sandbox. This host did not run the exe, so the generated Win32 calls were not observed against a real session. PDB files contain symbols and must stay with the evidence bundle, not in a public download. Logs of a failed link are truncated and NUL-stripped, but they can still quote compiler diagnostics. There is no fuzz corpus yet. Parser, path, and import tests cover the cases encoded in `crates/sash`.

## Out of scope until admitted

Clipboard and IPC authorization, notification payload trust, shell registration, startup hijack, signing-key handling, and clean-machine DLL planting. Those need fixtures before a PASS.
