# VIUDA-0 — Phase 0: reconciliation and platform feasibility

Status: **Phase 0 evidence.** Date: 25 November 2026. Directive: *Viuda —
Enterprise Technical Directive* v1.0.0 (pasted by the owner, same day).
Written in a `nebula/v0.1` checkout, then moved to `viuda/v0.1`. The owner
answered decisions 1 and 2 the same day (branch `viuda/v0.1`; Developer
Mode is enough). Phase 1 is [VIUDA-1](VIUDA-1.md).

The directive's §17 states that it was written without looking at this
repository, and asks for reconciliation first. This file does that, and
records the first device evidence for the §3.2 pathway question.

## 1. Headline

* **The canonical resolver runs natively on the owner's TV.** `goya-resolve`
  (GY-RESOLVER/0.1) builds as a static ARMv7 binary of 1,154,444 bytes. It
  admits Goya Pairs (`apps/pairs.gscene` on `arachnid/v0.2`, GSSH/4-live,
  314 actions) on the NEON XR55N21WEB24 (webOS 22) in Developer Mode, and
  dispatches its first move to revision 1 (`first_pic 4`, `phase 1`).
  There is no Arachnid build, no IPK and no JavaScript.
* **V-Native is possible in Developer Mode.** The native-app jail exposes
  Wayland, EGL/GLES2 (PowerVR), SDL2 2.0.14 and the GPU device node.
* **Nothing is drawn yet.** The run is headless. VIU-G01 needs presentation
  on the screen, so it is **not** met.

## 2. Reconciliation: directive names against the repository

| Directive name | In this repository | Consequence for Viuda |
| --- | --- | --- |
| Goya Resolver (§5.1 VRI) | `crates/goya-resolver`, contract `GY-RESOLVER/0.1`, ADR-RESOLVER-001 with 12 amendments. Admission through the GSSH handoff, the D1 machine from `goya-executable`, indexed display invalidation, reload, suspend and resume, caret, scroll, and hit test. **Untracked** in `nebula/v0.1` (commit `e956ef28` dropped it from the manifest; the working tree adds it back). | Viuda is a **host crate over `goya-resolver`**, as ADR-RESOLVER-001 requires ("must not fork admission or the D1 machine"). This is §5.1 and the §13 duplication risk handled by an existing rule. |
| `webos` as a resolver target | ADR-RESOLVER-001 Amendment 3: "`macos`, `webos`, `tvos`, and Quitasol are not targets". | Needs **Amendment 13**: `webos` served by Viuda. Until then the probe runs headless with no `--target` (and `android_tv` for a projection). |
| VBE typed internal representation (§5.2) | The resolver forbids "a second IR, a second VM, or a bytecode format" (ADR Decision 1). D1 runs on `D1Program`. D2 is refused (`GY-RESOLVER-D2`). | §5.2's "compact typed representation" is `D1Program`. A new lowering would need an ADR change. D2 (async, effects) is not executable in any interpreter yet. |
| Arachnid (Control B) | `arachnid/v0.2`, worktree `~/goya-arachnid-v02`, commit `290c91c`. Focus slots, stock-remote input, `webos.local-storage.v1`, GVI lowering to `gvi.js`. | Arachnid and the resolver are on **different lines**: `arachnid/v0.2` forked from `684fd84`, which has no resolver. A Viuda branch needs both. |
| GVI family (§5.3) | `crates/goya-gvi`: reference decoder, GVI-D/A/O/L, packed and expanded loops (GFX-001). `emperor-runtime` feature `scene` (on `arachnid/v0.2` only) draws GSSH/4-live scenes with tiny-skia, an embedded font, the D1 executor and the GVI reference decoder. | No new graphics format is needed (§5.3 is satisfied by reuse). **The Emperor scene renderer is the obvious first V-Native presenter.** It is software raster today. |
| Caprichios (§6) | `goya-caprichios`; webOS engine table and model decoder (`webos_engine_caprichio.rs`, `webos_tv_models.rs`) on the Arachnid line. | V-C01…V-C08 belong in `goya-caprichios`, keyed by the existing model decoder. None exist yet. |
| goya-state, romántico (§5.6) | `goya-state`, `goya-romantico` exist. D8 records, D11–D13 persistence defaults (`webos.local-storage.v1`). | The resolver classifies durable state and does not write it (Decision 5). Viuda must write D8 records compatible with Arachnid's for OBJ-10. On V-Native `localStorage` does not exist, so this needs a **new store token** (for example `webos.native-file.v1`) and a D13 migration story. |
| Disparates (conversion pipeline, §4 and §7) | Named only in Arachnid documents. No code or specification. | §7 cannot start until it is specified. |
| Inquisition (§9.1) | Does not exist (`docs/arachnid/roadmap.md` §2). Nearest: `nevada::conformance`. | VIU-G03 and VIU-G08 have no validator to plug into. |
| Adios (§5.6 lifecycle) | No code or specification (`docs/graphics/README.md`). | The resolver's `Phase` (initialising, running, suspended, terminated) is the only lifecycle model there is. |
| Tauromaquia (§9.2) | Does not exist. | Optional per the directive. |

## 3. Device evidence (§3.2 pathways)

Device: the owner's NEON XR55N21WEB24, `192.168.1.182`, Developer Mode, SSH
as `prisoner`. Reproduce with `scripts/viuda-phase0-probe.sh <host> <scene>
[action]`. It deletes what it copies.

