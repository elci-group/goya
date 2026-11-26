# VENTADA-0 — Phase 0: architecture reconciliation

Status: **Phase 0 evidence, not a gate pass.** Date: 25 November 2026. Written in a
`nebula/v0.1` checkout and not committed. Directive: *Ventada — Enterprise
Technical Directive* v1.0.0 (pasted by the owner, same day).

The directive's closing qualification says its crate boundaries, Windows
backends, tiers and budgets are recommendations, and that Phase 0 must
reconcile them with this repository first. This file does that.

## 1. Headline

* **Most of Ventada already exists as a closed host.** ADR-RESOLVER-001
  Amendment 3 makes `windows` a served resolver target (`WindowsHost`,
  compiler Sash, surface `win32`). Today that surface is **closed**: `present`
  returns a projection and opens no window. Ventada's real work is opening it.
* **The canonical resolver cross-links for Windows from this host.**
  `goya-resolve` builds for `x86_64-pc-windows-msvc` and
  `aarch64-pc-windows-msvc` with `cargo xwin`. With a static CRT it imports
  only `KERNEL32`, `ntdll`, `bcryptprimitives` and
  `api-ms-win-core-synch-l1-2-0`, which is the same Windows 8 floor as Sash's
  `win8` (`crates/sash/src/api_floor.rs`). Clippy is clean for the x64 target.
* **Nothing has run on Windows.** This host has no Windows machine, VM or Wine.
  This is the same blocker as Sash Phase 0, so **VNT-G03 cannot start**.

## 2. Reconciliation: directive names against the repository

| Directive name | In this repository | Consequence for Ventada |
| --- | --- | --- |
| Goya Resolver (§5 steps 1–3, 7) | `crates/goya-resolver`, `GY-RESOLVER/0.1`, ADR-RESOLVER-001 with 12 amendments: GSSH admission, the D1 machine from `goya-executable`, indexed invalidation, reload, suspend and resume, hit test, caret, scroll. Committed on `nebula/v0.1` as `b408ee19`. | Ventada is a **host over `goya-resolver`**. The ADR forbids forking admission or D1, and directive §22.2 says the same. Steps 1–3 and 7 are already implemented. |
| `windows` target (§7) | `goya-resolver-host::WindowsHost`. Surface `win32` is closed (Amendments 3 and 4). | No new ADR amendment is needed to *name* Windows. Opening the surface needs one: **Amendment 13**, "the `win32` surface opens". |
| Typed execution representation, tiers V0–V1 (§6) | ADR Decision 1: no second IR, VM or bytecode. D1 runs on `D1Program`. Every launch parses and admits again. There is no cache. | V0 is the resolver as it is. **V1 (cached resolution) needs an ADR change**: either a serialised admitted scene or a content-addressed admission cache. V2 and V3 conflict with Decision 1 and wait for evidence, as the directive itself says. |
| D2, asynchronous logic | Refused: `GY-RESOLVER-D2` (Decision 4). | Ventada inherits the refusal. Files, network and notifications are D2-shaped, so §7 rows beyond window, input, clipboard and accessibility are blocked on this decision. |
| Sash (§3.3, VNT-G04) | `crates/sash`: emits PE images directly, with an interactive profile (GSSH/4-live counter, `@iden` UIA annotation, D9 live region, clipboard). **Nothing launched.** | The parity oracle exists, but it is also unexecuted. VNT-G04 needs **both** running on Windows. The D9 announcement wording is already mirrored between Sash and `goya_lang`, which is a parity anchor. |
| Graphics, GVI family (§8) | `goya-gvi` (reference decoder, GVI-D/A/O/L, packed and expanded loops), `goya-gru`, `goya-gva`, `goya-scene-graphics`. | §8 is satisfied by **reuse**, with no new format. Residency policies (§8.2) map to GVI's existing packed and expanded loops. |
| Renderer | Emperor's `scene` runtime (**`arachnid/v0.2` only**) paints GSSH/4-live scenes with tiny-skia and `ab_glyph` into an RGBA frame for `goya-wayland`. It has its **own** D1 executor and layout walk, not the resolver's. | Copying it would duplicate canonical semantics (§22.2). The painter must first be split from `goya-wayland` and driven by the resolver's `NativeSurface`. Then the same painter serves Emperor, Viuda and Ventada. The first Windows presenter is that software raster blitted to a Win32 window (`StretchDIBits` or a DIB section). Direct2D and Direct3D come later and only with benchmark evidence (§7). |
| Accessibility, UI Automation (§7) | Emperor publishes an AccessKit tree through `accesskit_unix` (AT-SPI). Sash writes UIA annotations through `IAccPropServices`. | **`accesskit_windows`** is the same tree over UIA. That is reuse rather than a new `ventada-accessibility` crate. |
| Win32 bindings | `windows-sys` 0.59 and 0.61 are already in `Cargo.lock` (transitively). | No new binding family is needed. |
| Caprichios (§9) | `goya-caprichios` (registry, negotiation, artifacts). There are no Windows hardware caprichios. | §9's profiles belong in `goya-caprichios`. None exist yet. |
| Durable state, romántico (§13–14) | `goya-state`, `goya-romantico`. D8 records, D11–D13 defaults. Emperor uses the `linux.xdg-state.v1` store token. | The resolver classifies durable state and does not write it (Decision 5). Ventada needs a **new store token** (for example `windows.localappdata.v1`), with D8 records compatible with the other hosts. |
| Adios (§13) | No code or specification (`docs/graphics/README.md`). | The resolver's `Phase` is the only lifecycle model. |
| Tauromaquia (§12) | No code or specification (`docs/arachnid/roadmap.md` §2). | SEC-01…14 have nothing to integrate with. Job Objects, AppContainer and restricted tokens would be Ventada-local until Tauromaquia exists. |
| Iberia, España (§15) | Named in docs and a few crates (`docs/iberia-mvp.md`). | Phase 6, and not a dependency (§22.11). |
| bloom, Pelt, Delectable (§15, §19.1) | **Not mentioned anywhere** in this repository. | VNT-B12 cannot be planned until Delectable is specified. |

