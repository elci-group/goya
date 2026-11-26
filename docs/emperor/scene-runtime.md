# Emperor scene apps — `emperor.linux.scene/0.0.1`

Status: **Candidate**, 25 November 2026, branch `arachnid/v0.2`. This builds
general GSSH/4 and GSSH/4-live scenes for Linux beside Emperor's one-counter
path. The counter path, its NIR and its evidence are unchanged. The Nebula
COSMIC profile selects it with `emperor build --desktop cosmic`, like any
other source.

## Build (`crates/emperor/src/scene_build.rs`)

`emperor build SCENE --out DIR [--release] [--desktop cosmic]` first tries
the counter lowering. A source that is not the counter subset but is
admitted by `handoff::admit_v4` or `admit_v4_live` is built as a scene app:

* **Refused at their range:** `host` state, `@link`, `image-from` and D2
  interactions. These need Linux platform services that don't exist yet.
* **Interactions:** compiled by the canonical D1 compiler.
* **`@graphic`:** compiled and verified by `goya_scene_graphics`, the same
  path Arachnid and Aragon use, and embedded.
* **Durable state:** only `linux.xdg-state.v1` (D11). The build warns when
  the scene relies on the default; any other adapter is refused.
* **Font:** DejaVu Sans is read once at build time and embedded, with its
  SHA-256 in the report.
* **Link:** `emperor-runtime`'s `emperor-scene-app` binary (feature `scene`)
  is linked offline into `DIR/App`.

## Runtime (`crates/goya-scene-runtime`, re-exported as `emperor_runtime::scene`)

The runtime moved to its own crate on 25 November 2026 so that Viuda
(`docs/viuda/VIUDA-1.md`) can link it without `goya-wayland` and AT-SPI.
AccessKit is its `a11y` feature, which Emperor's `scene` feature turns on.
Generated apps still name `emperor_runtime::scene::…`.
`Scene::render_into` adds retained, dirty-region repainting that is pixel-exact against `render()` (see `docs/viuda/VIUDA-1.md` §8). `Scene::display_list`, `Scene::sprite` and `Scene::compose` describe the same frame for a compositing (GPU) renderer (§9 there). Emperor still calls `render()`.


* **Admission and layout.** The embedded source is admitted again at start
  and laid out with `goya_lang::layout` at the window size, one point per
  surface pixel. The frame is rendered on the CPU with tiny-skia and
  presented through `goya-wayland`'s `present_rgba`.
* **Interactions.** They run on `goya_lang::CompiledInteraction::execute`,
  the canonical D1 executor, over each action's exact read and write
  snapshot.
* **Live constructs.**
  * `text-from` shows the authored text and the value, wrapping live text
    at the element's edge.
  * `present-when` (`is` and `at-least`) is conjoined with the element's
    ancestors.
  * `remote-digit` maps to the digit keys. `remote-colour`, `colour-group`,
    `sky` and `@apps` are refused at their range.
  * `fill-to` breathes the background with ease in and out. `drift-p` and
    `appear-s`/`appear-from-p` are offsets and opacity inherited down the
    subtree. All motion stops with `GOYA_REDUCE_MOTION`.
* **Graphics.** Each graphic is opened with the reference decoder
  (`goya_gvi::Inventory::open`, binding checked). The runtime paints the
  chosen projection's items, and a dimension with `step-s` steps through
  its intermediate values.
* **Input.** The pointer activates the topmost present button. The arrow
  keys move geometrically between present buttons; Enter or Space
  activates. When a stacked variant swaps, focus stays on the same
  rectangle.
* **Durable state.** This is decision D8 with D4 and D12. The App's record
  is `$XDG_STATE_HOME/goya/<ontology>.json`, the only file it writes
  (Nebula decision 6, amended).
  * Restore happens before the first frame, all-or-nothing.
  * A record that fails to restore is reported and never overwritten.
  * A transition that writes durable state is saved by atomic rename before
    it is shown. If another writer changed the file, the transition fails.
* **Accessibility.** AccessKit publishes an AT-SPI tree:
  * buttons, with their `@iden` label as name and hint as description;
  * text elements as labels whose value is the shown text;
  * a polite live region carrying each transition's D9 announcement, in
    `goya_lang::display_binding`'s wording.

  AccessKit is used through `accesskit_unix` 0.12.3 with its Tokio backend,
  the one that resolves offline.
* **Modes.**
  * Default or `--live`: the Wayland window.
  * `--render PNG [--keys K] [--size WxH]`: one frame rendered headless.
  * `--exercise K`: a headless key script, printing the JSON state, the
    last announcement and any errors.
  * `--tree`: the accessibility nodes.

## Qualification

`scripts/check-emperor-scene.py APP apps/pairs.gscene WORKDIR` plays Goya
Pairs headless: a first pick, a miss, all eight pairs, then "Play again". It
checks the state, the exact D9 strings, the record file (and that it is the
only file written), restore in a fresh process, and D4 refusal of a retyped
record. It also checks the accessible names and values, and renders a
mid-game frame. On 25 November 2026 it passed 15 of 15 checks.

The live window was opened on the owner's COSMIC session (Pop!_OS 24.04,
`WAYLAND_DISPLAY=wayland-1`). Orca speaking the announcements has not been
observed.

## Not done

* Host state, `@link` (`goya.lanlink/0`), `image-from` and D2 interactions.
* `@input` and text fields.
* Scroll viewports.
* Native input-method support (preedit) for scenes.
* The four-compositor matrix (GY-EMP-001 §4.3) for scene apps. Only COSMIC
  was run.
