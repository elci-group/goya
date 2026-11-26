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

* `scripts/check-viuda.sh`: tests, clippy, the ARMv7 build and its hash.
* `scripts/check-viuda.sh <tv> [scene]`: also a headless key script on
  the TV, then install + launch as `com.goya.viuda.check`, and its log
  after 35 s.
* `viuda <scene> --headless --keys ok,right,…` runs anywhere, with no
  compositor.

## 7. Not done, in order of cost to the directive

1. **Rendering.** Dirty regions are done (§8). Ambient frames of a scene
   whose whole background breathes stay full repaints, and everything is
   still drawn on the CPU. GLES2 through `wl_egl` is next.
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