### Crate layout (§4.1)

The directive asks for fifteen `ventada-*` crates in a separate repository,
then requires reuse wherever shared crates exist. Against this workspace, most
of them collapse:

| Proposed | Reconciled to |
| --- | --- |
| core, admission, resolver | `goya-resolver` (existing) |
| graphics, residency | `goya-gvi`, `goya-scene-graphics`, plus the painter split above |
| caprichios | `goya-caprichios` |
| continuity | `goya-state`, `goya-romantico` |
| accessibility | `accesskit_windows` inside the host |
| win32, cli | **`crates/goya-ventada`**: the only new crate. It contains the Win32 window, message loop, input and UIA adapter, plus a `ventada` binary. |
| directx, security, scheduler, cache, telemetry | Not created until a phase needs them and evidence supports them |

## 3. Build evidence (Linux host, no launch)

Reproduce with `scripts/ventada-phase0-build.sh`. It needs `cargo-xwin`. It
pins `XWIN_SDK_VERSION=10.0.22621` because the default SDK
(`Win11SDK_10.0.26100`) failed with "unable to find x86_64 headers".

| Fact | x64 | ARM64 |
| --- | --- | --- |
| `cargo check` (`goya-resolver`, `goya-resolver-host`) | pass | pass |
| `cargo clippy -D warnings` | pass | not run |
| `goya-resolve.exe`, dynamic CRT | 1,241,600 B; imports `VCRUNTIME140.dll` and `api-ms-win-crt-*` | 1,012,736 B; same imports |
| `goya-resolve.exe`, `+crt-static` | 1,337,344 B, sha256 `9fc2a57a…3705d` | 1,098,752 B, sha256 `4c21f4a2…ae58a` |
| Static import closure | `KERNEL32`, `ntdll`, `bcryptprimitives`, `api-ms-win-core-synch-l1-2-0` | same |
| Subsystem / OS version | CUI, 6.0 | — |

On Linux, `goya-resolver` and `goya-resolver-host` pass their tests (38
tests). `goya-resolve crates/sash/fixtures/interactive-live.gscene --target
windows --present` admits under GSSH/4-live and prints `surface: win32
closed`, with four projected nodes. The resolver's default viewport is
800×600 and Sash's link uses 320×160 DIP, so the frames are not yet
comparable for VNT-G04.

Reading: a shared Ventada runtime with a static CRT is about 1.3 MB **before**
any windowing, rasteriser, font or accessibility code. No §17 budget is
measured or implied by these figures. Import inspection is not a
clean-machine test, as Sash's `api_floor.rs` also notes.

## 4. Gate status

| Gate | Status |
| --- | --- |
| Phase 0: architecture reconciliation | **Done** (this file). |
| VNT-G01 Workspace integrity | Not met: there is no Ventada crate. The resolver crates build, test and lint. The full workspace test run was not repeated. |
| VNT-G02 Resolver correctness | Inherited from `GY-RESOLVER/0.1` (differential test against `D1Application`). Not a Ventada claim. |
| VNT-G03 Native execution | **Blocked:** no Windows hardware is reachable from here |
| VNT-G04 … VNT-G12 | Not started |

## 5. Proposed next steps (none started)

1. ~~Commit the resolver.~~ **Done:** `b408ee19` on `nebula/v0.1`, owner's
   decision on 25 November. It was checked in a clean worktree of `47ebae1d`.
2. **Split the scene painter** from `emperor-runtime` (`arachnid/v0.2`) into a
   platform-free crate that paints a resolver `NativeSurface` to RGBA. This
   also gives Viuda its first presenter.
3. **ADR-RESOLVER-001 Amendment 13:** the `win32` surface opens, so
   `NativeSurface.opened` can be true.
4. **`crates/goya-ventada`:** a Win32 window with a DIB blit, pointer,
   keyboard and `WM_CHAR` routed to `point`, `edit` and `enter`, plus
   `accesskit_windows`, behind a `ventada run <scene>` command. The first
   target is Sash's `interactive-live.gscene` at 320×160 DIP, the scene Sash
   already linked. This is the narrowest VNT-G04 pair. Goya Pairs (VNT-B03)
   comes after.
5. **A Windows machine** (decision 1). It unblocks VNT-G03 and Sash's
   Phase 0 at the same time.

## 6. Decisions for the owner

1. **Hardware:** where is the "ageing Windows laptop in the Goya evaluation
   inventory" (§19.2)? Nothing in this repository or in the Sash records
   reaches one. Can it be reached over SSH or OpenSSH-for-Windows?
2. ~~Repository~~ **Decided:** one workspace crate, `crates/goya-ventada`.
3. ~~Branch~~ **Decided:** `nebula/v0.1`. The resolver is committed there
   (`b408ee19`). The Emperor scene painter is still on `arachnid/v0.2`
   only, so step 2 has to bring it across or replace it.
4. **V1 cache:** does persistent cached resolution reopen ADR Decision 1, or
   is a content-addressed admission cache acceptable under it?
5. **D2:** the resolver refuses D2. Is D2 execution in scope for Ventada
   (files, network, notifications), and does that reopen Decision 4?
6. **Adios, Tauromaquia, bloom, Pelt, Delectable:** none has code or a
   specification here. Where are they specified?
