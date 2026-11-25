<p align="center"><img src="../../assets/arachnid-logo.png" alt="Arachnid — webOS app compiler" width="180"></p>

# Arachnid roadmap — webOS TV compiler target

Status: **in progress**, 20 November 2026, on branch `arachnid/v0.1`; continued
on `arachnid/v0.2` ([ARACHNID-2](ARACHNID-2.md), 25 November 2026).
* The owner's decisions on §6 are recorded in
  [ARACHNID-1](ARACHNID-1.md).
* Phases 0 and 1 and the static part of Phase 2 are implemented and
  qualified offline (`scripts/check-arachnid.sh`).
* Nothing has been installed or launched on a TV yet, so every device
  gate is BLOCKED.
* The engine and model lookup table is [webos-engines.md](webos-engines.md).

Sections 1–4 are the reconciliation as first written, when no Arachnid
code existed. They are kept as the audit trail.

Arachnid is Goya's proposed target for LG webOS TV. webOS Open Source
Edition (OSE) is a separate, later profile. The proposal's governing idea
is kept: the television is its own interaction environment, not a browser
on a large screen. In particular, spatial focus is a compiler
responsibility, and pointer and 5-way input must both work at all times.

## 1. Reconciliation with the repository as it exists

The pasted proposal was written without inspecting the repository. This is
how its assumptions compare with the tree on branch `nebula/v0.1`, checked
on 20 November 2026.

| Proposal assumption | Repository reality |
| --- | --- |
| Reuse Maja's web code generation | Maja's only emitting profile is `maja.web.local.v1` (docs/maja/standalone-profile.md). It admits one root `@cell` with an `id` and static `text`, and nothing else. It has no buttons, inputs, state, bindings, navigation, storage or browser runtime. `maja-runtime` can generate one checked integer-add D1 function, but no profile uses it. Reusing Maja today means reusing a static one-cell HTML/CSS emitter, plus Maja's deterministic, non-overwriting output discipline. The shared browser state contract that Maja is still missing is a prerequisite for Arachnid as well. |
| Compile-time focus graph from Gscene geometry | Most of the engine exists: `crates/nevada/src/focus.rs` (`nevada.focus/0.0.1`). It does deterministic directional navigation over millipoint rectangles, with strict perpendicular overlap, a documented tie-break, `focus_graph` and BFS `unreachable_essential`. Three things are missing. **Scene→rectangle lowering does not exist** (Nevada gap N0-FOCUSGRAPH), because the engine takes hand-built `FocusElement`s. There is **no focus-trap detection**: reachability from the initial focus does not prove the user can get back out. There is **no nested-scope model** for moving focus between child Gscenes. The engine lives in a crate whose contract is named `androidtv`. |
| Shared TV infrastructure with Nevada and Quitasol | Nevada has `dcr`, `profiles`, `translate`, `focus`, `conformance`, `target`, `orchestrate` and `diagnostics` (`GY-NV-5xx`). All are std-only and none admit scenes. Its census model assumes a D-pad baseline, and its `MotionOutput::NativePointerEvents` already expresses "pointer remote with usable output", which is the Magic Remote case. Quitasol (`quitasol-semantic`) compiles `@intent` declarations into a manifest marked "proposed, not ratified", with Alexa/Fire OS emitters. Nothing in Quitasol carries over to webOS voice. |
| Inquisition validates reachability and ergonomics | **No Inquisition exists** in the repository: no crate, doc or symbol. The closest existing piece is `nevada::conformance`, a fail-closed validator that combines translation, focus reachability and capability resolution. |
| Tauromaquia (Capes / Shields / Spears) | **Does not exist** in the repository. Arachnid cannot "implement all three primitives" until those primitives are specified somewhere. |
| Mesut manages Gscene lifecycle and resources | Mesut appears only as a typed host bridge (`goya-ecosystem`: `MesutTask`, `MesutBridge`, `submit_mesut`), which submits work to an *embedding application's* Mesut runtime. No Mesut runs in a browser, and no Gscene lifecycle or retention policy exists. |
| Iberia / España neural execution | `ie-contract` v0.1 is frozen (PAMPER IE-X01, ADR-IE-001), with `ie-cpu`, `ie-uds`, `ie-loopback`, `ie-gate` and `ie-conformance`. Its transports are Rust UDS and loopback. A packaged webOS web app can use only HTTP/WebSocket and has no CPU executor it can call. Running Iberia on the TV, or reaching España from it, therefore needs a new transport and executor that the frozen contract does not cover. That makes it a contract revision, not a Phase V add-on. |
| Caprichios per device family | `goya-caprichios` exists, and Maja uses it (`RuntimeCapabilities`, `Target`, `NetworkDeclaration`). This is the right home for webOS generation quirks. Nevada has no caprichio yet (N0-CAPRICHIO). |
| RUGID-powered games | RUGID is an optional external `rdf_render` binary called through `goya-ecosystem::export_rugid`. It is a host tool, not something that can run on a TV. |
| `goya build --target webos` | **There is no unified `goya` binary.** Each programme ships its own CLI (`maja`, `emperor`, `sash`, `neptune`, `dockyard`). Arachnid should start as `arachnid check/build/inspect`. A unified front end is a separate cross-programme decision. |
| Emperor 13-stage pipeline | `emperor::stage::NAMES` is Linux-native (`native-linux-lowering`, `elf-assembly-linking`). Nevada's `orchestrate.rs` already shows how to report a non-ELF target honestly: stages it cannot run are `NOT_APPLICABLE` or `BLOCKED` and are never shown as passing. Arachnid should copy that pattern. |
| Proposed capability tiers (Legacy / Standard / Enhanced) | These are invented tiers. LG publishes a concrete axis: one web engine per platform version (§2). Profiles should key on that axis, not on made-up tiers. |

