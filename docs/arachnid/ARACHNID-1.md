<p align="center"><img src="../../assets/arachnid-logo.png" alt="Arachnid — webOS app compiler" width="180"></p>

# ARACHNID-1 — First webOS increment: engine caprichios, ontology limit, focus, packaging, deploy

Status: **Candidate**, 20 November 2026, branch `arachnid/v0.1`.
Continued by [ARACHNID-2](ARACHNID-2.md) (branch `arachnid/v0.2`, on
current Goya), which adds GSSH/4-live lowering, focus slots, durable state,
D9 announcements and a GVI lowering.
Implementation:
* `crates/arachnid` (`goya-arachnid`, contract `arachnid.webos/0.0.1`,
  binary `arachnid`);
* `crates/goya-caprichios/src/webos_engine_caprichio.rs` and
  `webos_tv_models.rs`.

Qualification: `scripts/check-arachnid.sh`. 59 Arachnid unit tests and 14
caprichio tests pass, both clippy-clean. First device run on 20 November
2026: see [Device run](#device-run-20-november-2026).

## Decisions taken by the owner (20 November 2026)

| Question (roadmap §6) | Decision |
| --- | --- |
| Shared focus crate or copy? | **Copy.** `src/focus.rs` is Nevada's `focus.rs` (commit `db00f6a`) with only the header and contract id changed (`arachnid.focus/0.0.1`). Extensions live in other modules. |
| Lowest supported webOS | **4.x onwards.** The floor is webOS TV 4.0 (2018, Chromium 53). |
| Logical canvas | **1920×1080 is the `@ontology` limit** for webOS builds. |
| Device | The owner has an LG TV that can be switched to Developer Mode. |
| Missing pieces (Inquisition, Tauromaquia, ares-cli, …) | **Build from the ground up** where not found. This increment builds the validator, packager and deployer it needs. |
| Branch | `arachnid/v0.1`, worktree at the repository's `arachnid/` directory, from `main`. |

## What it does

**Engine caprichios** (`goya.platform.webos.engine.v1` plus one
`goya.platform.webos.<series>.v1` per series):
* A quick lookup table covering webOS TV 4.0 to 26. For each release it
  gives the Chromium version, the user-agent `Chrome/` token, the
  reported-version range, Back behaviour and gamepad passthrough.
* 140 catalogued series from 2018 to 2026, each with its shipped
  release and upgrade ceiling.
* A model-number decoder that covers OLED, prefix-style UHD/NanoCell/FHD,
  suffix-style NANO/QNED/NU, LX and StanbyME.
* Lookups by platform name, reported version and user agent.
* 13 web-feature gates keyed to the Chromium version that first shipped
  each feature.
* A canonical text IR, hashed into every build manifest.

Every lookup fails closed: below the floor, unknown year, unrecognised
input, and the ambiguous `OLED…6…` (2016 or 2026). Human-readable table:
[webos-engines.md](webos-engines.md).

**Ontology** (`arachnid.ontology/0.0.1`):
* Admission is strict GSSH/3, so geometry is proportional only.
* Layout is computed once at exactly 1920×1080 points. One point is one
  CSS pixel at the app's fixed resolution.
* Every node must lie inside the limit (`GY-AR-620`).
* Buttons are actionable and essential. The build refuses:
  * a button with zero area (`630`);
  * overlapping buttons, since a click would be ambiguous (`631`);
  * more than 512 actionable elements (`632`);
  * a button the arrow keys cannot reach (`640`);
  * a focus trap (`641`).
* `traps.rs` adds a check that the reachability test alone misses: from
  every element reachable from the initial focus, every reachable
  essential element must still be reachable.

**Refusals instead of silent loss.** These constructs are refused with
their own codes:
* `@interaction` (`610`);
* `@state` and bindings (`611`);
* `@input` (`612`);
* `@intent` (`613`);
* gesture policies (`614`);
* `scroll` (`615`).

**Emission (Web + Tarsus):**
* `index.html` holds absolutely positioned nodes in draw order.
* `app.css` provides a double-ring focus indicator (black, then white).
* `app.js` is ES5 with the compile-time focus graph built in:
  * arrow keys 37–40 move focus, and OK (13) activates;
  * Magic Remote hover moves focus, and click activates;
  * pointer mode is entered **only** when the platform reports a visible
    cursor (`cursorStateChange`) and left on any arrow key. Mouse events
    never enter it, so a stray event cannot make OK wait for a click a
    stock remote will never send;
  * each press activates exactly once, and a press cut short by blur,
    hiding or `webOSRelaunch` does not swallow the next OK;
  * every key outside 5-way (Back 461, colour keys, digits, media keys) is
    left to the platform.
* `REQUIRED_FEATURES` is empty, so one build runs on every platform from
  4.0 to 26. The gate (`660`) is in place for when that changes.

**Stock remote** (`remote.rs`, `arachnid.remote/0.0.1`). 5-way is the
baseline and the pointer is optional:
* `KEYS` is the key contract. Each key is marked as navigate, activate or
  platform, and as observed on the TV or documented by LG.
* `press_counts` measures, over the compiled graph, the fewest arrow
  presses from the initial focus to each element and back.
  `navigation-graph.json` records `presses`, `presses_home`,
  `max_presses` and `max_presses_home`; for the fixture these are 4 and 4.
  The counts are reported, not judged, because there is no sourced
  threshold.
* `compatibility.json` gains an `input` block with the key lists.

**Packaging (Spinneret)**, std-only and written from scratch:
* CRC-32, Adler-32, stored-block gzip/zlib, ustar, System V `ar` and an
  80×80 PNG icon.
* `appinfo.json` is checked against LG's rules (`670`/`671`).
* The IPK layout matches `ares-package`, with deliberate differences for
  reproducibility (mtime 0, root ownership, stored DEFLATE, deterministic
  `Installed-Size`); see `ipk.rs`.
* Builds are byte-reproducible. Output goes to a new directory only.
* The build also writes `reports/compatibility.json`,
  `navigation-graph.json` and `compilation-manifest.json`.

**Deploy** (`arachnid.deploy/0.0.1`), with no Node.js and no ares:
* `arachnid getkey` fetches the encrypted Developer Mode key from
  `:9991/webos_rsa` and saves it with mode 0600.
* `arachnid deploy` runs the `@webos-tools/cli` protocol over the system
  `ssh` (`prisoner@tv:9922`, one multiplexed connection). It stages the
  IPK in `/media/developer/temp` and never removes that directory, which
  is the webOS 26 regression in dev-manager-desktop #403. It installs via
  `appInstallService/dev/install` and launches via
  `applicationManager/launch`.
* `--dry-run yes` prints the exact commands.

## Evidence

| Check | Result |
| --- | --- |
| Unit tests | 55 (`goya-arachnid`) + 14 (`goya-caprichios` webos) pass. |
| Model decoder | 43 model numbers seen on real TVs (Glasshouse, tab-tv) decode to the right year, family and catalogued series. Pre-2018 models and ambiguous ones are refused. |
| Reproducible packaging | Two builds of `fixtures/media-centre.gscene` for `model:OLED65C3PUA` are byte-identical (sha256 `c91aa378…`). |
| Independent format check | GNU `ar t/x`, `gzip -t` and `tar tzf` accept the IPK. Python `zlib` decodes the icon. |
| Remote runtime | In headless Google Chrome 157, 21/21 scripted checks pass. They cover the initial focus, 5-way traversal in all four directions and at the edges, held-key repeat, and one activation per OK even when a click arrives during the press. Stock-remote checks: OK still works after a stray `mousemove` and after hover without a platform cursor, and a press interrupted by blur, hide or relaunch does not swallow the next OK. Pointer checks run only after `cursorStateChange` reports the cursor visible. Back, colour and digit keys are not consumed. |
| Render | 1920×1080 screenshot of the fixture: grid laid out as designed, focus ring visible on Films. |

## Live scenes, motion, links and the splash (addendum, 20 November 2026)

Added for the Tapestry TV shell (`~/tapestry/tapestry-webos/shell`). The
owner decided as follows:
* Extend Goya minimally.
* The app must be **written only in `.gscene`**. Any other language,
  apart from Rust that extends Goya itself, counts as cheating.
* Every length is **parent-relative**.
* Every Arachnid build shows the **Arachnid splash**.

**GSSH/3 live admission** (`goya_lang::parse_strict_gssh3_live`, proposed)
is GSSH/3 plus the following. Every older profile refuses all of it
(`GY-E1012`).

| Construct | Meaning |
| --- | --- |
| `text-from: state.k;` | Show a declared key's value as text. |
| `image-from: state.k;` | Show the verified image a platform service publishes under a `host` text key. |
| `present-when: state.k is <literal>;` / `… at-least N;` | The subtree is present only while the key equals the literal, or, for an int key, is at least N. `at-least` lets a fixed set of rows show as many as a count says. Nested conditions combine. |
| `remote-digit: N;` (buttons) | Remote number key N activates the button while it is present. Two present buttons may not share a digit. |
| `k: type = v host;` | State written only by a platform service (`@link`). Interactions may read it but not write it. Each host key has exactly one writer (`GY-E1008`/`GY-E1013`). |
| `fill-to: #hex; fill-period-s: N;` | The fill breathes between `fill` and `fill-to` (1 s ≤ N ≤ 1 h). |
| `drift-p: (fx, fy, 0); drift-period-s: N;` | The subtree orbits an ellipse whose half-axes are fractions (≤ 0.5) of the **parent's** extent, like `position_p`. The ontology limit stands in for the root's parent. |
| `appear-s: N; appear-from-p: (fx, fy, 0);` | Each time the subtree becomes present, it fades in over N seconds (0.1–2) and optionally slides in from a parent-relative offset. |
| `@link NAME { protocol; endpoint; ROLE: state.k; field F: state.k; act A; slot S { image: state.k; label: state.k; } }` | A platform link to a peer. The scene binds roles (`status`, `code`, `detail`, `error`, `outbox`, `mode`), image/video slots, and **fields** (any host key the peer sets by name) to host keys, and declares the **acts** it may send. D2 `network` requests whose first word is `NAME` address it. |

**Arachnid** lowers these as follows (`live.rs`, `motion.rs`,
`lanlink.rs`, `splash.rs`):
* **Interactions.** D1 and D2 interactions follow the reference VM:
  reads from the pre-state, checked addition and atomic commit. Integers
  are limited to ±2^53−1 (`GY-AR-616`).
* **No app scripts.** There is no host-script escape hatch: the former
  `--host` option and `host.manifest` are gone.
  * `host` state and D2 effects are served only by Goya platform services
    the scene declares. Today that is `@link` with protocol
    `goya.lanlink/0`.
  * Each request is checked at compile time (`GY-AR-650`):
    * pair, confirm, cancel, forget and retry;
    * `decide APPROVE|REJECT|SELECT|RATE_n SLOT`, or `decide DEFER [SLOT]`,
      with a declared slot;
    * `playback toggle|back|forward|replay|mute SLOT`;
    * `act NAME [ARG]` with a declared act and a one-word argument.
  * Acts answer with field values, or with a new **revision** of the shown
    item, which the runtime swaps in place without a Receiving screen.
  * The link runtime (`lanlink.js`) is compiler output. It is the same for
    every app apart from the configuration table taken from the `@link`.
  * It reproduces the golden vector pinned in `tapestry-desktop` (`ring`)
    exactly: `check-arachnid.sh` → `lanlink vectors`.
* **Presence configurations.** Each condition key is partitioned by
  representative values, so `is` and `at-least` are treated exactly.
  * Every combination of representatives is enumerated, up to 10^6.
  * Combinations are then de-duplicated by the set of present nodes, up to
    1024 distinct sets (`GY-AR-642`).
  * Each distinct set gets its own focus graph, reachability check, trap
    check, overlap check and digit check.
  * The runtime finds a set's focus graph by its present buttons, and
    restores focus when the set changes.
  * The Tapestry shell compiles to 150 nodes and 70 buttons this way.
* **Motion is refused** when:
  * a drawn node leaves the limit at any point of its drift or appear path
    (`GY-AR-621`; a slide covers only its own direction);
  * a drift carries some buttons but not all (`GY-AR-633`);
  * black text over a breathing or relatively moving background, sampled
    over the whole cycle, falls below WCAG AA (4.5:1, or 3:1 at 24 px and
    above), or an image could move under text (`GY-AR-622`). Only nodes
    that can be present together are compared.
* **Emission is compositor-only.** Animations use only transform and
  opacity. Under reduced motion, decorative motion stops, a slide becomes
  a plain fade, and a drift carrying the interface (burn-in pixel shift)
  continues.
* **The splash is enforced.** Every build opens on `assets/arachnid-logo.png`,
  which is embedded in the compiler:
  * it holds for 1.2 s and fades out over 0.4 s;
  * the logo box is 0.6 of the limit's height, centred, with a feathered
    edge;
  * the 5-way keys and clicks are held until it ends, while Back still
    reaches the platform;
  * no scene can remove it.
  * `check-arachnid.sh` verifies that the logo is packaged byte-for-byte,
    shown and recorded (`compilation-manifest.json` → `splash`), and that
    input is held.

**Tapestry on this stack** (20 November 2026):
* **Video:** H.264/AAC MP4. The desktop inspects it by walking MP4 boxes
  in Rust; the TV verifies the bytes and tests its decoder before playing
  it.
* **Star ratings:** `remote-digit`.
* **Webpage review:**
  * The desktop decomposes a page's CSS custom properties into Colours,
    Typography, Sizes and Other, each with alternatives.
  * It renders the page offline in headless Chrome: scripts are refused by
    an injected CSP and the network resolves nowhere.
  * It outlines the elements the selected variable styles, and re-renders
    as alternatives are chosen.
  * The decision records the chosen variant (`web_variables` annotation).
  * Measured end to end: select → outline render → "Darker" → re-render →
    approve, persisted as `--bg-colour: #f6f1e7 → #ddcaa4`.

**Evidence** (20 November 2026):
* 113 `goya-lang` and 74 Arachnid tests pass, and 26/26 remote checks
  pass, 5 of them for the splash.
* The lanlink vectors pass 4/4.
* The pure-`.gscene` Tapestry 0.3.0 completed pair → approve → choose
  end to end with keys only, against the real desktop.
* Real-time sampling in Chrome showed a thread drifting about 195 px in
  10 s and the background cross-fading from 1% to 68%. Headless
  `--virtual-time-budget` screenshots do not show compositor animation
  reliably.

**Not done:**
* There is no pause control for decorative motion (WCAG 2.2.2).
* Static text has no general contrast check.
* Motion smoothness and frame rate have not been observed on the TV.
* `goya.lanlink/0` is a proof of concept: no reviewed pairing protocol and
  no TLS.
* LG's own `splashBackground` in `appinfo.json` is not used, because its
  behaviour on webOS 22 is unverified.

## Device run (20 November 2026)

The owner's TV is a **NEON XR55N21WEB24**, a webOS Hub licensee TV, used
with a stock remote and no Magic Remote.

| Gate | Result |
| --- | --- |
| Developer Mode | Works on this licensee TV. SSH listens on 9922 (`SSH-2.0-OpenSSH_6.1`) and the key server on 9991. `arachnid getkey` fetched the key. The passphrase is case-sensitive and is shown in upper case. |
| Platform | `/var/run/nyx/os_info.json` reports webOS TV Lite 7.5.8, build 28 (8 May 2026), aarch64, kernel 5.4.96. |
| Engine table | UA `Chrome/87.0.4280.88 … WebAppManager` matches the webOS 22 row exactly. `reported:7.5.8` and `ua:` both resolve to webOS 22, Chromium 87. |
| IPK acceptance | `arachnid deploy` installed and launched `com.goya.arachnid.probe` 0.1.0 and `com.goya.arachnid.media` 0.1.1 through `appInstallService/dev/install`. Stored-DEFLATE gzip, mtime 0 and our `Installed-Size` are accepted. |
| Viewport | 1920×1080 CSS px at DPR 2 on a 3840×2160 panel. This confirms the 1920×1080 ontology limit. |
| Stock-remote input | Arrows: `keydown`/`keyup` 37–40 with `key` = `Arrow*`. OK: `keydown 13 > keyup 13`, with no mouse events or `click`. No `mousemove` or `cursorStateChange` arrived. Back never reached the page; the platform closed the app, and the next launch was a fresh load. |

Trap: `PalmSystem.deviceInfo.platformVersionMajor` is **4** on this TV.
That is the manufacturing version 04.56.35, not the webOS version. Never
use it to resolve a target.

## What it does not do

* **Magic Remote not observed.** The owner has no Magic Remote, so the
  pointer path (whether OK in pointer mode sends `click`, `keydown 13` or
  both) is tested only in desktop Chrome.
* **Only one engine on a device.** Chromium 87 (webOS 22) has run the
  runtime. The other rows (53–132) are verified only against the table.
* **Colour keys are documented, not observed.** They are marked
  `Documented` in `remote::KEYS` until the probe sees them on a device.
* **No interaction semantics.** `goya:activate` is dispatched but nothing
  executes D1/D2 programs. Interactive apps need the shared browser
  state contract that Maja also lacks.
* **No ergonomics contract.** There are no minimum size, spacing,
  safe-area or contrast thresholds. LG's design guidance has not been
  read, and numbers will not be invented. The focus ring's white outer
  band disappears on light backgrounds; its contrast should be checked
  at viewing distance.
* **No nested Gscenes, scrolling, media, Luna services, Iberia/España,
  Tauromaquia or Mesut.** Each one is refused or absent, never stubbed.
* **No emulator.** LG's emulator has not been downloaded. The owner's TV
  is the device of record.
* **No Nevada sync.** Changes to Nevada's `focus.rs` are not tracked.
  Diff the two copies (`diff` after line 8) before changing either.
