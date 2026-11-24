# GY-NEB-001

Nebula, the COSMIC desktop profile and qualification lane of Emperor. Revision 0.1.0. Repository copy recorded 19 November 2026. Base commit `6193d310603fa2668d8bde6ac3159cdec80f2ef9` on `nebula/integration`. Owner: the Goya compiler programme. Parent contract: [GY-EMP-001](../emperor/GY-EMP-001.md).

This file is the contract an engineer implements. It does not certify any NEB milestone. It does not certify EMP-M1. It does not designate Nebula or Emperor state of the art.

Current working-tree evidence (24 November 2026): NEB-001 is Measured in
[`evidence/nebula/neb-001-2026-11-24.json`](../../evidence/nebula/neb-001-2026-11-24.json)
after a native COSMIC Wayland live run with real seat button presses. The
base-commit status lines below remain historical. This does not certify a later
Nebula milestone or Emperor release.

Normative sentences use SHALL. Each section ends with repository status. "Implemented" means code at the base commit does the stated thing. It is not a qualification result. "Measured" means a command ran on the qualification host and its output is recorded in a named file. Anything else is Unmet or Blocked, as in [gap-ledger.md](gap-ledger.md).

## 1. Mission

Nebula SHALL qualify Emperor applications on the COSMIC desktop, starting with one daily Pop!_OS COSMIC workstation. Nebula SHALL specialise Emperor's compilation infrastructure and SHALL NOT duplicate it. Platform-neutral improvements SHALL remain shared. COSMIC-specific behaviour SHALL reside in `crates/nebula` and in its Caprichio selection.

A Nebula build SHALL be an Emperor build. It SHALL admit the same scenes, lower to the same `GY-EMP-NIR/1`, link the same `emperor-runtime`, and pass the same shared oracle. The COSMIC profile MAY change desktop-facing choices that GY-EMP-001 leaves to the host: the baked theme, the resize policy, and the qualification checks run after the build. It SHALL NOT change stored state, accepted inputs, transitions, focus order, or clipboard policy.

The qualification host recorded on 19 November 2026 is Pop!_OS 24.04 with `cosmic-comp` package version `0.1~1776708607~24.04~21679f2`, read with `dpkg-query` (`evidence/nebula/probe-2026-11-19.json`, field `compositor`). That is the installed package version. It was not checked against the running compositor process.

Repository status: the mission is specified. No NEB milestone is Measured. EMP-M1 is not certified.

## 2. Scope and non-goals

In scope: the COSMIC desktop profile in `emperor build --desktop cosmic`; the `nebula` crate; COSMIC session detection; the COSMIC protocol profile; reading the COSMIC theme at build time and mapping it onto Chroma; selecting shared behaviour for COSMIC, such as the resize policy; and qualification records for COSMIC under `evidence/nebula/`.

Non-goals. Nebula SHALL NOT:

- define a Nebula dialect, a Nebula syntax, or a Nebula-only scene construct;
- fork lowering, code generation, the window host, the renderer, or the oracle;
- put xdg-shell, layout, D-Bus, rendering, packaging, or signing code in `crates/nebula` when the code is not COSMIC-specific;
- bind or depend on a `zcosmic_*` or `cosmic_*` protocol from an application;
- count an Xwayland run as COSMIC qualification;
- give the running App a capability that the Emperor profile does not declare;
- follow theme changes at run time in this revision;
- add a COSMIC compositor optimisation without a measurement that motivates it;
- narrow or replace Emperor's release gate.

Repository status: the scope is specified. `crates/nebula/src/lib.rs` states the same split. Nothing in the non-goal list is implemented, which is the intended state.

## 3. The profile mechanism

### 3.1 Selection

`emperor build --desktop cosmic` SHALL select the COSMIC desktop profile. `build` without `--desktop` SHALL behave exactly as GY-EMP-001 §10 specifies, with the same output files and the same exit codes. An unknown `--desktop` value SHALL exit 2.

The Emperor profile string (`emperor.linux.interactive/0.0.1` for the counter) SHALL NOT change under `--desktop cosmic`. The desktop profile SHALL be recorded as a separate field in `build-report.json`, naming `nebula.cosmic.app-profile/0.1` (`PROFILE_ID` in `crates/nebula/src/protocols.rs`) and the `nebula` version.

