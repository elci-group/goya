# GY-EMP-001

Emperor, the Goya-to-Linux compiler. Revision 1.0.0. Repository copy recorded 16 November 2026. Baseline commit `3f7fa955568b778455da348b1b80ed98c66156e5`. Owner: the Goya compiler programme. Primary language: Rust. Counterpart: Sash, the Goya-to-Windows compiler.

This file is the contract an engineer implements. It does not certify that milestone EMP-M1 has passed. It does not designate Emperor state of the art. The directive's later objective is measurable competitive evidence. That designation is not granted here.

Compiler version for the first cycle: `0.0.1`. Profile for the first scene: `emperor.linux.interactive/0.0.1`. Pinned Rust toolchain: 1.95.0, from `rust-toolchain.toml`.

Normative sentences use SHALL. Each section ends with repository status. "Implemented" means the current cycle specifies the behaviour in `crates/emperor` or `crates/emperor-runtime`. It is not a test result. Anything those crates are not specified to do is Unmet or Blocked.

## 1. Mission and six release dimensions

Emperor SHALL convert admitted Goya source into a native Linux ELF with versioned behavioural guarantees. It SHALL be a compiler with admission, lowering, linking, static checks, and runtime checks. It SHALL NOT be only a source printer, only a wrapper over another GUI toolkit, or only a call into the existing Wayland host with no lowering.

One source SHALL keep the same admitted semantics on Emperor and on Sash. Native window handles may differ. Stored results may not differ by an undocumented rule.

Ordinary UI applications SHALL NOT require Electron, Chromium, a JavaScript engine, or an always-running application server. A browser engine is allowed only when the application declares a web-content capability. Profile `emperor.linux.interactive/0.0.1` does not declare one.

Release qualification SHALL be judged on six dimensions:

1. Correctness. The ELF preserves admitted Goya semantics.
2. Nativeness. The application integrates with the supported Linux desktops.
3. Performance. Efficiency is compared with equivalent implementations on a declared suite.
4. Portability. Qualified processors, distributions, compositors, and hardware configurations actually run the binary.
5. Security. Compile time and run time keep explicit capability and isolation boundaries.
6. Reproducibility. Outputs and evidence can be traced, repeated, and verified.

State of the art, if it is ever claimed, SHALL mean measured leadership or competitive parity on that suite. It SHALL NOT mean a small binary, the absence of JavaScript, or a fast launch by themselves. Optimisation SHALL NOT trade away correctness or security for an isolated speed gain. No such claim is made.

### 1.2 Mandatory outputs

For the selected target and profile, a production build SHALL be able to produce a native ELF, required native libraries and resources, a validated application manifest, a desktop entry and icons where they apply, a deterministic build report, a machine-readable capability manifest, an SBOM, a provenance statement, an executable conformance report, and a distribution package when requested.

This cycle's `build` SHALL write, in the output directory: `nir.json`, `embed.rs`, `capability-manifest.json`, `counter.desktop`, `sbom.json`, `provenance.json`, `App`, and `build-report.json`. Icons are not written. The conformance report for this cycle is the stage list plus the oracle line, and it is not a release pass when the window was not launched.

### 1.3 Non-goals

Emperor SHALL NOT replace the kernel, the compositor, or the desktop environment. It SHALL NOT invent a private protocol where an established Wayland protocol is suitable. It SHALL NOT promise every distribution. It SHALL NOT treat LLVM output as proof of optimal code. It SHALL NOT treat a linked ELF as proof of platform integration. It SHALL NOT let a Linux-only optimisation change shared semantics without a declared difference. It SHALL NOT keep a second copy of Sash's language and state rules.

Repository status: Implemented for the mission statement, the non-goals, and the file list in section 1.2, as the E0/E1 scope of `crates/emperor` and `crates/emperor-runtime`. The six dimensions are not satisfied. EMP-M1 is not certified. Performance, portability, security enforcement, and reproducibility are Unmet or Blocked as the later sections state.

## 2. Compiler architecture

### 2.1 Pipeline

The pipeline SHALL be these thirteen stages, in order. Every command that reports stages SHALL emit all thirteen. Each status SHALL be `PASS`, `FAIL`, `BLOCKED`, or `NOT_APPLICABLE`. `NOT_APPLICABLE` SHALL carry a reason. A blocked stage SHALL NOT be rewritten as `PASS`.

| Id | Name | Role |
| --- | --- | --- |
| 1 | lexical-syntactic-admission | Read the scene and admit syntax. |
| 2 | type-semantic-validation | Admit the interactive counter shape. |
| 3 | canonical-goya-ir | Keep the shared frontend result. Disagreeing frontends fail. |
| 4 | effects-capabilities | Record copy, paste, cut, and the closed capability set. |
| 5 | target-negotiation | Accept or block the triple and profile. |
| 6 | native-linux-lowering | Build `GY-EMP-NIR/1`. |
| 7 | optimisation | No pass in 0.0.1. |
| 8 | runtime-platform-linking | Link `emperor-runtime` and the Wayland host. |
| 9 | machine-code-generation | Rust ahead-of-time compilation of the embed. |
| 10 | elf-assembly-linking | Produce `App`. |
| 11 | static-verification | ELF checks in section 3. |
| 12 | runtime-conformance | Oracle, and a Wayland smoke only when a display is present. |
| 13 | packaging-signing-release | Package only when `package` is the command. Signing is never `PASS` in this cycle. |

`inspect` and `check` of an admitted counter SHALL mark stages 1 through 6 as `PASS`. Lowering MAY stay in memory. Stage 7 SHALL be `NOT_APPLICABLE` with the detail that compiler 0.0.1 has no optimisation pass. Stages 8 through 13 SHALL be `BLOCKED` because those commands do not link, launch, or package. The exit code SHALL be 0.

