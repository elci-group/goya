# VIUDA-1 — Phase 1: an interpreted `.gscene` app on the TV

Status: **Phase 1 slice, candidate.** Date: 25 November 2026. Branch
`viuda/v0.1` (worktree `~/goya-viuda`), based on `arachnid/v0.2` at
`290c91c`, with `goya-resolver` taken from `nebula/v0.1` at `b408ee19`.

Owner decisions of 25 November 2026: work goes on `viuda/v0.1`, and
Developer Mode is enough. That makes **V-Native** the pathway (VIUDA-0 §3).

## 1. Result

Goya Pairs (`apps/pairs.gscene`) runs on the owner's NEON XR55N21WEB24
(webOS 22) as a native app, interpreted by Viuda. There is no Arachnid
build: the package holds the generic runtime, the unchanged source, an
`appinfo.json` and an icon. On the first launch, someone played it with the
stock remote: 21 keys and 10 published actions, with D9 announcements in
the log (§4).

The app's own log shows that the surface was exposed and fullscreen,
frames were committed, and keys reached it. The owner looked at the TV
on 25 November 2026 and confirmed that **the cards look right**.

## 2. Architecture

```
scene.gscene ──► goya-resolver (GY-RESOLVER/0.1)   semantics: admit, D1, revision
             └─► goya-scene-runtime (published)    presentation: layout, focus,
                   └─ goya_scene_graphics::compile    GVI, motion, paint (tiny-skia)
goya-viuda::webos ── wl_shell + wl_webos_shell, wl_shm XBGR8888, remote keys
```

* **One semantic owner.** Every OK and every bound digit becomes
  `Instance::activate("button.<name>.activate")`. The scene then shows
  `Instance::state` through `Scene::show_published`, and executes nothing
  itself. Tests check that both sides hold identical values after every key
  (`a_sequence_publishes_once_per_action_and_stays_in_agreement`).
  ADR-RESOLVER-001 Amendment 13 records this.
* **Shared renderer.** Emperor's scene runtime moved, history kept, into
  `crates/goya-scene-runtime`. It is re-exported as
  `emperor_runtime::scene`, and AccessKit/AT-SPI is its `a11y` feature.
  Two methods were added: `Scene::from_parsed`, which avoids a second
  admission inside the renderer, and `Scene::show_published`, which is
  `activate`'s commit path without execution. `ambient_interval` is
  described in §5. Emperor's behaviour is unchanged:
  `scripts/check-emperor-scene.py` passes 15/15 on an App built on this
  branch.
* **Graphics at load.** `@graphic`s are compiled with the function
  Arachnid and Emperor use at build time. The inventory hashes the same on
  the TV as Arachnid's packaged `card.gvi`: `c74eba3b…26e80e03`.
* **The platform font.** The runtime uses
  `/usr/share/fonts/LG_Smart_UI-Regular.ttf`, so apps ship no font.
* **The window.** The Wayland client is pure Rust (`wayland-client` 0.31,
  no libwayland). The binary is a static `armv7-unknown-linux-musleabihf`
  build of 2,486,204 bytes, and rebuilds are byte-identical
  (`ee4624c6…dff3cc`). `webos-shell.xml` is LG's (Apache-2.0), vendored
  in `crates/goya-viuda/protocol/`.
* **Packaging.** `viuda-pack` uses Arachnid's IPK writer. A new
  `package_with_executables` gives `viuda` mode 0755; web IPKs are
  unchanged. Before packaging, it loads the scene with the same
  `Viuda::load` the TV runs, so a scene the runtime would refuse is never
  packaged.

## 3. Platform facts learned on the TV

