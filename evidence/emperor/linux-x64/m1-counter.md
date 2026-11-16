# EMP-M1-01 Wayland smoke

Compiler 0.0.1. Source SHA `3f7fa955568b778455da348b1b80ed98c66156e5`. This note explains `m1-counter.json`. It does not certify milestone EMP-M1, a compositor matrix, or a production release.

## Measured

Command, as an argument vector:

```
cargo run -p emperor --offline -- build
  /home/sal/goya/crates/sash/fixtures/interactive-counter.gscene
  --target x86_64-unknown-linux-gnu
  --profile balanced
  --out /tmp/emperor-m1
  --evidence /home/sal/goya/evidence/emperor/linux-x64/m1-counter.json
  --format json
```

- Exit status: 0.
- Host: Linux 6.18.7-76061807-generic x86_64, Pop!_OS 24.04 LTS, rustc 1.95.0 (59807616e 2026-04-14).
- `WAYLAND_DISPLAY` was set to `wayland-1`. `XDG_RUNTIME_DIR` was `/run/user/1000`. The socket `/run/user/1000/wayland-1` existed. No compositor name or version was read.
- ELF SHA-256: `6495deb52a2746689e367e54f3e29b381f64b413a68d74e63c9d7eb28b866775`.
- ELF length: 99518208 bytes. Debug build, `release` false. The size includes debug info.
- ELF class 64, little-endian, type 3 (DYN), machine 62 (`x86_64`). `PT_GNU_RELRO` present. `PT_GNU_STACK` present and not executable.
- Oracle stdout: `count=1 revision=1 presented=0 wayland=not-requested copy=true paste=false`.
- Smoke stdout: `count=0 revision=0 presented=1 wayland=opened copy=true paste=false`.
- Smoke status string: `passed`.
- A later `verify /tmp/emperor-m1` exited 0 because the stored hash matched. That verify did not open the window again.

## Inferred

`GoyaWindow::create` returned and one frame was presented on the display that was set. The window path did not send a seat click, key, resize, or clipboard event. The count stayed 0 on that path. The count 1 result is the headless oracle.

## Proposed

Section 11 targets, including cold start, idle RSS, and the Electron comparison, were not measured. ARM64, i686, musl, riscv64, Vulkan, packaging, signing, and the twelve production gates stay open.
