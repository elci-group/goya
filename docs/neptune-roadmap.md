# Neptune roadmap to v0.0.1

Status: **active plan, 21 November 2026.** This roadmap separates implemented
host-side capabilities from release evidence. A source preview, passing host
test, or generated artifact is not an admitted or working iOS application.

## Completion target

Neptune v0.0.1 is complete when one `.gscene` project admitted through the
approved shared handoff compiles into a signed iOS application, and the app
matches the Rust reference for layout, input conversion, D1 behavior and
lifecycle on both a simulator and a physical iPhone. Each release gate below
must have reproducible evidence. The public `check --target ios` path must
accept the declared profile and refuse unsupported constructs at exact source
ranges.

The initial profile is the bounded interactive scene slice already qualified
by Aragon: nested cells, proportional geometry, wrap, scroll, fill, buttons
with D1, bound inputs and session state. D2, durable persistence, navigation,
lists, `text-from`, and services remain refused until their shared semantics
are approved and separately qualified. The exact GSSH version is a gate: GSSH/2
is approved; GSSH/3 remains a candidate as of this roadmap. Do not claim the
proportional profile as stable until GSSH/3 is ratified.

## Current position

| Area | Observed status | Completion gap |
| --- | --- | --- |
| Shared contract | GSSH/2 approved; GSSH/3 candidate. Neptune's scene paths call `goya_lang::handoff::admit_v3`. | Decide whether Neptune waits for GSSH/3 ratification or starts an explicitly experimental parallel slice. |
| Frontend and emission | Neptune has several bounded preview emitters, source maps and CLI/service operations. | No unified exhaustive `compile_ios_scene` returning a typed iOS scene or complete per-file source maps. |
| Host oracle | `test --suite d1-corpus` compares Rust-side execution paths. | Generated Swift is not executed; no independent Swift semantics gate exists. |
| Swift validation | The repository documents Swift 6.1.2 as the pinned host toolchain. | `swiftc` is absent from this host; no Swift checks ran here, and no locked CI toolchain, iOS SDK type-check, or linked application build exists. |
| Apple pipeline | `apple-plan`, deploy-manifest validation, IPA hash verification and manual sideload hooks exist. | No Mac worker, Xcode build runner, simulator qualification, signing pipeline, or physical-device evidence. |
| Cleanup | Broken stdout pipes are handled; the Kotlin source-map branch is absent from the current source. The documented Swift 6.1.2 pin is retained. | Bootstrap the pinned toolchain on a permitted host and record reproducible host-check evidence. |
| Programme dependency | `GOYA-PARITY-LAB/0` says Neptune waits behind Linux/Sash parity. That programme's phases are unmet. | Obtain an explicit decision to wait or run a bounded Neptune slice in parallel. |

The status above includes the N0 host check on 21 November 2026: `cargo test
-p neptune` failed (43 passed, 25 failed). The failures are in preview/state
fixtures that now conflict with shared frontend requirements for declared
`@state` and bound inputs. `swiftc` is unavailable. The `uni --fail-under 80 .`
scan did not complete: its integration checks reported read-only evidence/cache
paths and the scan was interrupted before a score was produced. No release gate
is passed by these results.

## Critical path and phases

### N0 — Reconcile policy and close host cleanup (Linux)

1. Follow `GOYA-PARITY-LAB/0`: Neptune remains queued; no parallel-work
   exception is recorded. N1/N2 do not start until the programme exits or the
   project owner changes that policy.
2. GSSH/3 remains a candidate, so the proportional, Aragon-matching scene
   profile is blocked. GSSH/2 is approved, but it does not replace the required
   profile or authorize silently reducing its scope.
3. Retain the Swift 6.1.2 pin in `docs/swift-bootstrap.md`; this host has no
   `swiftc`, so Swift checks require a host with the pinned toolchain.
4. Record the observed `cargo test -p neptune` failure and the incomplete `uni`
   scan above. Fix the shared-contract fixture conflicts and rerun host checks
   before declaring N0 complete.

**Exit:** policy decisions are recorded, cleanup evidence is reproducible, and
the Neptune host suite and required quality scan pass. **Current status:
BLOCKED** by 25 failing tests, unavailable Swift, and the incomplete quality
scan. No Apple gate is implied.

### N1 — Shared admission and one scene compiler (Linux)

1. Route source through the approved, versioned GSSH handoff. Preserve the
   selected profile's rules and match Aragon's refusals for shared constructs.
2. Implement `compile_ios_scene(source) -> Result<IosScene, IosSceneError>` as
   the sole semantic lowering boundary. It must emit every admitted construct
   or reject it with a typed diagnostic and exact source range; no property may
   disappear silently.
3. Emit deterministic source maps for every generated Swift file, keyed by
   shared source origins and stable authored identities. Keep `map-swift` and
   preview integrity contracts aligned with the new bundle schema.
4. Compare admission decisions over the shared strict corpus and the Aragon
   profile. Keep old preview commands as temporary wrappers only, then retire
   them after callers migrate.

**Exit / NPT-G01:** shared admission, exhaustive lowering/refusals, full source
maps, deterministic output, and matching cross-target refusal evidence.

### N2 — Executable host semantics (Linux)

1. Separate a Swift-only core from the SwiftUI shell. The core owns D1 state,
   conversion and layout calculations; the shell maps solved frames to native
   views and dispatches typed actions.
2. Build an independent Swift execution harness comparing layout, scroll
   limits, field conversion and D1 against Rust references. Start with Aragon's
   measured corpus scale: 61 scenes × 7 viewports, 331 conversion inputs, and
   40 random D1 programs × 12 dispatches.
3. Plant and demonstrate detection of at least three representative faults:
   layout, conversion and D1 dispatch.