A rejected scene SHALL mark stage 1 or stage 2 as `FAIL` and SHALL NOT mark a later stage as `PASS`. The exit code SHALL be 1. The Sash diagnostic code SHALL be kept.

`check` SHALL NOT link. `build` SHALL link and still leave the window gate blocked when no display is present.

### 2.2 Shared frontend

Emperor SHALL consume shared contracts instead of forking them. The crates named for sharing are `goya-lang`, `goya-ir`, `goya-state`, `goya-layout`, `goya-runtime`, `goya-determinism`, `goya-executable`, and `goya-chroma`.

This cycle SHALL call `sash::admit::open` and `goya_lang::handoff::admit`. `goya-ir` is not a second source of window meaning. Amendments to shared semantics go to the shared owner. Swarm members SHALL NOT change canonical language semantics on their own. Emperor SHALL NOT add Linux-only syntax for a platform-neutral operation. A Linux-only feature SHALL be an explicit capability. This profile's closed set is in `contracts/gy-emp-nir-1.md`.

`goya-layout` supplies `Rect` in millipoints. `goya-runtime` supplies `Primitive`. `goya-parity` supplies the session, the key controller, and the hit map, and it is a consumer of Sash rather than a new frontend.

### 2.3 Native IR

Emperor SHALL lower into `GY-EMP-NIR/1`, specified in `contracts/gy-emp-nir-1.md`. The IR SHALL carry identity, geometry, text, input roles, state, capabilities, and source spans. Unknown fields SHALL fail. The canonical bytes SHALL be `serde_json::to_string` of the struct.

### 2.4 Strategies

Strategy A SHALL be implemented first. Admitted IR is lowered to typed Rust, then compiled and linked with the pinned Rust toolchain. Generated Rust is an embed file, not a crate the application author depends on.

Strategy B, direct lowering through LLVM or Cranelift, SHALL stay experimental until measured build time, binary size, or runtime cost justifies the maintenance. The first release SHALL NOT require a custom machine-code backend.

Repository status: Implemented for stages 1 through 6 on an admitted counter, for stage 7 as `NOT_APPLICABLE`, for Strategy A, and for the dual frontend call, all inside the crates named for this cycle. Strategy B is not started: Unmet. Full use of `goya-ir`, `goya-state`, `goya-determinism`, `goya-executable`, and `goya-chroma` on the Emperor path is Unmet. `goya-layout` and `goya-runtime` are specified only as the paint types in section 5. EMP-M1 is not certified.

## 3. Targets and ELF checks

### 3.1 Target table

Compilation success SHALL be recorded separately from launch. Cross-compilation alone SHALL NOT qualify an architecture. This cycle SHALL NOT download a Rust target.