| Fact | Observed |
| --- | --- |
| Kernel / userland | Linux 5.4.96 `aarch64` kernel, **32-bit ARM userland** (`/bin/busybox` is ELFCLASS32, EM_ARM; loader `/lib/ld-linux.so.3`) |
| CPU / RAM | 4 cores; MemTotal 1,266,292 kB; MemAvailable about 176 MB during the probe |
| Load | Load average about 29–30 with the TV on (the reason the timings below are only indicative) |
| Developer storage | `/media/developer` mounted `rw,nosuid`, **not** `noexec`. A script and a binary placed in `temp/` execute. |
| Native jail | `/media/developer/jail_app.conf`, section `native_Dev Mode`. Groups `video,audio,luna,compositor`. Mounts `/usr/lib` and `/tmp` (which holds `xdg/wayland-0`). Copies `/dev/pvrsrvkm` and `/dev/fb*`. Sets `EGL_PLATFORM=wayland` and `LD_LIBRARY_PATH=/usr/lib:$APPDIR/lib`. |
| Graphics stack | `libwayland-client`, `libwayland-egl`, `libEGL`, `libGLESv2` (`libGLESv2_PVR_MESA`), SDL2 2.0.14 (+image, ttf, mixer, net), `libNDL*` (LG media) |
| Web runtime (V-Hosted, V-Accelerated) | Chromium 87 (ARACHNID-1). WebAssembly is in Chromium since 57, but **no WASM module has been run on this TV**. |

Resolver on the device (static `armv7-unknown-linux-musleabihf`, linked with
`rust-lld`, symbols stripped):

| Measurement (Pairs, `--validate-only`) | Value |
| --- | --- |
| Binary | 1,154,444 bytes, sha256 `45d167d2…b541c` |
| Scene | sha256 `c3639252…60540`, GSSH/4-live, 314 actions, IR lowered |
| Wall time, admit + resolve, 5 runs | 0.23–0.26 s |
| Peak RSS | 24,000–24,032 kB |
| `--dispatch c0_d0_first` | revision 1, `sites 0`, `layout 0` |

These figures cover one process launch, which includes parsing and admission
every time, because no cache exists (§9.3). They are not cold-start figures
for an app: there is no window, no SAM launch and no comparison. musl's
allocator is in the loop.

### Pathway verdicts

| Pathway | Verdict | Evidence / gap |
| --- | --- | --- |
| **V-Native** | **Feasible in Developer Mode**: headless execution proven; presentation surfaces present in the jail | Not shown: a SAM-launched `"type": "native"` IPK that opens a Wayland/EGL surface. Not known: whether native apps can be **distributed** outside Developer Mode. The LG store accepts third-party web apps; native is believed to be partner-only. **Unverified.** |
| V-Accelerated | Plausible | Needs a `wasm32` build of `goya-resolver` (it reads files with `std::fs` in its library) and one run in Chromium 87 on the TV |
| V-Hosted | Proven by Arachnid for *compiled* output | Arachnid's runtime is generated per app. An interpreted, generic host does not exist. |

§3.2 says the choice must be evidence-driven and must not assume native
access on retail TVs. The evidence above supports V-Native **for Developer
Mode only**. If release (VIU-G12) means store distribution, V-Accelerated is
probably the release path and V-Native the measurement ceiling. That is
decision 2 below.

## 4. Gate status

| Gate | Status |
| --- | --- |
| Phase 0 gate: viable physical-device pathway | **Partial.** Execution is proven on the device; presentation is not. |
| VIU-G01 Physical-device execution | Not met (headless only) |
| VIU-G02 Interpretation integrity | Holds for the probe: the generic binary was built without the scene. Nothing app-specific was generated. |
| VIU-G03 … VIU-G12 | Not started |

## 5. Reference application (§7)

None is selected. **Goya Pairs is not eligible.** It was written in Goya, so
there is no conventional "Control A" to compare against. It remains the
Phase 1 smoke scene. Candidates must be open-source webOS **web** apps,
because Control A must run on the same TV. Each candidate needs a licence
check before conversion (§13). Without Disparates, conversion would be done
by hand, and §7.2's integrity dimensions would have to be argued case by
case.

## 6. Proposed next steps (none started)

1. **Branch.** Create `viuda/v0.1` from a line that has both the resolver
   and `arachnid/v0.2`'s scene runtime and webOS caprichios. This
   probably means committing the resolver first.
2. **ADR-RESOLVER-001 Amendment 13.** Add `webos` as a target served by Viuda.
3. **`crates/goya-viuda`.** A host over `goya-resolver`, following the
   workspace layout rather than the twelve folders in §10. Its first
   presenter would be the Emperor tiny-skia scene renderer on `wl_shm`,
   driven by the native jail's Wayland socket. EGL comes later.
4. **Native IPK.** Package the generic Viuda binary and a scene as a
   `"type": "native"` app with Arachnid's `ipk.rs` and `deploy.rs`. Launch it
   through SAM. Show Pairs on screen with the stock remote. That is VIU-G01.
5. **WASM probe** for V-Accelerated (decision 2).

## 7. Decisions for the owner

1. **Branch:** is `viuda/v0.1` the right line, and should the resolver be
   committed first?
2. **Distribution target:** is Developer Mode enough for VIU-G12, or must
   Viuda ship through the LG store? This decides between V-Native and
   V-Accelerated as the release pathway.
3. **D2:** the resolver refuses D2. Is D2 execution in scope for Viuda
   (§5.2 asks for asynchronous operations), and does that reopen ADR
   Decision 4?
4. **Disparates, Inquisition and Adios** have no code or specifications
   here. Where are their specifications?
5. **Reference application:** pick one, or approve a shortlist to
   licence-check.