4. Make `d1-corpus` include compiled Swift as another comparison column and run
   the checks in CI using the selected pinned host toolchain.

**Exit / NPT-G02:** all differential checks pass and each planted fault is
caught. This qualifies the pure core only, not SwiftUI or iOS behavior.

### N3 — Locked Apple toolchain and reproducible app build (Mac)

Prerequisites: an available Mac worker, a selected minimum iOS version, and an
Xcode/SDK identity. A custom SwiftUI `Layout` requires iOS 16 or later; choose
the minimum deliberately rather than assuming it.

1. Extend the lock schema with Xcode version/build, Swift version, SDK
   version/build and minimum OS, and validate those values on the worker.
2. Implement an isolated worker runner for the existing Apple plan, including
   SDK discovery, bounded logs and deterministic cleanup.
3. Generate a deterministic Xcode project or Swift package with no network
   dependencies.
4. Type-check and build simulator and device targets twice from fresh copies
   at different paths; compare outputs and retain commands, hashes and logs.

**Exit / NPT-G03/G04:** locked toolchain, matching reproducibility builds, and
successful iOS SDK compile evidence.

### N4 — Simulator qualification (Mac)

1. Add an automated qualification runner (XCUITest or accessibility-tree
   capture) and compare element frames to the Rust layout oracle on multiple
   device sizes, orientations and scroll limits.
2. Run on the selected minimum iOS and a current iOS version. Compare D1
   dispatch, overflow/error behavior, input conversion and refusal policy to
   the shared contract.
3. Verify session state over scene recreation and reset after process death;
   verify field restoration wherever the shared contract requires it.
4. Exercise keyboard input, IME composition and VoiceOver names/roles. Keep
   Dynamic Type refused or explicitly constrained until the source layout
   contract defines scaling.

**Exit / NPT-G05/G07/G08:** simulator matrix passes with accessibility-tree
frames, semantic traces, lifecycle results and retained logs. `check --target
ios` accepts only this qualified profile.

### N5 — Signed package and physical iPhone (Mac + account + device)

1. Configure an authorized signing identity and provisioning profile.
2. Produce the `goya.neptune.deploy/0.0.1` package and verify its manifest and
   IPA digest against the built bytes.
3. Implement Dock transport for install, launch, update and log capture.
4. Install on a named physical iPhone and repeat the essential N4 geometry,
   input, D1 and lifecycle checks.

**Exit / NPT-G06/G10:** signed IPA, verified manifest/hash, install/update
record and passing physical-device qualification.

### N6 — Release audit

Complete a worker and signing-key threat model, privacy-manifest review,
dependency provenance, native package inspection, and reviewed performance
budgets with measurements for launch, dispatch and memory. Finish user and
operator documentation for supported constructs, refusals, builds,
qualification and deployment.

**Exit / NPT-G09/G11/G12/G13:** audit artifacts are reviewed and every release
gate has a PASS backed by retained evidence. Only then admit Neptune v0.0.1.

## Release gates

| Gate | Phase | Required evidence | Status at 21 Nov 2026 |
| --- | --- | --- | --- |
| NPT-G01 Frontend | N1 | Versioned admission, exact-range refusals, complete source maps, cross-target corpus agreement | BLOCKED |
| NPT-G02 Semantics | N2 | Swift core differential suite and planted-fault detection | BLOCKED |
| NPT-G03 Determinism | N3 | Locked Apple toolchain and matching clean-path builds | BLOCKED |
| NPT-G04 iOS compile | N3 | Type-check and simulator/device builds against locked SDK | BLOCKED |
| NPT-G05 Simulator | N4 | Geometry, D1 and conversion matrix on minimum/current iOS | BLOCKED |
| NPT-G06 Physical device | N5 | Essential qualification repeated on named iPhone | BLOCKED |
| NPT-G07 Lifecycle | N4–N5 | Recreation, process end and restoration behavior | BLOCKED |
| NPT-G08 Input/accessibility | N4 | Keyboard, IME, VoiceOver and scaling disposition | BLOCKED |
| NPT-G09 Native | N6 | App contents and linked framework inspection | BLOCKED |
| NPT-G10 Packaging | N5 | Signed IPA, verified manifest/hash, install and update | BLOCKED |
| NPT-G11 Security | N6 | Worker/signing threat model, privacy and dependency evidence | BLOCKED |
| NPT-G12 Performance | N6 | Reviewed budgets and device measurements | BLOCKED |
| NPT-G13 Documentation | N6 | Supported subset, workflow, refusal and deployment docs | BLOCKED |

Missing infrastructure is BLOCKED, never PASS or N/A. A failed observed check
is FAIL and must be fixed before the gate can pass.

## Decisions and dependencies

| Decision | Owner/action | Blocks |
| --- | --- | --- |
| Parity Lab sequencing | Project owner: keep Neptune queued or authorize N1/N2 in parallel | N1 onward |
| GSSH/3 ratification | Language/architecture owners: approve or revise candidate profile | Proportional geometry and Aragon-matching N1 scope |
| Apple worker | Project owner: owned Mac, hosted Mac, or macOS CI runner | N3 onward |
| Minimum iOS | Project owner with implementer: select supported minimum | Apple project/API choices and N4 matrix |
| Signing and device | Project owner: Developer Program access and test iPhone | N5 |

GSSH/2 itself is approved. D3 (`@input text:` as initial content) is resolved;
accessible input names, D2/effect adapters, durable persistence and display
bindings remain outside this release profile pending their shared contracts.

## Explicitly out of scope for v0.0.1

Navigation, lists, durable persistence, D2 effects/services, `text-from`, UIKit
emission, a C ABI runtime, iPad-specific adaptation and App Store submission.
Add any of these only after a shared source contract, reference oracle and
separate admission evidence exist.