Status: Pending NEB-E evidence.

### 3.2 What the profile does at build time

Under `--desktop cosmic`, `build` SHALL:

1. Read the COSMIC theme with the `nebula` theme reader (section 6) and bake the resolved Chroma palette into the build output.
2. Record in `build-report.json` the theme name, the variant, the config root used, the theme format (`v1`), and a SHA-256 over the theme files that were read, so that two builds with different desktop themes are distinguishable.
3. Select the proportional resize policy (section 3.4).
4. Keep every file in GY-EMP-001 §1.2, unchanged in meaning.

A build that cannot read a theme SHALL say so in the report and SHALL fall back to the default Chroma palette. It SHALL NOT invent COSMIC colours.

Status: Pending NEB-E evidence.

### 3.3 What the profile does after the build

When `WAYLAND_DISPLAY` is set, the smoke run SHALL record the session verdict from `nebula::session::classify` and the compositor identification. Compositor identification SHALL distinguish the installed package version from the running process. Until the running process is checked, the record SHALL say that the version is the installed package version.

A smoke run SHALL count toward COSMIC qualification only when `XDG_CURRENT_DESKTOP` has a `COSMIC` component, `XDG_SESSION_TYPE` is `wayland`, `WAYLAND_DISPLAY` is set, and the window was created through the Wayland connection. An X11 `DISPLAY` in that session SHALL be recorded as Xwayland and SHALL NOT count (`crates/nebula/src/session.rs`).

Status: session classification and the installed-package version are Implemented in `crates/nebula/src/session.rs` and recorded in `evidence/nebula/probe-2026-11-19.json`. Their use in Emperor's smoke output is Pending NEB-E evidence.

### 3.4 Resize policy

Emperor's default is "resize without reflow" (GY-EMP-001 §4.2). `goya_layout::proportional::ResizePolicy::Fixed` encodes that rule and is the identity. The COSMIC profile SHALL select `ResizePolicy::Proportional`, with the tiled edges taken from `goya_wayland::ToplevelState::tiled`. Hit-testing SHALL go through `Placement::to_design`, so that the shared hit map still sees admitted design coordinates. The shared oracle SHALL NOT observe the policy.

Status: `ResizePolicy` and `Placement` are Implemented in `crates/goya-layout/src/proportional.rs` and are not called. The cosmic profile's use of them is Pending NEB-E evidence.

Repository status: the mechanism is specified. `--desktop cosmic` is not present at the base commit (`crates/emperor/src` has no `--desktop` flag). EMP-M1 is not certified.

## 4. Responsibilities

The user's proposal listed eight milestones. This table re-cuts that work by owner. Binding decision 2 in [README.md](README.md) governs it.

### 4.1 Nebula owns

| Work | Location | State at the base commit |
| --- | --- | --- |
| COSMIC session detection and Xwayland exclusion | `crates/nebula/src/session.rs` | Implemented |
| `cosmic-comp` version identification | `crates/nebula/src/session.rs` | Implemented for the installed package only |
| COSMIC protocol profile and classification | `crates/nebula/src/protocols.rs` | Implemented |
| COSMIC theme reader (format `v1`) and Chroma mapping | `crates/nebula/src/theme.rs` | Implemented |
| COSMIC geometry record (radii, spacing, gaps, active hint, frosted flag) | `crates/nebula/src/theme.rs` (`CosmicGeometry`) | Recorded; not consumed |
| Caprichio selection for the COSMIC host | `crates/nebula/src/caprichios.rs` | Implemented; nothing selected is COSMIC-specific |
| Probe record `nebula.probe/0.1` | `crates/nebula/src/probe.rs` | Implemented |
| The `--desktop cosmic` profile choices (theme bake, resize policy selection, smoke checks) | `crates/emperor` calling `crates/nebula` | Pending NEB-E evidence |
| COSMIC qualification records | `evidence/nebula/` | One probe record |

### 4.2 Shared upstream (Nebula qualifies)