| Triple | Priority | This cycle |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu` | P0 | Admitted for a host compile attempt. Launch still requires a Wayland display on that machine. |
| `aarch64-unknown-linux-gnu` | P0 | Admitted only when the host is aarch64. Cross-compiling it from another host is `BLOCKED`, exit 3. No ELF is presented as qualified. |
| `i686-unknown-linux-gnu` | P1 | Recognised. `build` returns `BLOCKED`, exit 3, and writes no successful ELF. |
| `x86_64-unknown-linux-musl` | P1 | Same as i686. Static-link viability is not investigated in this cycle. |
| `aarch64-unknown-linux-musl` | P1 | Same as i686. |
| `riscv64gc-unknown-linux-gnu` | P2 | Same as i686. Research only. |

Any other triple SHALL exit 1.

The default target SHALL be `x86_64-unknown-linux-gnu` when the compiler host is x86_64 Linux, and `aarch64-unknown-linux-gnu` when the host is aarch64 Linux. On any other host, a missing `--target` SHALL exit 2. An explicit admitted P0 triple that is not the host SHALL exit 3 and SHALL state that cross-compilation is not a launch.

The only implemented profile name is `balanced`. The default profile is `balanced`. `development`, `performance`, `small`, and `low-power` are recognised and `build` SHALL return `BLOCKED`, exit 3. An unknown profile name SHALL exit 2. `--release` is a flag inside `balanced`. It selects Cargo's release profile. It is not a sixth profile. `build` does not require `--release`. Debug output is allowed, with the RELRO rule below.

### 3.2 ELF checks this cycle performs

`crates/emperor` SHALL parse the ELF without an extra crate and without unsafe code. A file that is not a plausible ELF SHALL fail stage 11 and exit 4.

The checks that run are:

- Magic `0x7F`, `E`, `L`, `F`.
- Class 2, meaning 64-bit.
- Data 1, meaning least-significant byte first.
- Version 1.
- `e_machine` `0x3E` for `x86_64-unknown-linux-gnu`, or `0xB7` for `aarch64-unknown-linux-gnu`, matching the requested target.
- `e_type` is recorded and is not given a fictional required value.
- Program header `PT_GNU_STACK` (`0x6474e551`) SHALL NOT include `PF_X` (`1`). An executable stack SHALL fail stage 11 and exit 4 in both debug and release.
- Program header `PT_GNU_RELRO` (`0x6474e552`) is recorded. If it is absent, the stage detail SHALL say so. `--release` SHALL fail closed: stage 11 `FAIL`, exit 4. A debug build SHALL still be `PASS` only when every other check passed, and the missing RELRO SHALL remain visible in the detail. Hiding the absence is a defect.

`verify` SHALL repeat these checks.

### 3.3 ELF checks this cycle does not perform

This cycle does NOT perform a full glibc symbol-version audit, a dynamic-loader policy, an imported-shared-object allowlist, a relocation-type audit, a build-id requirement, a debug-symbol policy, or an unexpected-dependency failure. It does NOT launch the ELF on an older distribution. A binary built on a new glibc SHALL NOT be reported as able to run on an older one. Those checks stay Unmet until they exist, and the old-distribution launch stays Blocked until a machine or image is named and used.

Hardening that the toolchain does not support SHALL be reported. It SHALL NOT be assumed. Control-flow integrity and similar flags are not checked in this cycle.

Repository status: Implemented for the target table, the refusal to download targets, and the ELF checks listed as performed, in `crates/emperor`. ARM64 launch, i686, musl, riscv64, the glibc symbol-version audit, and an independent old-distro launch are Blocked. EMP-M1 is not certified.

## 4. Wayland, compositors, and X11

### 4.1 What the host already does

`goya-wayland` can create one `xdg_toplevel`, set a title, follow configure and acknowledgement far enough to store size and drop shared-memory buffers, present shared-memory frames, report pointer button `0x110`, report keyboard events, offer an in-memory clipboard, bind `zwp_text_input_v3` when that global exists, and emit close. Text input is disabled unless a caller enables it. The keymap-free path does not track Control and does not emit copy or paste shortcuts. The host does not apply the counter's allow and deny policy by itself. It is a live host for other shells. It is not already an Emperor compiler.

The programme SHALL put that host behind the versioned window behaviour in this section without a silent behaviour change for existing callers. A refactor that changes current host tests is out of scope for EMP-M1. EMP-M1 calls the host. It does not rewrite it.

### 4.2 What EMP-M1 wires

EMP-M1 SHALL wire one toplevel for the admitted counter:

- Create the window with the client text as the title and the admitted DIP size.
- Pointer: truncate pixel coordinates toward zero into DIP, hit-test with `goya_parity::pointer::hit_map`, focus and activate on the button, focus and do not activate on the edit.
- Keyboard: map host events onto Sash `key_action` as `contracts/gy-cross-parity-1.md` requires.
- Clipboard policy: copy allowed offers the edit text; paste denied changes nothing.
- Resize without reflow: dirty the frame, keep admitted DIP rectangles.
- Present on demand: `present_dirty` only when dirty, as `contracts/gy-emp-render-1.md` requires.

Emperor SHALL NOT assume it can place or focus a window when the compositor refuses. EMP-M1 does not call a position request.

The following host capabilities are required by the full window contract and are not wired for EMP-M1: destruction beyond the host close path, maximise, fullscreen, a second toplevel, popups, menus, dialogs, drag-and-drop, qualified IME, fractional scale, output hotplug, and a tested focus-activation protocol. They stay Unmet.

### 4.3 Compositor matrix

The qualification matrix SHALL include GNOME Mutter, KDE Plasma KWin, at least one wlroots-based compositor, and the COSMIC compositor. Each run SHALL pin the compositor version in the test manifest. A missing optional protocol SHALL be a documented fallback or an explicit unsupported feature. Protocol advertisement SHALL NOT be treated as correct behaviour.

The matrix has not been run.

### 4.4 X11

X11 SHALL be an optional backend, not the primary path. A future adapter MAY use a maintained X11 library. Native Wayland SHALL stay preferred. XWayland SHALL NOT be reported as native X11 support. No X11 adapter is started.

Repository status: Implemented for the single toplevel, pointer, keyboard, clipboard policy, resize without reflow, and present-on-demand wiring specified in `crates/emperor-runtime`. The existing host capabilities that EMP-M1 does not call remain as they are in `goya-wayland` and are not Emperor qualification. The compositor matrix is Blocked. X11 is Blocked. A Wayland launch is not certified. EMP-M1 is not certified.

## 5. Presentation

Emperor SHALL define `GY-EMP-RENDER/1` in `contracts/gy-emp-render-1.md`.

The renderer contract SHALL eventually include deterministic software rasterisation, a Vulkan backend, and a negotiated choice between them. The software path SHALL remain available for tests, accessibility tools, and machines without the accelerated device. Vulkan SHALL NOT be the only way to draw. Small static scenes MAY stay on software when measurement says so. No such measurement exists.

The Vulkan feature list in the directive (retained scene, incremental invalidation, dirty regions, bounded submission, texture lifetime, glyph and image caches, explicit ownership, surface recreation, multi-buffer scheduling, presentation feedback, device-loss recovery) is not started.

Frame scheduling SHALL be driven by real changes and by compositor constraints. An idle unchanged window SHALL NOT redraw. That rule is a runtime requirement of this cycle, not a benchmark result. Animation, refresh following, reduced motion, background throttling, and energy budgets are Unmet.

Integer scaling is the only scale this cycle specifies. One integer DIP equals one pixel at 250 millipoints and buffer scale 1. Fractional scaling is Blocked. HDR and wide gamut are Blocked. Logical versus physical coordinates, buffer size, input transforms, font rounding, colour conversion, and alpha rules beyond the host's current software present are Unmet except for the DIP-to-millipoint multiply and the truncate-toward-zero hit test. Chroma remains the theme authority and is not read by this cycle.

Repository status: Implemented for the software `present_dirty` path and the idle-redraw prohibition, in `crates/emperor-runtime`. Vulkan, fractional scaling, HDR, and wide gamut are Blocked. EMP-M1 is not certified.

## 6. Text, input methods, and accessibility

The full text requirement is grapheme-safe editing, bidirectional text, combining characters, emoji, font fallback, script shaping, selection, caret geometry, word movement, wrapping, scaling, multiline editing, and composition-aware undo. Measurement, glyph positions, hit testing, and carets SHALL agree when that work exists. This cycle does not implement that editor.

EMP-M1 text is the counter's edit string and the result string. The edit starts as the canonical decimal of `initial_input`. Non-control characters other than space append while focus is the edit. Backspace deletes the last Unicode grapheme. Neither changes the stored count. Space does not insert. The host calls `KeyController::on_key` with `WM_KEYDOWN` and `VK_SPACE`. The edit stays unchanged. The button activates. A control other than newline and backspace does not change the edit. Newline calls `press_add`. A string `parse_canonical_i64` rejects, including empty, `00`, `-0`, and overflow, stays in the edit and does not store. Copy and cut of a rejected non-empty edit offer that text, not the result. Cut clears the edit and does not store. An empty edit still copies as an empty string when copy is allowed and focus is the edit. Cut of an empty edit publishes nothing. Checked addition overflow and a revision that cannot increase do not store. The edit stays. A canonical negative stores through the same Enter path. The canonical text of `i64::MIN` stores that count through the same path. The edit stays. Preedit, commit text, and the rest of `zwp_text_input_v3` SHALL be ignored. Ignoring them SHALL be documented as a limitation, not as IME support. A partial XKB compose sequence is reset when the press is a shortcut. The same reset runs for an opted-in browser-navigation chord. The Emperor counter ignores that event. That reset is not browser qualification and not IME support. Keyboard leave resets a partial XKB compose sequence and clears the modifier booleans. The enter serial, layout group, and pointer position stay. That reset is not IME support. A compose completion while ctrl is held does not emit a character, except Ctrl+Enter and Ctrl+Backspace, which stay newline and backspace. That suppression is not IME support.

IME qualification, when it exists, SHALL cover Latin composition, CJK composition, combining diacritics, multi-codepoint emoji, dead keys, selection replacement, preedit cancellation, and focus changes during composition, on a real input method for each qualified desktop family. State-machine tests alone SHALL NOT pass that gate.

Native interaction that is in EMP-M1 is the focus cycle, the pointer hit, the clipboard policy, and the focus fill `0xFFD6E4F5` on the focused control. Pointer capture, hover and press states, context menus, scroll physics, touch, drag-and-drop, and a full shortcut map are Unmet. A press on `state.count` does not move focus and does not activate. A press outside the client rectangle is a miss. Focus stays. A later release on `window.add` does not activate. An allowed paste payload still replaces the edit after focus has moved. The request is the focus gate. Every interaction that does exist SHALL go through `CounterSession` or `KeyController`, not a parallel counter.

Accessibility semantics beyond that fill are Unmet. No AT-SPI tree is published. A fill is not a screen-reader pass.

Repository status: Implemented only for the edit string, grapheme backspace, focus fill, and the key and pointer policy specified in `crates/emperor-runtime`. IME qualification is Unmet. AT-SPI is Unmet. The rest of section 6 is Unmet. EMP-M1 is not certified.

## 7. Linux desktop integration

Emperor SHALL define `GY-EMP-LINUX/1` in `contracts/gy-emp-linux-1.md`.

The programme SHALL support XDG base directories, desktop-entry generation, application ids, MIME associations, icon installation, file dialogs, URI opening, notifications, settings, and startup activation where the platform supports them. Sandboxed mediation SHALL prefer XDG Desktop Portals. A direct call MAY be used only when it is explicitly selected and justified.

This cycle SHALL generate `counter.desktop` with `Type=Application`, `Name=Count`, `Exec=App`, `Terminal=false`, and `Categories=Utility;`. It SHALL NOT install that file. Portals are not started. MIME install, notifications, Flatpak, and icon install are Blocked.

Filesystem capability for this profile SHALL be `none`. The manifest SHALL also record network, subprocess, and web content as false. A described capability SHALL NOT be reported as an enforced sandbox. Undeclared privileged effects SHALL be rejected when a profile requires enforcement. This profile rejects them by admitting only the counter and by failing a NIR that claims a wider set.

Repository status: Implemented for desktop-entry generation without install, and for filesystem `none`, in `crates/emperor`. Portals and the other integration items are Blocked. EMP-M1 is not certified.

## 8. Security

The threat model SHALL cover untrusted scenes, third-party dependencies, build scripts and procedural macros, malformed assets and fonts, network-delivered content, untrusted documents, privileged Linux APIs, plugin execution, interprocess communication, package installation, and compiler artifact substitution. The repository statement is `threat-model.md`.

Compile-time protection that this cycle SHALL implement:

- Bound scene size by Sash admission, which rejects sources larger than 1 MiB.
- Reject native IR that fails `deny_unknown_fields` or the profile strings.
- Reject output, evidence, and `--nir` paths that contain a `..` component.
- Do not pass scene text or paths through a shell. `cargo` and `tar` are spawned by argument vector.
- Escape generated Rust strings.
- Keep build products under the requested output directory.
- Fail admission when the two frontends disagree.
- Reject a NIR whose capabilities conflict with this profile.
- Return diagnostics that keep the Sash code and, when Sash provided them, source locations.

Runtime profiles in the full programme SHALL be able to combine filesystem scope, namespaces, seccomp, resource limits, privilege drop, process isolation, and sandboxed document or plugin workers. Landlock MAY add filesystem limits where the kernel provides it, and the result SHALL be checked against that kernel. Failure to establish a required sandbox SHALL fail closed. This cycle meets that sentence by not claiming a sandbox. It does not install seccomp or Landlock.

Supply-chain requirements for a production release are a lock, a versioned toolchain, an SBOM, artifact hashes, provenance, vulnerability and licence review, signed artifacts, and verification with a separately trusted public key. Reproducible bytes and authenticity SHALL stay distinct.

This cycle writes a direct-edge SPDX 2.3 JSON SBOM naming emperor-runtime, goya-parity, goya-wayland, sash, and goya-lang. The document comment SHALL say that the list is direct edges only and that no vulnerability review was done. `provenance.json` SHALL record the scene SHA-256, the directive id, the compiler version, the target, the profile, the release flag, `rustc --version` when that command runs, and the `App` SHA-256 once the binary exists. It is not a signature.

Repository status: Implemented for the compile-time controls in this section and for the direct-edge SBOM and unsigned provenance file, in `crates/emperor`. Landlock, seccomp, signing, vulnerability review, and licence review are Unmet. The threat-model document exists and is not a completed review. EMP-M1 is not certified.

## 9. Optimisation

Emperor SHALL optimise in this order, and SHALL NOT skip ahead: correctness, removal of unnecessary work, allocation avoidance, locality, incremental state and layout, rendering, startup, package size, CPU and energy, then architecture-specific code generation.

Each optimisation pass SHALL declare preconditions, the IR it transforms, the invariants it keeps, any behavioural change, the expected benefit, the worst-case extra cost, the tests, and a rollback. The candidate list is constant propagation, dead-state elimination, unreachable-action elimination, static layout precomputation, constant resource folding, render-list deduplication, text-shaping cache reuse, incremental layout, allocation reduction, and architecture-aware generation. Diagnostics SHALL survive as far as the compilation strategy allows.

Profile-guided optimisation and link-time optimisation SHALL be experiments. Neither becomes the default without measurements on representative workloads. The profile names `development`, `balanced`, `performance`, `small`, and `low-power` SHALL each document compile time, size, runtime, and reproducibility before they are offered as qualified profiles. Only `balanced` is accepted for a compile attempt now, and it still has no optimisation pass. `--release` only forwards Cargo's release flag. It does not enable a named Emperor pass.

Stage 7 SHALL be `NOT_APPLICABLE` with the detail that compiler 0.0.1 has no optimisation pass.

Repository status: Unmet. No optimisation pass is implemented. The refusal to pretend otherwise is Implemented as stage 7 `NOT_APPLICABLE` in `crates/emperor`. EMP-M1 is not certified.

## 10. CLI and exit codes

The binary SHALL be `emperor`. Commands SHALL be `inspect`, `check`, `build`, `verify`, `test`, `benchmark`, `package`, and `qualify`. These names are the contract. Their presence in this document is not a claim that a given worktree already contains the binary.

Flags: `--target`, `--profile`, `--release`, `--out`, `--evidence`, `--format json`, `--launch` on `verify` only, and `--suite` on `test` and `benchmark`. An unknown suite name SHALL exit 2. `desktop-sota` is known and blocked, not unknown.

Positionals: one scene path for `inspect`, `check`, `build`, `test`, and `benchmark`. One directory for `verify`, `package`, and `qualify`.

Every command SHALL support `--format json`. The envelope fields SHALL be `schema_version` (`"1"`), `directive` (`GY-EMP-001`), `compiler` (`emperor`), `compiler_version`, `operation`, `target`, `profile`, `source_revision`, `exit_code`, and `stages`. `source_revision` SHALL be `git rev-parse HEAD` when that succeeds, otherwise `unknown`. It SHALL NOT silently reuse the baseline commit. Each stage SHALL have `id`, `name`, `status`, and `detail`.

Exit codes SHALL stay stable unless a later contract version changes them:

| Code | Meaning |
| --- | --- |
| 0 | Every requested check passed. |
| 1 | Source or user validation failed, including a bad scene, a `..` path, or an unknown triple. |
| 2 | Invocation or toolchain failed, including a missing flag, an unknown command, Cargo's failure to compile, or an unknown suite. |
| 3 | Qualification is blocked. The oracle may have passed. The window, the matrix, or the package did not. |
| 4 | The artifact failed verification, including a bad ELF, a release build without RELRO, or a hash mismatch. |
| 5 | Runtime or conformance failed, including a non-zero oracle or a non-zero smoke. |

`build --out DIR` SHALL reject `..`, create the directory, write the files in section 1.2, and invoke Cargo as `Command::new("cargo")` with arguments `build`, `-p`, `emperor-runtime`, `--bin`, `emperor-app`, `--offline`, `--target-dir`, `DIR/target`, plus `--release` when the flag is set. The environment variable `EMPEROR_EMBED` SHALL be the absolute path of `DIR/embed.rs`. The working directory SHALL be the workspace root. Cargo's stderr, truncated to 4 KiB, SHALL be kept in the stage detail on failure. The produced binary SHALL be copied to `DIR/App` and set executable. Stage 8 or 9 `FAIL` exits 2 when Cargo fails.

After a successful link, `build` SHALL run `DIR/App --oracle`. A non-zero status SHALL fail stage 12 and exit 5. If `WAYLAND_DISPLAY` is non-empty, it SHALL also run `DIR/App --smoke`, and a non-zero status SHALL exit 5. If `WAYLAND_DISPLAY` is empty and the oracle passed, stage 12 SHALL be `BLOCKED` with a detail that no display was present, the oracle passed, and no window launch was attempted. The process SHALL exit 3. Stage 13 SHALL be `NOT_APPLICABLE` on `build`. Signing SHALL NOT be `PASS`.

`verify DIR` SHALL require `App` and `build-report.json`, repeat the ELF checks, and recompute SHA-256. A hash mismatch SHALL exit 4. `App --oracle` failure SHALL exit 5. `--launch` with a display SHALL re-run `--smoke`, and failure SHALL exit 5. Without a recorded smoke status `passed`, and without a successful `--launch`, the window stage SHALL be `BLOCKED` and the exit code SHALL be 3 even if the ELF and the oracle passed. A stored smoke status `passed` may be reported as a previous observation only together with a matching hash. It SHALL NOT be invented. Headless oracle output SHALL NOT be labelled a window launch.

`test --suite conformance` SHALL run `emperor_runtime::oracle` in process on the lowered JSON. Exit 0 on success and 5 on failure. The default suite is `conformance`. `desktop-sota` SHALL exit 3. `benchmark` SHALL always exit 3 and SHALL NOT invent numbers. `qualify` SHALL always exit 3 because the matrix has not been executed.

`package DIR --format linux-archive` SHALL run `tar` by argument vector, reject `..`, and exit 0 only when `App` exists. The archive is not installed and not signed. A missing `--format` SHALL exit 2. `appimage`, `flatpak`, `deb`, and `rpm` SHALL exit 3.

The runtime binary `emperor-app` SHALL accept `--oracle` (the default), `--smoke`, `--live`, `--frames N`, and `--nir PATH`. `--nir` SHALL reject `..`. With no `--nir`, it SHALL use the embed when `embedded()` returns a scene, after `assert_scene_matches`. With neither, it SHALL exit 1. One stdout line SHALL be `count={c} revision={r} presented={p} wayland={w} copy={copy} paste={paste}`. Exit 0 on success, 5 on a runtime or Wayland error, 1 on usage or a bad NIR.

Repository status: Implemented as the CLI contract for `crates/emperor` and the `emperor-app` contract for `crates/emperor-runtime`. A green exit 0 from `check` is not EMP-M1. Exit 3 is the honest result of a build without a display. EMP-M1 is not certified.

## 11. Benchmarks

The comparison set SHALL include Electron, Tauri, GTK4, and Qt6. The report SHALL separate framework architecture from implementation language. A hand-written C++ Qt program SHALL NOT be treated as the same engineering effort as a declarative Goya scene unless the report says so.

Fixtures and what they measure:

| Fixture | Measurement |
| --- | --- |
| Hello World | Binary size, cold start, RSS |
| Interactive counter | Dispatch and UI latency |
| Complex dashboard | Frames per second, p95 and p99 frame latency |
| Rich text editor | Keystroke latency, allocations, Unicode |
| File explorer | Directory load, scrolling, I/O |
| Background worker | CPU, memory, idle power |
| Multiwindow application | Resource scaling and lifecycle |
| Large data view | Virtualisation and responsiveness |
| Accessibility fixture | Semantics and screen-reader behaviour |
| Persistence fixture | State survival and recovery |

Each run SHALL use equivalent features and data, pin software revisions, record CPU, GPU, RAM, kernel, and compositor, take at least 30 independent repetitions, separate cold and warm start, control or randomise order, report median, p95, and p99 where that is meaningful, report confidence intervals, disclose dropped runs, keep raw data and artifacts, and record thermal, power, and load conditions when they can be measured. Frame time SHALL come from presentation timing when the compositor provides it, not only from an in-process timer. Idle power SHALL name the instrument and the uncertainty.

The following numbers are provisional programme targets. They are not Emperor results. They SHALL NOT be copied into an evidence file as measurements. The steering record SHALL NOT weaken a threshold after a failed run in order to make that run a success.

| Metric | Provisional target |
| --- | --- |
| Cold start, trivial application | at most 150 ms on a reference machine that has not been named |
| Idle RSS, trivial application | at most 25 MiB |
| Idle CPU after settling | at most 0.5 percent of one logical CPU |
| p95 frame time at 60 Hz | at most 16.67 ms |
| p99 frame time at 60 Hz | at most 33.33 ms |
| p95 simple-action response | at most 30 ms |
| Unexpected background redraw | zero while the window is static |
| Cross-platform state mismatch | zero |
| Critical accessibility failures | zero |
| Required security gate failures | zero |

Relative aims, also unmeasured: at least 30 percent smaller median package than an equivalent Electron fixture when assets match, at least 30 percent lower median idle RSS than that Electron suite, and no material interaction-latency regression against the GTK4 and Qt6 baselines. Relative aims SHALL be tested apart from the absolute table.

`benchmark` SHALL exit 3. It SHALL NOT print fabricated samples.

Repository status: Unmet. Fixtures other than the counter scene do not exist as Emperor workloads. Targets are provisional and unmeasured. No Electron, Tauri, GTK4, or Qt6 comparison has been run. EMP-M1 is not certified.

## 12. Parity

The shared contract is `GY-CROSS-PARITY/1` in `contracts/gy-cross-parity-1.md`. The oracle SHALL define initial state, accepted inputs, transitions, errors, focus, clipboard, persistence, accessibility structure, visual geometry, and permitted platform differences. Persistence, accessibility structure, and visual tolerance are not defined for the counter beyond the statements in that file. They stay open.

Windows and Linux SHALL be observed separately. A Sash build that has not run on Windows SHALL NOT satisfy a parity gate. A Linux headless simulation SHALL NOT substitute for an Emperor Wayland launch.

The primary fixture SHALL grow into one authored project with editable text, buttons, menus, save and open, undo and redo, shortcuts, theme changes, resize, error recovery, and persistent state. The file that exists today is only the interactive counter. It is not that application.

Repository status: Implemented for the counter oracle specified to be called from `crates/emperor-runtime`, using `CounterSession` and `KeyController`. The Wayland launch half of the gate is not certified. The Windows half is Blocked. Parity Lab phases 1, 2, 3, 4, 5, and 6 stay unmet. EMP-M1 is not certified.

## 13. Packaging and release qualification

The first distribution form SHALL be an independently installable Linux archive. Later forms are AppImage, Flatpak, Debian packages, and RPM. Each form SHALL be qualified on its own. Flatpak SHALL use portals where the sandbox requires them. Native packages SHALL declare runtime dependencies accurately. A package SHALL NOT be called self-contained until that dependency model has been checked.

This cycle's only package command is `--format linux-archive`. The archive is a tar of the output directory. It is not installed, not signed, and not a release. AppImage, Flatpak, deb, and rpm are Blocked.

A release SHALL pass source-to-binary traceability, ELF structural verification, an architecture-specific launch, a clean-machine install, a functional smoke test, an uninstall and upgrade test, a check that there is no unexpected network dependency, provenance verification, SBOM validation, platform integration checks, a security review, and a reproducibility comparison. This cycle performs traceability only as hashes inside `build-report.json` and `provenance.json`, and ELF checks only as section 3.2. The other release bullets are Unmet or Blocked.

Repository status: Implemented for `linux-archive` generation only, in `crates/emperor`. That generation is not release qualification. AppImage, Flatpak, deb, and rpm are Blocked. Clean-machine install is Blocked. EMP-M1 is not certified.

## 14. Swarm

Work SHALL be split into reviewable streams. Shared language changes stay with the language owner. Each stream SHALL have paths, contracts, tests, a budget, dependencies, permitted integrations, exclusions, and merge prerequisites. A deliverable without its evidence is incomplete.

| Stream | Responsibility | Key output |
| --- | --- | --- |
| EMP-A | Compiler frontend and target IR | Linux lowering and the `emperor` CLI |
| EMP-B | Native code generation and linking | Verified ELF pipeline |
| EMP-C | Wayland desktop integration | `emperor-runtime` and the counter window |
| EMP-D | Rendering and typography | Software and GPU rendering |
| EMP-E | Input, focus, and accessibility | Interaction conformance |
| EMP-F | Linux capabilities and security | Sandbox and effect policy |
| EMP-G | Packaging, ABI, and distribution | Installable releases |
| EMP-H | Performance and comparison | Reproducible measurements |
| EMP-I | Emperor and Sash parity | This contract set and the shared oracle |
| EMP-J | CI, provenance, and integration | Release gate machinery |

This cycle assigns EMP-M1 compiler work to EMP-A (`crates/emperor`), the Wayland runtime to EMP-C (`crates/emperor-runtime`), the programme documents to EMP-I (`docs/emperor`), and the workflow file to EMP-J (`.github/workflows/emperor.yml`). EMP-B is not a separate backend. Strategy A is the code generator. EMP-D through EMP-H are not staffed by this cycle beyond the software present path inside EMP-C.

A completed stream SHALL include source, tests, evidence, limitations, performance notes, security notes, contract changes, integration steps, reproduction commands, and an owner. Missing evidence means the task is not complete. This document does not mark any stream complete.

Repository status: Implemented as the assignment table. No stream has a closing evidence bundle. EMP-M1 is not certified.

## 15. Phases and exit criteria

E0 and E1 are the current implementation cycle. Their exit criteria are not met. E2, E3, E4, E5, and E6 are unmet. No phase is recorded as met.

| Phase | Objective | Exit criterion | Status |
| --- | --- | --- | --- |
| E0 | Baseline | A clean reproducible workspace build, tests, lint, and baseline evidence. | Current cycle. Exit unmet. |
| E1 | First ELF application | A person can build and run the counter from source on a real Wayland desktop without hand-writing the shell. | Current cycle. Exit unmet. EMP-M1 is not certified. |
| E2 | Interaction | The shared parity application passes an end-to-end suite on Linux. | Unmet. |
| E3 | Graphics | Rendering conformance passes and the provisional thresholds are evaluated against measured baselines. | Unmet. |
| E4 | Distribution | Install, run, update, and uninstall on clean qualified machines. | Unmet. |
| E5 | Convergence with Sash | Both platforms pass the shared suite on real target systems. | Unmet. Windows execution is BLOCKED. |
| E6 | Comparative evidence | Correctness, security, and conformance gates hold, and the published performance criteria hold. | Unmet. |

Parity Lab phases 1 through 6 stay unmet. Windows execution stays BLOCKED. Those sentences are the parity programme's status. They are not an Emperor phase pass.

The immediate order is: repair CI and record the baseline; add the crate, CLI, targets, and contracts; add the smallest NIR on the existing frontend; connect the counter to the existing Wayland host without a new renderer; produce and check the first ELF; align input, focus, clipboard, and state with Sash; expand to the full parity application; qualify, then render and measure; extend to ARM64, compositors, packaging, sandboxing, and accessibility; then run the comparison suite. This document does not record any of those steps as finished. Steps after the first ELF's real launch are Unmet.

Repository status: E0 and E1 are the current cycle and their exits are unmet. E2 through E6 are Unmet. EMP-M1 is not certified.

## 16. CI job names

Emperor SHALL have a dedicated workflow, separate from the root `ci.yml`, integrated with the workspace. The workflow name is `emperor-ci`. The job names SHALL be:

- `fmt-and-lint`
- `unit-and-property-tests`
- `frontend-contract-tests`
- `native-ir-validation`
- `x86_64-elf-build`
- `arm64-elf-build`
- `host-wayland-integration`
- `compositor-matrix`
- `performance-regression`
- `security-analysis`
- `package-verification`
- `sash-parity`
- `release-admission`

Jobs that need hardware or a compositor the runner does not have SHALL write `BLOCKED` evidence and SHALL NOT write `PASS`. A protected release branch SHALL enforce the mandatory checks once those checks exist. A benchmark regression SHALL be statistical. A measured regression past an approved threshold SHALL block performance promotion unless a written waiver exists. No threshold has been approved, and no sample has been taken.

`x86_64-elf-build` on a runner without a display SHALL run admission (`check` on the counter, expecting exit 0) and SHALL NOT treat a missing display as a failed compile. It SHALL NOT treat that admission as a launch. The step SHALL say that it is an admission check and that the ELF launch is local. `arm64-elf-build` without an aarch64 machine SHALL record `BLOCKED` and SHALL NOT call cross-compilation a qualification. `release-admission` SHALL remain decision `BLOCKED` while there is no signed artifact, no clean-machine install, no Wayland launch in CI, no Windows parity launch, and phases E2 through E6 are unmet. A successful GitHub conclusion on a job that only uploads that JSON is not admission.

Repository status: The job names are specified. Whether `.github/workflows/emperor.yml` exists on a given branch is an EMP-J deliverable, not a qualification result. Blocked-evidence jobs are not passes. EMP-M1 is not certified.

## 17. Evidence layout

Evidence rules and the directory tree are in `evidence.md`. The directories SHALL be `baseline`, `compiler`, `native-ir`, `linux-x64`, `linux-arm64`, `compositor`, `input`, `accessibility`, `security`, `performance`, `parity`, `packaging`, and `release`, all under `evidence/emperor/`.

Each run record SHALL separate measured values, inferred conclusions, and proposed targets, and SHALL carry the source SHA, compiler version, target triple, toolchain, command, exit status, artifact hash, Wayland display presence, and qualification decision. Mocked or headless-only results SHALL NOT be reported as release success.

Repository status: Implemented as the evidence contract in `docs/emperor/evidence.md`. The directories are not a populated qualification archive. EMP-M1 is not certified.

## 18. Twelve completion gates

Emperor SHALL NOT receive a production designation unless all twelve gates hold. If any mandatory condition fails, the release SHALL stay `BLOCKED`. None of them is satisfied.

1. The authoritative compiler pipeline exists. Not satisfied. Stages 7 through 13 are not a complete production pipeline.
2. Supported native ELF targets compile and are qualified. Not satisfied. ARM64 is not launched. i686, musl, and riscv64 are blocked on purpose.
3. Required applications run on actual target systems. Not satisfied. No certified Wayland launch and no Windows launch.
4. Shared source contracts are preserved. Not satisfied as a release claim. The counter oracle is specified, and the wider parity application is not.
5. Linux desktop interaction is qualified. Not satisfied.
6. Accessibility tests pass. Not satisfied.
7. Security requirements are met. Not satisfied. No sandbox, no signature, no vulnerability review.
8. Distribution packages pass clean-machine verification. Not satisfied.
9. Required reproducibility checks pass. Not satisfied.
10. Equivalent workload benchmarks are published. Not satisfied.
11. No unresolved P0 or P1 release blocker remains. Not satisfied. This ledger is the blocker list.
12. Emperor and Sash parity obligations are satisfied. Not satisfied. Windows execution is BLOCKED, and a Linux oracle is not a Wayland launch.

Repository status: all twelve gates are not satisfied. The release decision is BLOCKED. EMP-M1 is not certified.

## 19. Immediate execution order

The first cycle SHALL follow this order, and a later step SHALL NOT be reported as done while an earlier launch step is blocked:

1. Repair Goya CI and record immutable baseline evidence.
2. Add the Emperor crate, the CLI, the target table, and these contracts.
3. Add the smallest `GY-EMP-NIR/1` and reuse the existing frontend.
4. Connect the admitted counter to the existing Wayland host without a new renderer.
5. Produce the ELF, execute it, and verify it.
6. Align input, focus, clipboard, and state with Sash.
7. Expand to the full parity application.
8. Qualify the Linux application, then change rendering and performance.
9. Extend to ARM64, compositors, packaging, sandboxing, and accessibility.
10. Run the comparison suite and the production admission review.

Repository status: steps 2, 3, 4, and 6 are the specified contents of `crates/emperor` and `crates/emperor-runtime`. Step 5 is not certified, because execution on Wayland has not been accepted by this document. Steps 1, 7, 8, 9, and 10 are Unmet. EMP-M1 is not certified.

## 20. Final instruction

Emperor SHALL be a native Linux compiler. The primary evidence of success SHALL be a functioning Linux application, generated from shared Goya source, that an independent person can run and check. Architecture diagrams SHALL NOT promote a release. Agents SHALL prefer an end-to-end executable, preserved semantics, reproducible evidence, and an explicit qualification over extra backends. When two strategies both meet the measured functional, security, and performance requirements, the simpler one SHALL be kept. Further backends, language extensions, and hardware specialisation SHALL wait until their benefit pays for their maintenance.

Binding decisions for this revision:

| Decision | Direction |
| --- | --- |
| Backend | Rust ahead of time first. LLVM or Cranelift only after a measured benefit. Not started. |
| Display | Wayland first. X11 is not started. |
| Presentation | Deterministic software through `goya-wayland`. Vulkan is not started. |
| Semantics | Shared with Sash. No Emperor dialect. |
| Release authority | Executable tests and evidence, not a successful compile by itself. |

EMP-M1, the native parity counter, is one admitted `.gscene` lowered to one ELF that opens one Wayland window. The acceptance list is a real window, working input and button activation, authoritative reproducible state, clipboard policy matching the scene, resize and focus matching the declared contracts, clean-build and runtime verification, and automated tests on the same oracle Sash uses. That list is the milestone definition. It is not a result.

Repository status: this section is the governing contract. No acceptance item is certified. EMP-M1 has not passed.

The governing principle is: compile first, prove correctness second, optimise under measurement third, and claim superiority only after comparative evidence.