## 2. Verified platform facts

Each fact below was checked against LG's developer pages on 20 November
2026. Facts marked *(unverified)* are still assumptions.

**Web engine per platform.** Source: webostv.developer.lge.com
"Web API and Web Engine".

| webOS TV | Year | Engine |
| --- | --- | --- |
| 26 | 2026 | Chromium 132 |
| 25 | 2025 | Chromium 120 |
| 24 | 2024 | Chromium 108 |
| 23 | 2023 | Chromium 94 |
| 22 | 2022 | Chromium 87 |
| 6.x | 2021 | Chromium 79 |
| 5.x | 2020 | Chromium 68 |
| 4.x | 2018–19 | Chromium 53 |
| 3.x | 2016–17 | Chromium 38 (QtWebEngine 5.2.1) |
| 2.x / 1.x | 2015 / 2014 | WebKit 538.2 / 537.41 |

The platform version is readable through Luna `getSystemInfo().sdkVersion`.
**Packaged (file-scheme) apps get no cookies.** Persistent state must use
web storage.

**Input.** Source: "Magic Remote".
- Two remotes ship: the Magic Remote (pointer plus 5-way) and the
  conventional remote (5-way only, with media keys).
- **Every app must support 5-way.** Pointer is the primary mode, but
  5-way is mandatory.
- Pressing an arrow key switches to 5-way and hides the cursor. Shaking
  the remote switches back to pointer. The `cursorStateChange` event
  reports cursor visibility.
- Pointer mode uses standard mouse events (`mouseover`, `click`). While
  a system overlay is shown, the app receives no pointer events; it
  observes this through `blur`/`focus`.
- Keycodes: Left 37, Up 38, Right 39, Down 40, OK 13, Back 461.
  Red/Green/Yellow/Blue are 403–406. Conventional-remote media keys:
  Play 415, Pause 19, FF 417, Rewind 412, Stop 413.
- Back: by default the platform drives `history`.
  `disableBackHistoryAPI: true` delivers Back to the app instead. If
  there is no previous page, webOS 6.0+ shows an exit popup, and 5.0 or
  lower goes to Home.

**Packaging metadata.** Source: "appinfo.json".
- Required fields: `id`, `title`, `type` (only `"web"`), `main`, `icon`
  (80×80 PNG), `version`. `version` is three integers with no leading
  zeros, and the same version cannot be uploaded twice.
- `id` rules: lowercase, digits, `-` and `.`; at least two characters.
  Reserved prefixes are `com.palm`, `com.webos`, `com.lge` and
  `com.palmdts`. Avoid `-` if JS services are ever added.
- **Web apps render at 1920×1080 (default) or 1280×720 only. UHD is not
  available to web apps.** The Ocelli geometry problem is therefore two
  fixed logical viewports, not arbitrary display sizes.
