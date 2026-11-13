# Goya Gestures grounding and gap ledger

Directive: GOYA-GEST-TD-001 v0.0.1; product baseline: GOYA-GEST-PRD-001 v0.0.1.
Inspection baseline: commit `0878b42f196806958283801b152c1459c3c52442`;
`git status --short` and `git diff --stat` were empty at the start. This
document is a partial source inspection, not G0 ratification or release evidence.
Applicable `/home/sal/AGENTS.md` requires tests, synchronized documentation and
`uni --fail-under 80 .`. The directive was supplied in the task conversation;
no repository copy of it or its companion PRD was found in the initial filename
search. No device or target build was exercised in this inspection.

## Authority and implementation boundary

The existing [GSSH gap ledger](aragon/gssh-gap-ledger.md) selects GY-LANG/1
`Scene` as authoring authority and a future versioned shared contract as the
handoff. `goya_lang::parse_strict` produces `ParsedScene` with authored IDs,
source digest and property byte ranges. `goya_ir::Module` is a separate legacy
flow representation; `goya_lang::lower` discards authored semantics and
origins and resolves layout for a viewport. Neither `Module` nor a target
emitter is a complete semantic handoff. Gestures should extend the existing
source authority and the shared handoff, not the legacy parser or a target
dialect. The physical module and schema of that handoff are not yet established;
this choice does not pretend that GSSH exists today.

`InteractionDecl` describes bounded D1/D2 button-triggered transitions; it is
not the proposed editing policy. `goya_state::Store` provides in-memory
revisioned transactions, but no field binding or persistence contract. A
Gestures editor must not treat its selection/history as a second domain store.
The existing `goya-runtime` custom editor owns byte-indexed text, selection,
composition and undo/redo. Android's `GoyaInputConnection` bridges this runtime
to the IME; this is not evidence of a qualified native editing control.

## Requirement ledger

| Requirement | Observed path/type and behaviour | Semantic loss / required change | Migration and verification |
| --- | --- | --- | --- |
| GES-001–003, 014 | `crates/goya-lang/src/{scene,parser}.rs`: cells and existing button interactions; strict parser retains source entries. No Gestures declarations. | Add opt-in closed typed declarations, stable target references, decisions and exact origins through the shared handoff; reject unknown and contradictory input. | Preserve legacy parser/profile; exercise F04/F11/F15 and source-map failures. |
| GES-003, 012 | No shared Gestures resolver or distinct hard masks identified. | Define defaults, ordered scopes, pure predicates, hard restrictions and shadow diagnostics; prohibit widening. | Version defaults; compare policy combinations and hard-mask monotonicity. |
| GES-004, 007–009, 017 | `crates/goya-runtime/src/lib.rs` owns custom input state and uses UTF-8 byte ranges; Android IME converts UTF-16 indices. | A shared editor oracle, checked scalar/UTF-16/UTF-8 conversions, grapheme contract, revision/lifecycle reservations, bounded history and common validation are not established by those APIs. | Keep legacy runtime behaviour until an explicit migration; verify F01–F09 and F14, including injected failures. |
| GES-006, 015 | `crates/goya-clipboard/src/lib.rs` provides capability-gated copy and an in-memory provider; it is not a policy-aware async paste/cut execution boundary. | Add user-initiated, bounded plain-text transfer and revalidation; do not treat `last_entry` or Deep Copy as evidence of native clipboard enforcement. | Enumerate all controlled routes and test failure/partial-export cases without payload logging. |
| GES-005, 010–011 | `crates/neptune/README.md`: static Swift source preview only, no admitted iOS build. `docs/aragon/release-readiness.md`: Android compiler and qualification blocked at its inspection. | No native control or route set is admitted by this evidence; inspect actual platforms and enforce or reject each reachable policy. | Native builds, real controls, assistive sessions and device matrices required; F10 and route probes remain blocked. |
| GES-011, 016 | No inspected shared Card/Canvas admission contract or checked deferral implementation. | Surface-specific disposition and reachability must be explicit; reject deferral until implemented. | F12 requires real surface and fallback evidence. |
| GES-013–014 | `ParsedScene` records digest, element ID and source ranges; GSSH ledger identifies missing composed origins and Quinta identity. | Preserve property-level ancestry through the future handoff; inspect Quinta's actual authoring interface before integration. | F13/F15 compare CLI and Quinta canonical outputs; no parity claimed. |
| GES-018 | Neptune emits a source-preview bundle and hashes, not Gestures disposition, route or qualification evidence. | Bind deterministic outputs to inputs/locks and produce route, conformance, benchmark and release evidence. | F16 and D0–D8 need recorded procedures, environments and outcomes. |

## Open G0 decisions and gate status