| Proposal item | Upstream owner | What Nebula adds |
| --- | --- | --- |
| Tiling-aware window state (NEB-002) | `goya-wayland` | Observation of the states on COSMIC |
| Proportional geometry (NEB-002) | `goya-layout` | Selects the policy; qualifies a resize on COSMIC |
| Fractional scale (NEB-002) | `goya-wayland`, `emperor-runtime` | A COSMIC run against `wp_fractional_scale_manager_v1` |
| Notifications and portal appearance (NEB-003) | `goya-freedesktop` | A run against `cosmic-notifications` and the COSMIC portal |
| Desktop entry and icon install (NEB-003, NEB-006) | Emperor (GY-EMP-LINUX/1) | A COSMIC launcher check |
| Text editing, IME, clipboard (NEB-004) | `goya-wayland`, `emperor-runtime` | A COSMIC input-method and clipboard run |
| Pointer gestures, touch, drag-and-drop (NEB-004) | `goya-wayland`, `emperor-runtime` | A COSMIC seat run |
| Hardware-accelerated rendering (NEB-005) | Emperor (GY-EMP-RENDER/1) | A COSMIC dmabuf run on a named GPU |
| Packages and signing (NEB-006) | Emperor (GY-EMP-001 §13) | A Pop!_OS COSMIC install, launch, and uninstall |
| Benchmarks (NEB-007) | Emperor (GY-EMP-001 §11) | The same protocol on the COSMIC workstation |
| ZCode client (NEB-008) | Emperor admission and profiles; `goya-zcode-native` | A COSMIC run of the compiled client |

### 4.3 Dropped or deferred

| Item | Decision |
| --- | --- |
| COSMIC compositor optimisations | Dropped until a measurement motivates one. |
| A Nebula dialect or Nebula-only syntax | Rejected. |
| Use of `zcosmic_*` or `cosmic_*` protocols by applications | Rejected. Observed only. |
| Live theme following | Deferred. Unmet. It needs either a runtime capability or a `SettingChanged` subscription, and neither is admitted. |
| Theme format `v2` | Deferred. The host has an empty `v2` directory beside `v1` (`evidence/nebula/probe-2026-11-19.json`, `theme.other_versions_present`). |
| Xwayland as a COSMIC path | Rejected. |

Repository status: the split is specified. The upstream items are each Unmet or Blocked as section 9 states. No dropped item has code.

## 5. COSMIC protocol profile

`COSMIC_PROFILE` in `crates/nebula/src/protocols.rs` is the authoritative table. Profile identifier: `nebula.cosmic.app-profile/0.1`. [protocol-profile.md](protocol-profile.md) is a human-readable copy, and the source wins where they differ.

The profile SHALL classify every advertised global into one of four groups:

- Required: `wl_compositor`, `wl_shm`, `wl_seat`, `xdg_wm_base`. A missing required global makes `nebula probe` exit 4.
- Optional: 19 rows. Each names the Goya feature it serves, a minimum version, whether `goya-wayland` binds it, and the fallback when it is absent or below the minimum.
- COSMIC-private, not used: 8 named rows, plus any other `zcosmic_*` or `cosmic_*` global observed. They serve the COSMIC panel, dock, applets, and settings. An application SHALL NOT bind them.
- Unlisted: every other global. Recorded, not used.

Classification SHALL be a pure function of the observed list. Advertisement SHALL NOT be treated as behaviour (GY-EMP-001 §4.3).

On 19 November 2026 the host advertised 55 globals. Required 4 of 4. Optional 15 of 19. Absent with the fallback in effect: `xdg_toplevel_icon_manager_v1`, `xdg_wm_dialog_v1`, `xdg_toplevel_tag_manager_v1`, `wp_color_manager_v1`. COSMIC-private observed: 8. Unlisted: 28 (`evidence/nebula/probe-2026-11-19.json`, `protocols.counts`). One seat reported pointer, keyboard, and touch.

Repository status: Implemented in `crates/nebula/src/protocols.rs`. Presence is Measured for one host on one date. No behaviour of any listed protocol is qualified by that record.

## 6. Theme mapping

### 6.1 Input