- Lifecycle: `handlesRelaunch` together with the `webOSRelaunch` event and
  `PalmSystem.activate()`. `requiredMemory` is in MB.

**Toolchain.**
- The CLI is the npm package `@webos-tools/cli` (`ares-package`,
  `ares-install`, `ares-launch`, `ares-setup-device`, and others).
- Recent releases need Node ≥ 20 *(read from the release notes; not
  checked against the installation guide)*.
- OSE uses the separate `@webosose/ares-cli`.

**Still to verify before Phase 0 closes:**
- IPK container layout. Assumed to be a Debian-style `ar` archive holding
  `debian-binary`, `control.tar.gz` and `data.tar.gz`; needs a byte-level
  check against `ares-package` output.
- Whether `ares-package` output is reproducible (timestamps).
- Which webOS TV emulator versions exist, and whether they are
  VirtualBox images.
- Safe-area and minimum-size guidance in LG's design guidelines.
- Whether third-party web apps get any voice-intent API.
- Whether OK in pointer mode fires both `keydown 13` and `click`. This
  decides the "duplicate modality-triggered actions = 0" benchmark.

## 3. Host state

Checked on 20 November 2026.
- Node, npm and the ares CLI: **not installed**.
- `VBoxManage`: present.
- Disk: 31 GB free (93% used).
- LG TV with Developer Mode: none known.

Every device, emulator, performance and LG-submission gate is therefore
**BLOCKED**. Packaging can still be built and byte-checked offline.

## 4. Proposal items to drop or reframe

- **Eight subsystem names.** They are fine as module names inside one
  crate. Do not mirror them as crates, contracts or gates until each one
  has content. Ganglion, Hemolymph and Stridulation have no
  implementation path before Phase 3.
- **"Unsupported hardware is never silently assumed".** Keep this. It is
  Nevada's presence ≠ usability rule (dcr), and Arachnid should reuse it
  rather than restate it.
- **The neural, Bluetooth, camera, DRM and system-settings rows of §8.**
  These are refusals until a Luna permission model is specified. Each one
  is a compile-time refusal with a diagnostic, never a feature that
  quietly degrades.
- **Benchmarks.** Static reachability = 100%, traps = 0, unsupported
  diagnostics = 100% and reproducible packaging = 100% can all be checked
  offline in this repository. Latency, cold launch and FPS need a
  physical TV and stay BLOCKED until one exists. They must be reported
  per webOS version (§2).
- **"No external compilation server".** Keep this, and extend it to
  packaging. Arachnid should write the IPK itself, deterministically in
  Rust, and use `ares-package` only as a differential oracle. Install and
  launch must still go through `ares-*`, because the Developer Mode
  transport is LG's.

## 5. Phases

Each phase leaves the tree green. No phase claims more than its tests
show.

### Phase 0 — Shared TV focus and the Arachnid skeleton

**Decision needed:** where the shared focus engine lives. Recommendation:
move `nevada::focus` into a target-neutral crate (`goya-tv-focus`,
contract `goya.tvfocus/0.1.0`). Nevada re-exports it with no change in
behaviour, and its 40 tests must pass unchanged. Having Arachnid depend on
a crate whose contract is `nevada.androidtv` would leak the Android TV
identity into webOS.

Then add to the shared crate:
- **Trap detection.** Find every strongly connected set of focusable
  elements that has no edge to an essential element outside it, and
  check that Back leaves every scope.
- **Nested scopes.** A focus scope per child Gscene, with explicit
  enter/exit edges, and a rule that focus never crosses an unentered
  scope.

`crates/arachnid` (`goya-arachnid`, contract `arachnid.webos/0.0.1`)
starts with three pieces:
- `profiles`: one profile per webOS version, from the §2 table, holding
  the engine, logical viewport and documented keys.
- `keys`: the §2 keycode map.
- `diagnostics`: a `GY-AR-6xx` code space, reserved now so codes never
  have to move.

No scene admission and no output.

### Phase 1 — Packaging: Spinneret

- A deterministic `appinfo.json` writer that enforces the §2 field rules.
- A deterministic IPK writer with fixed mtimes, owners and order, and no
  host paths.
