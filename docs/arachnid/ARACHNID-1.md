# ARACHNID-1 — First webOS increment: engine caprichios, ontology limit, focus, packaging, deploy

Status: **Candidate**, 20 November 2026, branch `arachnid/v0.1`.
Implementation:
* `crates/arachnid` (`goya-arachnid`, contract `arachnid.webos/0.0.1`,
  binary `arachnid`);
* `crates/goya-caprichios/src/webos_engine_caprichio.rs` and
  `webos_tv_models.rs`.

Qualification: `scripts/check-arachnid.sh`. 55 Arachnid unit tests and 14
caprichio tests pass, both clippy-clean.

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
  * pointer and 5-way modes are tracked from `cursorStateChange`, mouse
    movement and arrow keys;
  * each press activates exactly once;
  * Back (461) is left to the platform.
* `REQUIRED_FEATURES` is empty, so one build runs on every platform from
  4.0 to 26. The gate (`660`) is in place for when that changes.

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
| Remote runtime | In headless Google Chrome 157, 12/12 scripted checks pass. They cover the initial focus, 5-way traversal in all four directions and at the edges, one activation per OK even when a click arrives during the press, hover focus, pointer click, arrow keys after pointer mode, and Back not consumed. |
| Render | 1920×1080 screenshot of the fixture: grid laid out as designed, focus ring visible on Films. |

## What it does not do

* **No TV contact yet.** Nothing has been installed on or launched on a
  TV. Deploy is tested only for command construction, reply parsing and
  key fetching against a local server. Whether webOS accepts these IPKs
  is unproven: in particular stored-DEFLATE gzip, mtime 0 and the
  `Installed-Size` value. Device gates stay **BLOCKED** until the owner's
  TV runs `arachnid getkey` / `deploy`.
* **The runtime has not run on a webOS engine.** The browser checks use
  desktop Chrome 157, not Chromium 53–132 on a TV. Whether OK in pointer
  mode sends `click`, `keydown 13` or both is still unobserved.
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