The reader SHALL read `<config>/cosmic/com.system76.CosmicTheme.{Dark,Light}/v1/` and `com.system76.CosmicTheme.Mode/v1/is_dark`, where `<config>` is `$XDG_CONFIG_HOME`, else `~/.config`, or an explicit `--config-root DIR`. It SHALL read format `v1` only and SHALL refuse any other requested version. A config without `v1` SHALL be reported with the versions it does have. Unknown fields SHALL be ignored. A missing or unparsable key SHALL degrade that key only, and the report SHALL list it. The reader SHALL NOT write.

### 6.2 Mapping rules

COSMIC colours are absolute. Chroma themes are relational. The mapping SHALL fill Chroma `Palette` slots directly and SHALL NOT synthesise relations:

| Chroma slot | COSMIC source |
| --- | --- |
| `bg` | `background.base` |
| `fg` | `background.on` |
| `accent` | `accent.base` |
| `border` | `background.divider` |
| `ok` | `success.base` |
| `err` | `destructive.base` |
| `muted` | Chroma's 55% blend of `fg` toward `bg`. COSMIC has no muted key. |

Warning, text on accent, and the primary and secondary containers have no `Palette` slot and SHALL be carried as extras. The accent SHALL also be given as an OKLCH anchor. Colours with alpha SHALL be composited over `background.base`. Hover, pressed, selected, and disabled states are not mapped.

Corner radii, spacing, gaps, the active hint width, and the frosted flag SHALL go to `CosmicGeometry`. Nothing in Goya consumes them yet.

### 6.3 Contrast

Contrast SHALL be measured against WCAG AA, or AAA when COSMIC high contrast is on, and SHALL be reported. It SHALL NOT be enforced by rewriting COSMIC's colours. A pair below target SHALL stay visible in the build report.

On 19 November 2026 the host theme was `cosmic-dark`: `bg` `#1B1B1B`, `fg` `#E7E7E7`, `accent` `#63D0DF`. `fg/bg` measured 13.93. `muted/bg` measured 3.85, below AA, and was reported, not fixed (`evidence/nebula/probe-2026-11-19.json`, `theme.chroma.contrast`). The failing colour is the Chroma blend, not a COSMIC key.

### 6.4 Baking

The resolved palette SHALL be fixed at build time. The running App SHALL NOT read `~/.config/cosmic`. A theme change after the build SHALL NOT change a running App. Section 3.2 lists what the build report records.

Repository status: the reader and the mapping are Implemented in `crates/nebula/src/theme.rs` and Measured once in the probe record. Baking into an Emperor build is Pending NEB-E evidence. Emperor's paint does not read Chroma (EMP-M1-26). Live theme following is Unmet.

## 7. Security

Nebula SHALL NOT add a runtime capability. Under `--desktop cosmic`, `capability-manifest.json` SHALL record `filesystem` `none` and network, subprocess, and web content false, exactly as GY-EMP-001 §7 requires. Baking the theme is the mechanism that keeps the App from reading the user's config directory.

The theme files are a compile-time input. They are read by the compiler on the build host. They are untrusted in the same sense as the scene. The theme reader at the base commit reads each file with `std::fs::read_to_string` and no size limit (`crates/nebula/src/theme.rs`, line 324). Before the reader is a compiler input on a release path, it SHALL bound the bytes read per file, and the bound SHALL be stated in [../emperor/threat-model.md](../emperor/threat-model.md).

A session D-Bus connection from the running App is a new effect. `goya-freedesktop` is a library. Wiring notifications or the portal into an App SHALL wait for a declared capability in an Emperor profile. Nebula SHALL NOT add that capability by itself.

`nebula probe` binds `wl_seat` at version 1 only to read capability bits, creates no surface, and disconnects (`crates/nebula/src/protocols.rs`, `list_globals`). It runs `dpkg-query` by argument vector.

COSMIC-private protocols SHALL NOT be bound by an App. The probe records their presence and nothing else.

Repository status: the capability rule is specified and is inherited from Emperor. The unbounded theme read is Unmet (NEB-GAP-12 in [gap-ledger.md](gap-ledger.md)). No sandbox is installed by Nebula, and none is claimed (EMP-M1-11).

## 8. Evidence