The [decision register](gestures-decisions.md) and [preliminary threat model](gestures-threat-model.md)
record candidate choices and evidence gaps. Both are drafts, not ratification.
On the inspected Linux host, Java and ADB are on PATH; Swift, Xcode, .NET and
Kotlin compiler commands were not found on PATH. This is not an inventory of
remote workers or SDK installations and establishes no target qualification.

Follow-up at `d2d231608b9283625c58b6abf68c5a7ef466aaf5`: the Android IME
adapter clamps requested byte and UTF-16 offsets, so its existing conversion
cannot be assumed to meet the proposed checked-rejection contract. Neptune's
static-text preview is not a selection or editor control. `adb devices -l`
reported no attached devices. The strict frontend regression test
`strict_frontend_does_not_admit_unimplemented_gesture_declarations` covers
rejection of an unknown scalar property and the illustrative nested declaration;
it does not establish complete gesture admission, native behavior or legacy
parser enforcement.

The directive requires ratified ADRs, not assumptions. Syntax, exact shared
schema location, defaults and hard-mask representation, Unicode/grapheme version
and conversion fixtures, editor ownership, Windows framework, pinned platform
matrices, resource and performance corpus, custom-renderer exclusions,
deployment and named owners remain unratified. Sylyca, Quinta and the relevant
target toolchains have not been inspected sufficiently to select a first native
control. Proposed defaults and bounds in the directive are not measured or
admitted. Missing target workers, accessibility sessions or integrations must
be recorded as BLOCKED, never PASS.

An experimental calculation in `crates/goya-lang/src/gestures.rs` covers the
proposed profile defaults, ordered scope defaults, element override and hard
denial for one operation with a bounded scope list. It is not an execution
boundary: predicates, operation dependencies and contradictions, source
origins, target capabilities and live context are not implemented. In
particular, a static allow is never evidence that a secret export is permitted.
It is not connected to parsing, lowering or a native adapter. The scope limit
is an implementation bound, not a ratified profile limit.
The same module also checks a limited set of unconditional explicit
declaration contradictions: selection/copy/cut dependencies, mutation on
permanently read-only or static targets, and protected export. It does not
validate inherited decisions, conditional declarations or report source origins;
passing this check is not complete policy validation or target admission.
The experimental `convert_offset` helper converts checked scalar, UTF-8 byte
and UTF-16 code-unit offsets within the provisional 1 MiB input bound. It
rejects out-of-range positions and positions inside a scalar, including a
surrogate pair. It does not validate grapheme boundaries, pin Unicode
segmentation, alter the existing Android adapter or qualify a native control.
The separate experimental `validate_grapheme_range` checks scalar endpoints
against extended grapheme boundaries using the workspace lock's
`unicode-segmentation` 1.13.3. This is a build dependency lock, not G0
ratification of a Unicode profile or proof of native segmentation equivalence.
The experimental `decode_plain_text` helper reads at most one byte past the
provisional 1 MiB limit, rejects invalid UTF-8, and normalizes CRLF and CR to LF.
It accepts empty text and preserves other content. It does not select native
formats, access an OS clipboard, establish user authorization, validate an
editor replacement, or admit a native route.

## Experimental native clipboard work

These components are transport prototypes, not a shared Gestures execution
boundary, admitted controls, or evidence of platform support. No adapter is
wired to the Gestures policy and editor owner; the D0–D8 gates remain blocked.

| Platform | Observed implementation | Qualification and remaining work |
| --- | --- | --- |
| Linux/Wayland | `goya-wayland` data-device selection; its legacy receive path closes the local writer and caps reads at 1 MiB. | Synchronous receive may block on a peer, and the legacy `Option` conflates errors and empty text. No Gestures policy, revision reservation or route coverage. |
| Windows | `platform/windows/clipboard.rs` prototype uses CF_UNICODETEXT with bounded transport and typed failures. | Not linked to a host; native build and Windows route/device tests are pending. Clipboard retrieval may wait for delayed rendering. |
| macOS | `platform/macos/clipboard` AppKit export prototype with caller-owned authorization and cut revalidation. | No host integration or native build; paste fails closed pending bounded acquisition. Native route coverage and editor ownership are missing. |
| iOS | `platform/ios/gestures` UIKit export prototype; paste and cut fail closed. | No Swift/Xcode build, native control integration, or device/VoiceOver evidence. |
| Android | `platform/android/clipboard` Android transport prototype with host-owned policy checks. | No SDK-35 build, device tests, command-route enforcement or integrated editor lifecycle evidence. See its README for current supported operations. |

All D0–D8 gates remain **BLOCKED**: this partial ledger alone does not complete
grounding; no complete Gestures contract, independent oracle, native build, route coverage, device
evidence, security review, benchmark or Quinta parity has been demonstrated.
Next: inspect the actual target and authoring interfaces and toolchain
availability; write and ratify the required ADRs and threat model; implement
the shared contract and independent oracle before attempting one real native
receipt/notes slice. Preserve a separate qualification matrix for each surface
and adapter.
