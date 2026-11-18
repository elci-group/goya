# Neptune roadmap to v0.0.1

Status: **proposed, 18 November 2026.** This is a plan, not evidence. Every
gate below is BLOCKED today. Under `docs/parity/programme.md`
(GOYA-PARITY-LAB/0), Neptune waits behind the Linux/Sash parity programme.
Phase N0 is clean-up only. Starting N1 or a later phase needs that programme to
exit, or an explicit decision to run Neptune alongside it.

## Definition of done

Neptune v0.0.1 is complete when **one `.gscene` source, admitted under the same
shared handoff Aragon uses (GSSH/3), produces a signed iOS application.** That
application must reproduce the Rust reference's layout, D1 behaviour, input
conversion and lifecycle rules on a simulator and on a physical iPhone, with
recorded evidence for each gate below. `neptune check --target ios` must pass
for that profile and refuse everything else at an exact source range.

The scope matches Aragon's admitted profile, not a wider one. That means nested
cells, proportional geometry, wrap, scroll, fill, buttons with D1, bound inputs
and session state. Navigation, lists, durable persistence, D2 and services stay
refused, as they do for Aragon (`docs/neptune-semantic-mapping.md`).

## Where Neptune stands (assessed 18 November 2026)

| Area | State |
| --- | --- |
| Frontend | Consumes `goya_lang::parse_strict` + `lower` directly. It does **not** use GSSH/2/3 (`admit_v2`/`admit_v3`), unlike Aragon. |
| Emission | Eight narrow, separate preview commands: one cell, a static column, a transition, a D1 owner, a root button, an observable button, an observable column and observable buttons. Layout is a plain `VStack`; geometry, wrap, scroll, fill and inputs are refused. |
| Host oracle | `test --suite d1-corpus` passes all 10 cases. It compares the reference executor, the application owner and the target IR. Generated Swift is not executed. |
| Swift on Linux | All six generated kinds pass `swift-parse`. `preview-transition` output passes `swiftc -typecheck` (Swift 6.4.2). SwiftUI previews can't type-check on Linux. |
| Apple | `apple-plan` writes a handoff plan; `dock` validates a deploy manifest and the IPA hash. No worker, Xcode lock, build, simulator, signing or device run exists. |
| Tests | `cargo test -p neptune`: 79 pass. |
| Debt | Uncommitted clean-up in 7 files. Dead `KotlinMapping`/`map_kotlin_location` in `src/source_map.rs`. The CLI panics on a broken stdout pipe. Swift is pinned at 6.1.2 in `docs/swift-bootstrap.md`, but 6.4.2 is the version installed locally. |

The path below follows Aragon's sequence (clean-up, shared handoff, scene
compiler with differential tests, locked toolchain, emulator, device). Aragon
reached its Phase 4 that way (`docs/aragon/release-readiness.md`).

## Phases

### N0: Clean-up (Linux; allowed now)

- Commit the pending Neptune clean-up, which removes the Android artifacts from
  the Swift preview bundle and simplifies the conformance errors.
- Delete `KotlinMapping`, `map_kotlin_location` and the Kotlin branch of
  `map_verified_location`. Android source maps live in `goya-sdk/src/kotlin.rs`
  (GSSH decision D5).
- Handle `EPIPE` on stdout without panicking.
- Resolve the Swift pin drift. Either use the pinned 6.1.2 from
  `scripts/bootstrap-swift.py` for Neptune checks, or re-pin deliberately with
  new hashes.

**Exit:** clean tree, tests pass, `uni --fail-under 80 .` recorded.

### N1: Shared handoff and one scene compiler (Linux)

- Move Neptune onto `goya_lang::handoff::admit_v3` (GSSH/3), the same entry
  point as `compile_android_scene`.
- Replace the eight narrow preview commands with one
  `compile_ios_scene(source) -> Result<IosScene, IosSceneError>`. For each
  construct in the Aragon profile, it either emits code or refuses at the
  construct's exact range. Keep the existing commands as thin wrappers only
  while callers migrate, then remove them.
- Emit a source map keyed by GSSH/2 origins that covers every generated Swift
  file, not just `App.swift`. Keep the `map-swift` and `verify-preview`
  contracts.
- Make refusals match Aragon's for the same source. Add a shared corpus test:
  for every scene in `crates/goya-lang/tests/strict_corpus.rs`, Aragon and
  Neptune must agree on whether it is admitted.

**Exit:** NPT-G01 frontend evidence on host, plus a matching refusal test
against Aragon.

### N2: Run the generated Swift on Linux (Linux)

This phase moves `generated_swift` from BLOCKED to measured without a Mac.

- Split the generated output into a **SwiftUI-free core** and a **thin SwiftUI
  view**:
  - The core holds the D1 state owner, field conversion, and a pure-Swift
    layout spec that computes frames from the GSSH/3 tree.
  - The view only places views at those frames, using a custom `Layout`, and
    dispatches to the owner.
- Add `scripts/check-ios-scene-swift.sh`, mirroring
  `scripts/check-android-scene-kotlin.sh`. It compiles the core with the pinned
  Linux Swift and compares it against `goya_lang::layout`,
  `Frame::scroll_limits`, `handoff::convert_field` and the canonical D1
  executor, at least at Aragon's scale: 61 scenes × 7 viewports, 331 conversion
  inputs, and 40 random D1 programs × 12 dispatches.