Nebula records SHALL follow [../emperor/evidence.md](../emperor/evidence.md) and SHALL live under `evidence/nebula/`. Each record SHALL also carry the session verdict, the compositor identification with its method, and the `nebula` version. A record SHALL state its scope: protocol presence, a window, or a release check.

A live observation printed by an `#[ignore]` test and copied into crate documentation is a recorded observation. It is weaker than a record under `evidence/nebula/`. The gap ledger says which kind each row has.

Repository status: one record exists, `evidence/nebula/probe-2026-11-19.json`, scope protocol presence only. The NEB-001 record `evidence/nebula/neb-001-<date>.json` is Pending NEB-E evidence.

## 9. Milestones

Each milestone names its acceptance evidence and its owner crate. A milestone passes only when that evidence exists for the COSMIC qualification host. The status column uses the gap-ledger terms.

### NEB-001. Interactive native COSMIC application

The EMP-M1 counter (`crates/sash/fixtures/interactive-counter.gscene`) SHALL be built with `emperor build --desktop cosmic` and launched natively on COSMIC Wayland. Acceptance evidence: `evidence/nebula/neb-001-<date>.json`, containing the ELF SHA-256, the session verdict (COSMIC, Wayland, not Xwayland), the compositor identification, the bound `xdg_wm_base` version, the committed `ToplevelState`, the baked theme digest, the stage list, the oracle line, and a smoke status from a real window with the EMP-M1 input and present checks. A button activation SHALL come from a real seat event, not from the scripted release check.

Owner: `crates/emperor` (flag, smoke output), `crates/nebula` (session, profile, theme). Status: Pending NEB-E evidence.

### NEB-002. Tiling-aware proportional geometry

The App SHALL follow COSMIC's configure sizes with `ResizePolicy::Proportional`, flush against tiled edges, with hit-testing through the inverse. Acceptance evidence: a COSMIC run that records at least these transitions, each with the committed `ToplevelState`, the configured size, the placement, and one hit test through `to_design`: floating to tiled, tiled to floating, an interactive resize, a change of `configure_bounds`, and `suspended` on a workspace switch. A fractional-scale run on COSMIC SHALL record logical size, buffer size, input transform, and font rounding.

Owner: `goya-wayland` and `goya-layout` (shared), `emperor-runtime` (calling them), `crates/nebula` (policy selection). Status: state reporting Implemented, one state Measured as recorded documentation (NEB-GAP-03); proportional placement Implemented and not called; the cosmic policy Pending NEB-E evidence; transitions Unmet; fractional scale Blocked on EMP-M1-17.

### NEB-003. Native appearance and desktop integration

The App SHALL present the baked COSMIC palette through Chroma. COSMIC geometry (corner radius, gaps) SHALL be consumed only through a shared Goya contract, not a COSMIC-only paint path. Notifications and the appearance portal SHALL go through `goya-freedesktop` once an Emperor profile declares the capability. The desktop entry SHALL be installed and SHALL launch the App from the COSMIC launcher. Acceptance evidence: a NEB-001-style record whose presented frame uses the baked palette, with the palette and contrast report in the build report; a notification posted by the App and its `ActionInvoked` signal received; an installed desktop entry launched from the COSMIC launcher, with the `app_id` recorded.

Owner: `crates/nebula` (theme), `goya-chroma`, `goya-freedesktop`, `crates/emperor` (desktop entry, capability). Status: theme read Measured in the probe record; theme bake Pending NEB-E evidence; paint through Chroma Unmet (EMP-M1-26); notifications Measured at library level only (NEB-GAP-06); signal subscription Unmet; App capability Unmet; desktop-entry install Unmet (EMP-M1-23).

### NEB-004. Text editing, clipboard, and gestures

The App SHALL pass the IME cases in GY-EMP-001 §6 on COSMIC's input method path, exchange `CLIPBOARD` text with a native COSMIC application under the scene's clipboard policy, and handle pointer gestures, touch, and drag-and-drop through the shared Goya actions. Acceptance evidence: an input-method log on COSMIC for each case in GY-EMP-001 §6; a clipboard exchange in both directions with a named COSMIC application; a real-seat gesture and touch log. Each interaction SHALL resolve through `CounterSession` or `KeyController` or their successors, not a COSMIC-only state path.