- Output layout follows the proposal's `dist/arachnid/` tree plus
  `compilation-manifest.json`, using Maja's create-new, never-overwrite
  directory rule.

Acceptance:
1. Two builds are byte-identical.
2. The IPK structure matches `ares-package` output for the same app
   directory. This requires Node and `@webos-tools/cli`, which needs user
   approval to install, and stays BLOCKED until then.
3. `ares-install` succeeds on an emulator or TV. BLOCKED.

### Phase 2 — Static scene to webOS

Admit the same subset as `maja.web.local.v1` (one static `@cell`) and emit
it at 1920×1080 with safe-area margins. Share Maja's emitter by extracting
it, not by forking it. First launch target: the emulator, if one is
available.

### Phase 3 — Interaction correctness: Tarsus

This phase depends on two things outside Arachnid. The cross-target
browser state contract that Maja is also missing. And the unresolved
grammar questions from NEVADA-1 (gesture vs intent declarations, and where
"essential" comes from), which go through `docs/contracts.md`.

Then:
- Lower scenes to focus rectangles; this closes N0-FOCUSGRAPH for both
  targets.
- Write `navigation-graph.json`.
- Generate a JS runtime that consumes the precomputed graph for 5-way and
  native mouse events for pointer, with one action dispatch so a single
  intent fires once.
- Handle `cursorStateChange` modality switching and Back (via
  `disableBackHistoryAPI`).
- Add a bounded runtime focus resolver for dynamic lists. It must agree
  with the static rule on static input.
- Differential tests against Goya's canonical D1 executor.

### Phase 4 onward — not scheduled

Generation profiles through Caprichios, media, Luna bindings, lifecycle
and resource retention, and security. Each needs its upstream programme
to exist first: Mesut in-browser, Tauromaquia, a Luna permission model,
and an `ie-contract` revision with a web transport. Scheduling them now
would plan against systems that are not in the repository.

## 5a. Phase status (20 November 2026)

| Phase | Status |
| --- | --- |
| 0 — focus and skeleton | Done, with changes from the plan. Focus engine **copied** rather than extracted (owner's decision). Trap detection added. Engine/model caprichios replace the planned `profiles` module. Diagnostics use `GY-AR-6xx`. Nested scopes not started. |
| 1 — packaging | Done offline. Reproducible IPK, checked with GNU tools. No `ares-package` differential, because the decision was to build from the ground up instead of installing Node. Installer acceptance BLOCKED until the TV run. |
| 2 — static scene | Done offline for cells and buttons at the 1920×1080 ontology limit, rendered in headless Chrome. Launch on a TV BLOCKED. |
| 3 — interaction | The remote runtime (5-way, pointer, modality, single activation) is done and browser-tested. `@interaction`/`@state`/`@input` lowering is not started; it waits on the shared browser state contract. |
| Deploy | `arachnid getkey` / `deploy` are implemented without ares, following the `@webos-tools/cli` protocol and the webOS 26 temp-directory rules. Not yet run against a TV. |

## 6. Open decisions for the owner

The answers given on 20 November 2026 are in [ARACHNID-1](ARACHNID-1.md):
copy the focus engine; support 4.x onwards; the owner has a TV that can
enter Developer Mode; build missing pieces from the ground up; use a
dedicated branch. Question 5 (where Inquisition, Tauromaquia and the
browser version of Mesut are specified) was answered by "build from the
ground up". This increment built only what it needed: the focus and trap
validator, packaging and deploy. The original questions follow.

1. Extract `goya-tv-focus` from Nevada, or have Arachnid depend on
   `goya-nevada` directly?
2. Lowest supported webOS TV version. Recommendation: **webOS TV 4.x
   (Chromium 53)**. It gives a single Chromium baseline, and the WebKit
   1.x/2.x generations would need their own compatibility tier.
3. May Node ≥ 20 and `@webos-tools/cli` be installed for differential
   packaging and device work? Where should they go (`~/.local/opt`, as
   with the Aragon toolchain)?
4. Is a physical LG TV with Developer Mode, or the LG emulator, available?
5. Do Inquisition, Tauromaquia and Mesut-in-browser exist outside this
   repository? If so, where are their specifications?
6. Which branch should Arachnid work go on? This file was written in a
   checkout of `nebula/v0.1` and is not committed.
