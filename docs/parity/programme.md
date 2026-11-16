# Goya Parity Lab

Programme `GOYA-PARITY-LAB/0`, adopted 2026-11-16. The primary development objective is a Linux native application and a Sash Windows application from one Goya source. Maja, Neptune, Aragon, and further experimental features wait behind this programme.

The objective is identical application intent, consistent core behaviour, and appropriate native behaviour.

This document is the programme. The executable gate is `crates/goya-parity`. That crate does not launch a window and does not admit a release.

## Equivalence

| Class | Obligation | Current measurement |
|---|---|---|
| Functional | The same actions produce the same results. | `run_authored_add` gives the shared D1 owner the integers Sash parsed from the counter. The checked sum is 1 at revision 1. A second activation stores 2. A rejected edit, overflow, busy activation, release, or mismatched readback leaves the stored count unchanged. Undo, redo, document save, and restore are absent. |
| Visual | Layout, colour, spacing, typography, and hierarchy follow the authored Goya design. Native font rendering and window decorations are measured differences. | Sash admits the counter at 320 by 160 DIP. The four control rectangles are recorded in `docs/parity/geometry.md`. No pixel baseline and no tolerance exist. |
| Platform-native | Win32 conventions on Windows and Wayland desktop conventions on Linux. | Sash admits Enter, Tab, and the copy/paste policy in generated code. That image has not been launched. `parity-counter` runs the admitted add in a headless Linux process. It does not open a Wayland window. The differences already in the two code paths are listed in `docs/parity/platform-differences.md`. |

## Reference application

Parity Lab is the intended reference application: text editing, copy and paste, undo and redo, document saving, resizing, theme changes, keyboard navigation, error handling, and state restoration, driven by one script on both systems.

The scene that exists today is `crates/sash/fixtures/interactive-counter.gscene`. It is a heading, one integer edit, one add button, and one result cell. It is not Parity Lab. The illustrative two-window sketch is not a screenshot of a compiled application.

## Phases

| Phase | Objective | Exit condition | Status |
|---|---|---|---|
| 1 | Shared language, state, input, and UI contracts | Both compilers accept the same source with no undocumented exception | Unmet |
| 2 | Linux and Sash executable pipelines | Both binaries launch and operate on real target systems | Unmet. Windows execution is BLOCKED. |
| 3 | Layout, behaviour, native controls, and gestures | Automated behavioural parity and a written list of platform differences | Unmet |
| 4 | Persistence, accessibility, Unicode, crashes, and recovery | The conformance suite passes on both platforms | Unmet |
| 5 | Comparison with an equivalent Electron application | Reproducible measurements on both operating systems | Unmet. This waits until phase 3 passes. |
| 6 | Package and install on clean machines | Two independently usable distributable applications | Unmet |

`goya-parity::phase_exit_met` returns false for every phase.

## What the gate holds

- `d1_add` is shared. `run_authored_add` supplies the integers Sash parsed from the authored text. An empty D1 map is still rejected.
- `copy_paste_policy` is admitted by `sash.win32.interactive/0.0.2` and has not been executed on Windows. `clipboard_gate` runs that same allow/deny decision in process. `win32_keys` applies Enter, Tab, and Space-on-button through the session. Those keys have not been launched. `pointer_hit` names the front-most admitted rectangle. It does not open a window.
- `undo_redo` is rejected by the scene parser. The gesture resolver already has the operation names.
- `document_save_and_restore` and `theme` are absent for this source. `linux_executable` is headless: `parity-counter` runs the admitted add and does not open a Wayland window. `goya-wayland` remains a live host for other native shells and is not bound to this scene.
- `visual_tolerance` and `electron_comparison` are not compared. No universal pixel or memory threshold is declared.

## Success, when the evidence exists

Admitted core semantics pass on both targets. Neither target omits an admitted behaviour silently. Native binaries have been launched on actual Linux and Windows systems. Visual differences sit inside a tolerance that was agreed after a baseline. Platform-specific interaction differences are tested. Performance tables include every workload that was run.

Sash's cross-linked x86, x64, and ARM64 images remain unqualified until a Windows launch. Wine, QEMU, and libvirt are not that launch.
