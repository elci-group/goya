<p align="center"><img src="../../assets/arachnid-logo.png" alt="Arachnid — webOS app compiler" width="180"></p>

# ARACHNID-2 — Arachnid on current Goya: GSSH/4-live, focus slots, GVI lowering

Status: **Candidate**, 25 November 2026, branch `arachnid/v0.2` (from
`nebula/v0.1` at `684fd84`). It builds on [ARACHNID-1](ARACHNID-1.md).

The owner asked for a picture-matching game compiled by Arachnid and
installed on the webOS TV, to demonstrate recent work. They decided:
* move Arachnid onto current Goya, on a new branch `arachnid/v0.2`;
* express the game rules as precomputed transitions, not a language change;
* add a GVI lowering.

The demo is [`apps/pairs.gscene`](../../apps/pairs.gscene) ("Goya Pairs").

## 1. The port

`arachnid/v0.1` had forked from `nebula/v0.1` on 18 November. By 25 November
`nebula` was 793 commits ahead, so the two lines no longer shared each
other's work.

The port moves the Arachnid crate, the webOS engine and model caprichios,
the scripts and the docs. It also moves Arachnid's language proposal into
`nebula`'s `goya-lang`. Before the port, `nebula` carried only `text-from`.
It now also has:
* `host` state;
* `@link` (roles, slots, `field`, `act`);
* `present-when` (`is`, `at-least`);
* `image-from` and `remote-digit`;
* the bounded motion properties.

These are admitted by the live profiles only, with Arachnid's checks and
tests (11 ported). Approved profiles do not change (D11).

Arachnid's draft made `@persistence` mandatory in GSSH/4-live; D11 had
already withdrawn that, so `nebula`'s optional semantics were kept.

Other targets fail closed. `goya_lang::extended_live_construct` names the
first such construct: `host` state, `@link`, a live property other than
`text-from`, `@graphic` or `@draw`. Maja and Sash refuse it at its range
instead of dropping it.

## 2. GSSH/4-live lowering