| Fact | Observed |
| --- | --- |
| Compositor globals | `wl_compositor` v3, `wl_shell`, `wl_webos_shell`, `wl_shm`, 4 × `wl_seat`, `wl_output` v2, `wl_drm`, `zwp_linux_dmabuf_v1` v3, `qt_hardware_integration`, webOS input/foreign/group extensions. **No `xdg_wm_base`.** |
| `wl_shm` formats | includes `XBGR8888`/`ABGR8888` (tiny-skia's byte order) |
| Output | 1920×1080 |
| Remote, as evdev codes | OK 28, Up 103, Down 108, Left 105, Right 106 |
| Native launch | argv[1] is `{"@system_native_app":true,"nid":"<id>"}`. `HOME` is the app directory. `XDG_RUNTIME_DIR=/tmp/xdg` and `WAYLAND_DISPLAY` is unset (Viuda defaults to `wayland-0`). |
| App's writable, SSH-readable place | the TV's `/tmp`. The jail user cannot create files in `/media/developer/temp`. |
| Developer Mode limits | `prisoner` cannot see the app's process (hidepid), read the journal, or call `closeByAppId`. The app therefore reports its own CPU and memory (`status` lines). |
| Launch quirk | `luna-send-pub -n 1 …/launch` in the foreground of an SSH session, straight after install, started nothing (3 of 3 tries). Sent from a background job, it started the app every time. `viuda-pack --launch` does the latter. |
| Relaunch | Launching an app that is already running only raises it: a reinstalled binary does not run until the old process ends. `…/dev/remove` ends it, with no Wayland `close`. |

## 4. Measurements (indicative, not VIU-G06)

Single runs on one TV with load average about 29–30. Pairs has no
conventional original, so nothing here is compared with Control A.

| Metric | Value |
| --- | --- |
| Load: resolver admit + resolve + start | 231–287 ms |
| Load: presentation admit + scene | 658–749 ms, of which `@graphic` compile 379–430 ms |
| First frame committed, from process start | 1,036–1,037 ms (3 launches) |
| D1 dispatch + presentation update | median 890 µs, max 1,247 µs (n=10, played by hand) |
| Full-frame software render, 1920×1080 | median 59.0 ms, p95 75.2 ms, max 162.5 ms (n=200) |
| Idle CPU, Pairs on screen and untouched | about 47 ms per second (about 4.7 % of one core) |
| Memory, steady | RSS 28.0 MB, peak 36.2 MB |
| Package | 2,800,066 bytes: runtime 2,486,204, scene 283,824 |

**Idle CPU fix.** At first the host repainted ambient motion every 100 ms.
Pairs' background breathes (`fill-to` over 30 s), so the full-frame render
took about 60 % of a core while nobody touched the remote.
`Scene::ambient_interval` now computes the longest interval that still
paints every 8-bit level the fill passes through:
`2·period / (π·Δ)`, with the largest channel change Δ = 14, which gives
1.364 s. That is no visual loss by construction. Drift asks for 30 frames
a second.

## 5. Gate status

| Gate | Status |
| --- | --- |
| Phase 0 gate (pathway) | **Met for Developer Mode** (V-Native) |
| Phase 1 gate: a valid `.gscene` runs without Arachnid compilation | **Met** (§1; the owner confirmed the picture) |
| VIU-G01 Physical-device execution | As the Phase 1 gate |
| VIU-G02 Interpretation integrity | Holds: one runtime binary for every app; the package carries the source unchanged |
| VIU-G03 Semantic correctness | Partly: D1 runs only in `goya-resolver`, and tests assert agreement after every key. D2 is still refused. |
| VIU-G09 Lifecycle | Partly: minimise suspends, restore resumes, and `close` terminates (code paths; the events have not been seen on the TV yet). A kill by reinstall or remove gets no summary. |
| Others | Not started |

## 6. Reproduce

* `scripts/check-viuda.sh`: tests and clippy (software and `gpu`), and
  both ARMv7 runtimes with their hashes (§9).
* `scripts/check-viuda.sh <tv> [scene]`: also both runtimes headless on
  the TV, then the GPU runtime installed and launched as
  `com.goya.viuda.check`, and its log after 35 s.
* `viuda <scene> --headless --keys ok,right,…` runs anywhere, with no
  compositor. `--gpu` adds the GPU compositor offscreen and compares every
  frame with the software composition.

## 7. Not done, in order of cost to the directive

1. **Rendering.** Dirty regions are done (§8), and the GPU compositor runs
   on the TV (§9). Input-to-frame latency on the TV is not measured yet.
2. **Load time.** Graphics compile both encodings, as Arachnid does, to
   choose the smaller. A §9.3 cache keyed on source digest, resolver
   version and Viuda version would remove most of the 1 s.
3. **Durable state** is not persisted (ADR-RESOLVER-001 Decision 5). V-Native
   needs a store token (for example `webos.native-file.v1`) and a D13
   migration story beside Arachnid's `webos.local-storage.v1`.
4. **Termination.** There is no SIGTERM handling, so a platform kill loses
   the summary and would lose a durable write.
5. **Accessibility.** D9 announcements are computed and logged, not
   spoken. The webOS TTS service has not been tried.
6. **Back / Home / Exit** are left to the platform (default key mask).
   Holding an arrow does not repeat.
7. **Comparison.** There is no Control A; decision 5 (reference app) is
   open. Viuda vs Arachnid on Pairs is possible now, but the web app's
   process is invisible to `prisoner`, so Arachnid's CPU and memory need
   another channel.

## 8. Dirty-region rendering (25 November 2026, after the first commit)

The directive's rendering policy (§5.3) asks for retained structures and
dirty-region invalidation. `goya-scene-runtime` now has
`Scene::render_into(target, advance_ambient)`:
* **Retained frame.** The caller keeps the previous frame. For every node
  the scene records what decides its pixels: geometry after motion,
  opacity, fill, the graphic's shown state, text and focus. It also
  records the pixels the node can reach, including the focus ring and
  graphic overflow.
* **Damage.** The old and new bounds of every node whose record changed
  are merged into disjoint rectangles. Above 60 % of the frame, the whole
  frame is repainted.
* **Exactness.** Rect fills are intersected with each rectangle and text
  is clipped to it. Paths (graphics, strokes, the focus ring) are never
  clipped: the region grows until every path it reaches lies wholly
  inside it. A clip mask was tried first and rejected, because it rounds
  translucent fills by one level. Tests play a whole game, card flips
  included, and compare every partial frame with a full paint: **0
  differing bytes**.
* **Bare hit areas.** A node that paints nothing, such as Pairs' stacked
  buttons, produces no damage when its presence changes. This took the OK
  frames from about 600k damaged pixels to about 100k.
* **One ambient clock.** Ambient motion moves only on the frames the host
  asks for (`advance_ambient`). A partial repaint therefore never mixes two
  phases of a breathing fill.
* **Host.** The webOS host copies only the damaged rows into each
  `wl_shm` buffer, tracking each buffer's backlog, and sends the same
  rectangles as `wl_surface.damage`.

Emperor's `render()` goes through the same paint path. Its Pairs
qualification still passes 15/15, and its mid-game PNG is byte-identical
to the one rendered before the change.

Measured on the TV, headless (`viuda --headless`), 3 runs of the same
13-key script, load average about 30:

| | median | p95 | max |
| --- | --- | --- | --- |
| Full paint after each key (old path, n=13 per run) | 66–70 ms | 75–103 ms | 77–118 ms |
| Partial repaint, every frame including animation steps (n=62–63) | 5.2–5.5 ms | 9.7–10.7 ms | 16.9–17.5 ms |

A focus move or a flip step damages one or two card-sized rectangles
(60–100k pixels). The live app (0.1.3) runs at about 3 % of one core when
idle, down from 4.7 %. RSS is 36 MB, up from 28 MB: that is the retained
1080p frame (8.3 MB), the cost of partial repaints. Interactive latency
on the live app has not been measured yet, because no keys were pressed
during that run.

## 9. GPU compositing (26 November 2026)

The directive's rendering policy prefers the GPU where the platform has
one. The TV has PowerVR GLES2 behind `libEGL`, `libGLESv2` and
`libwayland-egl` (VIUDA-0 §2).

**Display list.** `Scene::display_list` describes the frame that `render()`
paints, in the same order, as two kinds of step:
* `Rect`: a rect fill, with its exact edges and colour. The fragment
  shader computes how much of each pixel lies inside, so fractional edges
  are antialiased without a raster. A breathing background or a fading
  card changes one uniform.
* `Sprite`: a graphic at its shown dimension values, a stroke, text, or
  the focus ring. `Scene::sprite` rasterizes it with the CPU code
  `render()` uses, at opacity 1, cut to the frame. The key holds
  everything that decides the pixels (node, kind, the box after motion,
  the graphic state or the text), so the compositor keeps each texture
  while its key is drawn. It drops idle textures, oldest first, above
  48 MB.

`Scene::compose` draws a display list in software. With a frozen clock it
differs from `render()` by at most 2 per channel, except on 2 pixels of
a 960×540 frame (at most 12): sprite opacity is applied to the raster as a
whole, not to each path.

**Compositor** (`goya-viuda`, feature `gpu`, module `gles`). It is OpenGL
ES 2.0 with two programs, one unit quad and nearest-texel sprites in
premultiplied alpha. `libEGL` and `libGLESv2` are opened with `dlopen`,
so a runtime without them falls back to software. The webOS host creates
a `wl_egl_window` on its surface and an EGL window surface with swap
interval 1. If EGL fails to start, or a swap fails, it presents through
`wl_shm` as before. EGL needs C `wl_display` and `wl_surface` pointers,
so `gpu` switches the Wayland client to libwayland, also loaded with
`dlopen`.

**Off the TV** (Mesa Intel Graphics RPL-U, Mesa 25.2.8, surfaceless EGL, release build):
* Every GPU frame of a Pairs game, read back, is within 2 per channel of
  the software composition of its display list: 0 pixels over, worst 1.
  The test asserts this when `VIUDA_REQUIRE_GPU` is set, which
  `check-viuda.sh` does.
* At 1080p the first frame uploads 26 sprites (12.9 MB resident), with
  3.0 ms of rasterizing. After that, a focus move or a flip step uploads
  1–4 sprites. Submit takes 0.5 ms median and `glFinish` 1.5 ms.

**The TV binary.** The TV's userland uses the soft-float calling
convention (`/lib/ld-linux.so.3`, Debian's *armel*), so the static
hard-float runtime cannot load its graphics libraries. The GPU runtime is
`armv7-unknown-linux-gnueabi`, built with
`-C target-cpu=cortex-a53 -C target-feature=-soft-float`. That is
*softfp*: float arguments pass in core registers, as the TV's libraries
expect, while arithmetic uses VFP. The generated assembly confirms both.
rustc warns that toggling `soft-float` is unstable.

It links with clang and `rust-lld` against a glibc 2.24 sysroot
(`scripts/viuda-armel-sysroot.sh`). The sysroot is built from four
Debian stretch armel packages, pinned by the SHA-256 listed in Debian's stretch
Packages index. The binary is 2,552,808 bytes and needs only glibc symbols up to
`GLIBC_2.18`, plus `libgcc_s.so.1`.

The static software runtime (2,504,292 bytes) stays as it was.

**On the TV** (`scripts/check-viuda.sh 192.168.1.182`, load average about
30; glibc 2.31):
* **Driver**: `PowerVR Rogue BXE-4-32`, OpenGL ES 3.2 build
  1.15@5986747, EGL 1.5 (Mesa front end).
* **Headless** (`--headless --gpu`, offscreen, 13-key script, 46 frames):
  every frame is within 2 per channel of the software composition (0
  pixels over). Submit takes 1.2 ms median (p95 3.1 ms) and `glFinish`
  5.4 ms median, against 5.5 ms for a software partial repaint and 67 ms
  for a full one (§8). The first frame rasterizes 26 sprites in 57 ms.
  Offscreen EGL works over SSH.
* **Live**: PowerVR's EGL accepts a `wl_egl_window` on the `wl_shell`
  surface. The app came up on the GPU path (`first-frame
  committed_ms=1246 gpu=1`), and someone played it by hand: 97 keys, of
  which the first 58 gave 21 published actions.

| Live, 700 frames | median | p95 | max |
| --- | --- | --- | --- |
| Submit (GL calls, sprites already resident) | 2.9 ms | 5.4 ms | 76 ms |
| `eglSwapBuffers` | 0.37 ms | 2.3 ms | 53 ms |
| Sprite rasterizing, frames that upload | 0.59 ms | 7.2 ms | 105 ms |

* **CPU**: 9–10 % of one core while being played (10 s windows; flips
  upload about one sprite per frame). Idle, 30 ms over 20 s: about **0.15 %**, against
  3 % for the software path with dirty regions and 4.7 % for full repaints.
  An ambient frame only changes a fill colour uniform.
* **Memory**: RSS 17.2 MB (peak 19.3 MB), against 36 MB for the software
  path, which keeps an 8.3 MB frame plus two `wl_shm` buffers. Sprite
  textures stay near the 48 MB budget during a game (50 MB resident).
* **Package**: Goya Pairs 0.2.1 (`com.goya.viuda.pairs`) is installed with
  the GPU runtime.

**Not measured yet: input-to-frame latency.** The host summarized it
only at exit, and a platform kill (reinstall, `dev/remove`) gives no
exit. It is now also written with each 10 s status line.