- Plant at least three bugs (layout, conversion, D1), each of which must be
  caught.
- Extend `test --suite d1-corpus` so the compiled Swift core is a fourth column
  next to the reference, the owner and the target IR.
- Run the check in CI with the pinned toolchain.

**Exit:** NPT-G02 host semantics PASS for the core. The SwiftUI layer stays
unverified.

### N3: Apple toolchain lock and reproducible build (needs a Mac)

**Prerequisites.** These are owner decisions, listed below:

- A Mac worker.
- A chosen minimum iOS version. A custom `Layout` needs iOS 16 or later.
- An Xcode/SDK identity.

**Work:**

- Record the lock in the `apple-plan` schema: Xcode version and build, Swift,
  SDK version and build, and minimum OS.
- Implement the worker runner that executes the plan in an isolated copy,
  including SDK path discovery.
- Generate a deterministic Xcode project or Swift package with no network
  dependencies.
- Type-check, then build an unsigned simulator app and device app twice each,
  from fresh copies at different paths, and compare the hashes. This mirrors
  `scripts/check-android-apk-build.sh`.

**Exit:** NPT-G03 determinism and NPT-G04 iOS compile, both recorded in
`evidence/`.

### N4: Simulator qualification (Mac)

- Add a device-qualification runner (XCUITest or an accessibility-tree dump)
  that compares every element's frame with the Rust reference. Run it:
  - on several device sizes, in both orientations, at each scroll limit;
  - on the minimum iOS version and the current one.
- Check D1 dispatch and overflow errors, and field conversion, against GSSH/2.
- Lifecycle: session state survives scene recreation and resets after the
  process ends. Field text is restored if GSSH/2 says so.
- Input and accessibility: keyboard, IME composition, VoiceOver labels and
  roles, and Dynamic Type. Dynamic Type stays refused or documented until a
  source text-scaling contract exists.
- `neptune check --target ios` passes for this profile.

**Exit:** NPT-G05 simulator, NPT-G07 lifecycle and NPT-G08 input/accessibility,
each with logs.

### N5: Signing, packaging and a physical device (Mac, Apple Developer account, iPhone)

- Add the signing identity and provisioning profile, and produce an IPA
  described by `goya.neptune.deploy/0.0.1`.
- Implement the Dock transport (for example `devicectl` install, launch and
  log capture) behind the existing stage machine.
- Install, run and update on a physical iPhone, and repeat the N4 checks there.

**Exit:** NPT-G06 physical device and NPT-G10 packaging.

### N6: Release audit

- Security: a threat model for the worker and signing keys, the app's privacy
  manifest, and dependency evidence.
- Performance: reviewed budgets and measured launch time, dispatch latency and
  memory.
- Documentation: compiler, project, qualification and deployment.
- A release audit against every gate.

**Exit:** NPT-G09, G11, G12 and G13 pass. Neptune v0.0.1 is admitted.

## Gates

| Gate | Phase | Needs | Evidence required |
| --- | --- | --- | --- |
| NPT-G01 Frontend | N1 | Linux | GSSH/3 admission, refusals at exact ranges, full source map, refusals matching Aragon |
| NPT-G02 Semantics | N2 | Linux | Swift core matches the Rust references; planted bugs caught |
| NPT-G03 Determinism | N3 | Mac | Locked toolchain; matching twin builds from fresh paths |
| NPT-G04 iOS compile | N3 | Mac | Type-check and build against the locked iOS SDK |
| NPT-G05 Simulator | N4 | Mac | Accessibility-tree frames and D1/conversion behaviour on minimum and current iOS |
| NPT-G06 Physical device | N5 | iPhone | N4 checks repeated on hardware |
| NPT-G07 Lifecycle | N4–N5 | Mac | Recreation, process end and restoration as specified |
| NPT-G08 Input/accessibility | N4 | Mac | Keyboard, IME, VoiceOver and Dynamic Type results, with known gaps listed |
| NPT-G09 Native | N6 | Mac | Contents of the app package and the linked frameworks |
| NPT-G10 Packaging | N5 | Developer account | Signed IPA, manifest hash, install and update |
| NPT-G11 Security | N6 | — | Threat model, privacy manifest, dependency evidence |
| NPT-G12 Performance | N6 | Device | Reviewed budgets and measurements |
| NPT-G13 Documentation | N6 | — | Usage, supported constructs, refusals, deployment |

A missing worker, SDK or device keeps a gate BLOCKED. It is never a pass and
never "not applicable".

## Owner decisions needed

1. Whether Neptune N1/N2 may run alongside the parity programme, or must wait
   until it exits.
2. Mac worker hardware: owned machine, hosted Mac or CI macOS runner. This
   blocks N3.
3. Apple Developer Program membership and a test iPhone. These block N5.
4. Minimum iOS version, which constrains the SwiftUI APIs available.
5. Shared items Aragon also needs: architectural approval of GSSH/2, the `@input`
   `text:` decision, and input accessible names (decision D3).

## Not in v0.0.1

Navigation, lists, durable persistence, D2 effects and services, UIKit
emission, a C ABI runtime, iPad-specific layout and App Store submission. Each
needs a shared language contract first (`docs/neptune-semantic-mapping.md`,
prerequisite 2).