Owner: `goya-wayland`, `emperor-runtime` (shared). Nebula qualifies. Status: Unmet. `zwp_text_input_manager_v3` (v1) is an optional profile row and `goya-wayland` binds it, with text input disabled unless a caller enables it. `zwp_pointer_gestures_v1` (v3) and `zwp_tablet_manager_v2` (v1) were advertised and have no profile row (`evidence/nebula/probe-2026-11-19.json`, `protocols.unlisted`). Adding rows for them is NEB-D work in `crates/nebula/src/protocols.rs`.

### NEB-005. Hardware-accelerated rendering

The App SHALL present through the GY-EMP-RENDER/1 Vulkan path on COSMIC with `zwp_linux_dmabuf_v1`, and SHALL keep the software path. Acceptance evidence: a COSMIC run on a named GPU with device-loss behaviour, the software fallback, and presentation timing from `wp_presentation`.

Owner: Emperor (shared). Nebula qualifies. Status: Blocked on EMP-M1-08. `zwp_linux_dmabuf_v1` was advertised at version 5 and `wp_presentation` at version 2 (`evidence/nebula/probe-2026-11-19.json`). Advertisement is not behaviour.

### NEB-006. Installable signed packages

An Emperor package SHALL install, launch from the COSMIC launcher, upgrade, and uninstall on a clean Pop!_OS COSMIC machine, and its signature SHALL verify with a separately trusted key. Acceptance evidence: the install, launch, upgrade, and uninstall log, and the signature verification, under `evidence/nebula/`. The window icon SHALL resolve through the `app_id` and the desktop entry, since `xdg_toplevel_icon_manager_v1` was absent.

Owner: Emperor (shared). Nebula qualifies. Status: Blocked on EMP-M1-12, EMP-M1-22, and EMP-M1-27.

### NEB-007. Comparative performance benchmarks

The GY-EMP-001 §11 protocol SHALL be run on the COSMIC workstation, with the compositor version pinned, the CPU, GPU, RAM, and kernel recorded, and raw samples kept. A libcosmic application MAY be added as an extra baseline. It SHALL NOT replace the Electron, Tauri, GTK4, and Qt6 set. Acceptance evidence: the §11 records under `evidence/nebula/` for this host.

Owner: Emperor (shared). Nebula qualifies. Status: Unmet (EMP-M1-14). Note: at the base commit, `crates/emperor/tests/cli.rs` line 145 asserts that `frame_rate` is BLOCKED, while `crates/emperor/src/orchestrate.rs` (commit 473db3f, around line 876) measures it when a display is present. That inconsistency is Emperor's. It SHALL be resolved before a frame-rate sample from this host is cited.

### NEB-008. Goya-native ZCode client compiled through Nebula

A ZCode client written as Goya source SHALL be built with `emperor build --desktop cosmic` and run on COSMIC. The existing `goya-zcode` and `goya-zcode-native` binaries run directly on `goya-wayland`. They are not compiled through Emperor, and they do not satisfy this milestone. Acceptance evidence: an Emperor build report and a COSMIC run record for the client, with the client's capabilities declared in an Emperor profile.

Owner: Emperor (admission and profile growth), `goya-zcode-native` (application). Nebula qualifies. Status: Blocked. Emperor admits only the interactive counter (GY-EMP-001 §2.1), and `emperor.linux.interactive/0.0.1` sets subprocess and network false. A ZCode client that starts or attaches to a ZCode CLI (`docs/goya-zcode-native.md`) needs a profile that declares those effects. That profile is Emperor's to define.

Repository status: no milestone is Measured. NEB-001 is Pending NEB-E evidence. NEB-002, NEB-003, and NEB-004 are partly Implemented upstream and not qualified. NEB-005, NEB-006, and NEB-008 are Blocked. NEB-007 is Unmet.

## 10. Relationship to EMP-M1 gaps

Nebula does not close an Emperor row by itself. A NEB record can supply the COSMIC part of the evidence an EMP row asks for. The coordinator updates [../emperor/gap-ledger.md](../emperor/gap-ledger.md). This document does not.