| Construct | webOS lowering |
| --- | --- |
| `@iden` (D14) | `aria-label`; `hint` → `aria-describedby` on visually hidden text; a labelled `cell` gets `role="group"` |
| `durable` state (D8, D11, D12) | The webOS default local store `webos.local-storage.v1` (the app's `localStorage`) holds the D8 record `goya.state.v1/<ontology>`. Restore runs before the first render, all-or-nothing. A record that fails to restore is reported and never overwritten (D4). A write first re-reads the record (D8 conflict check), then saves the staged state, and only then shows it (D12). Without `@persistence`, the build warns that the default was used (D11). Naming `webos.indexeddb.v1` is refused (`GY-AR-651`) until it is built. |
| `text-from` announcements (D9) | One polite live region. Each committed transition announces exactly `goya_lang::display_binding`'s wording, in depth-first order. Contexts are computed from the authored text at build time. |

`webos` is a persistence platform in the shared contract, which closes D11
open item (d): its default store is `webos.local-storage.v1` and its
database `webos.indexeddb.v1`.

## 3. Focus slots and literal domains

Precomputed transitions stack several buttons on one rectangle, one per
situation. Arachnid treats such a rectangle as a **focus slot**:
* **Domains.** A session key that interactions only assign literals can
  hold only its initial value and those literals (`live::literal_domain`),
  so its presence representatives are restricted to them.
* **Per-slot check.** For every slot, over the combinations of the keys its
  own buttons use, exactly one button is present (`GY-AR-643` otherwise).
* **One graph.** When every slot is always occupied, the set of focusable
  controls never changes, so there is one focus graph over the slot
  rectangles, with no global product of keys to enumerate.
* **Runtime.** It focuses slots and acts on the present button. The focus
  ring follows the present button when the variant changes.

When any slot can be empty, each button is its own slot and the earlier
per-configuration enumeration runs unchanged. Tapestry and the fixtures
build exactly as before.

## 4. GVI lowering (Phase 5 slice)

`@graphic` declares a drawing and its known, finite state model in
`.gscene`; `@draw` places it in a cell. The grammar is in
`goya_lang::graphic`; both are proposed constructs of the live profiles.
* **Shapes.** `path` (absolute SVG subset), `rect` (with optional radius)
  and `ellipse`, with `fill`, `stroke` and `stroke-width` in the graphic's
  own points on its `canvas`.
* **Effects.** `hide`, `scale` about the centre, `move`, `fade`, `paint`
  and `morph`.
* **`@draw` rules.** The first matching `dim: "value" when <condition>`
  wins, and each dimension ends with an unconditional rule.
* **`step-s`.** A dimension may step through its intermediate values when
  its chosen value changes.

At build time (`graphics.rs`), each graphic is lowered to a GY-GIR/0
drawing and a `Product` state model:
* **Compile.** It is compiled twice, with transforms kept and baked, each
  with GVI-D (depth 8), GVI-A (provenance) and GVI-O. The smaller file is
  kept.
* **Verify.** The kept file is opened with the reference decoder
  (`Inventory::open`) and checked with `verify_against` before packaging.

The IPK carries `graphics/<name>.gvi` and the generic runtime `gvi.js`;
`app.js` embeds each inventory and its SHA-256. On the TV, `gvi.js`:
1. checks the digest;
2. decodes the container, `INDX`, `PROJ`, `GEOM`/`DIFF` and `STYL`/`LSTY`;
3. paints projections with canvas `Path2D`, using the exact integer
   arithmetic (`BigInt`, rounding half to even).

The minimum engine is Chromium 67.

**Conformance.** Under Node, `gvi.js` reproduces the reference decoder
state for state: items, path segments, styles, transforms and opacity. This
covers both encodings of the Pairs card and a test badge, plus inventories
built through the API, which together cover:
* identity, bitmap and packed index keys;
* `STYL` and `LSTY`;
* record and columnar `DIFF`, including GVI-A trajectories;
* frame-of-reference, dictionary and operative columns;
* literal and operative `STAT`.

## 5. Goya Pairs

Measured at build:

| | |
| --- | --- |
| Scene | 624 nodes, 410 buttons in 17 focus slots, 314 interactions |
| Card graphic | 3 dimensions: picture (8) × flip frames (7) × matched burst (4) = 224 states, 112 projections |
| Inventory | 4,416 bytes with transforms kept, 5,120 baked (kept is chosen) |
| Durable state | `games` in `webos.local-storage.v1`, named explicitly |
| Target | `reported:7.5.8` (webOS 22, Chromium 87) |

Every card's art is the one GVI graphic. The flip is a scale-X sequence and
the match burst a morph, both stepped by the painter.

The rules form a decision list:
* **Card slot.** Each card is one slot of 13 buttons: one to continue after
  a miss, and per deal one matched, one first-pick, one held, and eight
  second-pick buttons keyed on the first card's picture.
* **Labels.** Each button's `@iden` describes its own situation ("Card 6:
  moon, turned over").
* **New game control.** One slot of eight "New game" buttons (keyed on the
  first unmatched pair) and two "Play again" buttons, one per deal.
  "Play again" counts the win and switches the deal.

**Authoring.** `apps/pairs.gscene` is the whole app. Its repetitive
transition table was typed out with a throwaway script; the script is not
shipped and the `.gscene` is the source.

**Qualification** (`scripts/check-arachnid.sh`):
* Two builds are byte-identical, and the inventory is packaged.
* Headless Chrome plays a game with the stock remote's arrows and OK (47
  checks): first pick, a miss, turning back, all eight pairs, the win
  panel, Play again, and the second deal. It checks state, focus variants,
  exact D9 announcements, canvas pixels after each animation, and the D8
  record written to the store.
* A relaunch on the same profile checks that `games` is restored and shown
  (4 checks).

## 6. Device run (25 November 2026)

The owner's TV is the NEON XR55N21WEB24 (webOS 22, Chromium 87) at
`192.168.1.182`. Its Developer Mode key was fetched again and is unchanged
since 20 November.

`arachnid deploy … --launch yes` installed and launched
`com.goya.arachnid.pairs` 0.1.0. Over SSH, the installed files are
present, and `graphics/card.gvi` has the build's SHA-256 (`c74eba3b…e80e03`).

What the TV showed, and play with the stock remote, depend on the owner's
report. Nothing on screen is claimed here until then.

## 7. Diagnostics added

| Code | Meaning |
| --- | --- |
| `GY-AR-643` | Two stacked buttons can be present together, or a stacked slot can be empty |
| `GY-AR-645` | A `@graphic` failed GVI compilation or verification |
| `GY-AR-651` | A durable store Arachnid does not implement |

## 8. Not done

* **Device run** (§6).
* **Durable store.** Only `webos.local-storage.v1`: there is no IndexedDB
  store, so there is nothing to migrate between (D13).
* **Slot mode** needs every slot always occupied; mixed scenes use the
  per-configuration path.
* **Graphics.** No `Transitions` reachability or exclusions are authored in
  `@graphic` (the decoder handles them, through the API). No colour fills
  with alpha, even-odd fills or join styles beyond round.
* **The `.gscene` grammar for graphics is a proposal**, like the rest of
  the live profiles, and needs the owner's review (README decision 8).