| EMP row | Emperor gap | NEB milestone | What the NEB evidence supplies |
| --- | --- | --- | --- |
| EMP-M1-01 | Compositor not identified in the smoke | NEB-001 | Compositor identification in smoke output. Pending NEB-E evidence. |
| EMP-M1-07 | Compositor matrix not run | NEB-001 | The COSMIC row only. Mutter, KWin, and wlroots stay open. |
| EMP-M1-08 | No Vulkan backend | NEB-005 | A COSMIC run once the backend exists. |
| EMP-M1-09 | No IME qualification | NEB-004 | The COSMIC desktop family's input-method run. |
| EMP-M1-12 | No signed release | NEB-006 | Verification on a COSMIC machine once signing exists. |
| EMP-M1-14 | No comparison | NEB-007 | One host's samples. Not a comparison across hosts. |
| EMP-M1-17 | Fractional scale not qualified | NEB-002 | A fractional-scale run on a named compositor (COSMIC). |
| EMP-M1-18 | One toplevel; no maximise, fullscreen, dialogs | NEB-002 | Observed maximise and fullscreen state on COSMIC. Second toplevel and dialogs stay Emperor's. |
| EMP-M1-21 | Portals, notifications not started | NEB-003 | `goya-freedesktop` runs against `cosmic-notifications` and the COSMIC portal. |
| EMP-M1-22 | No packages | NEB-006 | A Pop!_OS install log per format. |
| EMP-M1-23 | Desktop entry not installed | NEB-003 | An install and a launch from the COSMIC launcher. |
| EMP-M1-25 | Drag-and-drop, gestures, touch absent | NEB-004 | Real-seat runs on COSMIC. |
| EMP-M1-26 | Chroma not consulted | NEB-003 | A baked palette presented through Chroma. HDR and wide gamut stay open; `wp_color_manager_v1` was absent. |
| EMP-M1-27 | No clean-machine install | NEB-006 | A clean Pop!_OS COSMIC machine. |

Nebula does not help with EMP-M1-02 through EMP-M1-06, EMP-M1-10, EMP-M1-11, EMP-M1-13, EMP-M1-15, EMP-M1-16, EMP-M1-19, EMP-M1-20, EMP-M1-24, or EMP-M1-28, except that a COSMIC run is one of the records EMP-M1-28 needs.

Repository status: the mapping is specified. No EMP row changes status because of this document.

## 11. Streams

| Stream | Responsibility | Paths | State on 19 November 2026 |
| --- | --- | --- | --- |
| NEB-A | xdg-shell toplevel state | `crates/goya-wayland` | Merged on `nebula/integration` (f85db7a, merge 6193d31) |
| NEB-B | Proportional geometry | `crates/goya-layout/src/proportional.rs` | Merged (9493c7a, merge 4077085) |
| NEB-C | Freedesktop D-Bus, notifications, portal appearance | `crates/goya-freedesktop` | Merged (5f7858a, merge b30772c) |
| NEB-D | COSMIC profile, session, theme, probe | `crates/nebula`, `evidence/nebula/probe-2026-11-19.json` | Merged (7d39991, merge 169c48e) |
| NEB-E | `emperor build --desktop cosmic`, theme bake, cosmic resize policy, compositor identification, NEB-001 record | `crates/emperor`, `crates/emperor-runtime`, `evidence/nebula/` | In progress. Pending NEB-E evidence. |
| NEB-F | Programme documents | `docs/nebula` | This revision |

A stream is complete only with source, tests, evidence, limitations, and an owner, as GY-EMP-001 §14 requires. No stream has a closing qualification record. NEB-A, NEB-C, and NEB-D have recorded live observations. NEB-B has unit tests only.

Repository status: the assignment is recorded. No stream is marked complete.

## 12. Final instruction

Nebula SHALL be judged by an Emperor application that runs on COSMIC and that an independent person can build, run, and check. A protocol listing, a theme dump, or a library test against `cosmic-notifications` is a step, not a result. When a choice exists between putting code in `crates/nebula` and putting it in a shared crate, the shared crate SHALL be preferred unless the behaviour exists only on COSMIC.

Repository status: this section is the governing contract. No NEB milestone has passed. EMP-M1 has not passed.
