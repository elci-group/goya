<p align="center"><img src="../../assets/arachnid-logo.png" alt="Arachnid — webOS app compiler" width="180"></p>

# Goya webOS Launcher: reconciliation and plan

Status: **Slice 1 implemented offline**, 26 November 2026, branch
`arachnid/v0.3` (from `arachnid/v0.2` at `290c91c`), uncommitted. Slice 0
(the device probe) is built but has not run: the TV did not answer on the
LAN that day. See §7.

The owner pasted a design for a "Goya webOS Launcher". It describes a household
computing interface for the TV, written in `.gscene` and compiled by Arachnid,
built on five principles: input economy, spatial intelligence, household
awareness, resource economy and personal continuity.

That design was written without looking at the repository. As with the first
Arachnid proposal ([roadmap §1](roadmap.md)), this file compares it with what
the tree contains today. It then proposes a sequence of slices, each of which
leaves `scripts/check-arachnid.sh` green, and lists the decisions only the
owner can make.

The governing rule still holds: **the launcher is `.gscene` only**. Anything
`.gscene` cannot express is added to `goya-lang` or Arachnid in Rust, and
Arachnid generates the runtime for it.

## 1. What each proposal section depends on

| Proposal section | What exists | What is missing |
| --- | --- | --- |
| §1 Home interface (configurable, collapsible sections) | Cells, buttons, `present-when`, focus slots, motion, `@graphic`/`@draw`, durable state in `webos.local-storage.v1` (ARACHNID-2). This is enough for a static home screen with sections that can be shown or hidden. | Nothing for a first version. Reordering sections needs either precomputed orders (as Pairs does) or a list construct. |
| §2 Colour buttons as command channels | `remote.rs` lists Red/Green/Yellow/Blue (403–406) as **Documented, Platform**: the runtime does not consume them. `remote-digit` shows the pattern for binding a key to a button. | A `remote-colour` property on `@button`, mirroring `remote-digit`. A check that the TV actually delivers 403–406 to the app (not yet observed on the NEON). Colour assignments per profile need presence variants (one button per profile per colour, which focus slots already handle). "Exclude intents a native key already covers" needs a per-remote capability table; the owner's remote is the stock one. |
| §3 Profiles | Durable D8 state, `present-when`, literal domains. A profile can be a session key whose literal domain is the set of profiles. | PIN entry: `remote-digit` exists, but storing a secret in `localStorage` is not protection (anyone with Developer Mode can read it), so the PIN should be described as a child lock, not as security. Per-profile appearance needs either precomputed variants or bindable colours. Guest sessions map onto a non-durable profile. |
| §3 romántico session resume | `goya-romantico` (`GY-ROMANTICO/0.1`) proves handoff **in one process**. Its transport admission refuses every adapter, because none reports authenticated encryption. | A romántico transport over `goya.lanlink/0`, which already has ECDH, HKDF and AES-256-GCM. A `.gscene` construct for "resume". No other Goya app on the TV holds romántico state yet. |
| §4 Home HUD | `@link` / `goya.lanlink/0` connects the TV to one paired peer, with `field` values and `act` requests. `~/bloom` has `bloomd`, which serves local resource telemetry (CPU, memory, storage, GPU/NPU, thermal) over HTTP. | A **household peer** on the desktop that the TV pairs with over lanlink and that aggregates bloom nodes for it. The TV should not talk to every device itself. Wake-on-LAN needs UDP broadcast, which a packaged web app cannot send, so it must come from an awake peer. Home Assistant has nothing in the tree. |
| §5 Occlusion-aware notifications | Arachnid knows every node's geometry, its `@iden`, whether it is focusable, its motion and its text at build time. | The cost function, evaluated at build time over the launcher's own scene. Each presence configuration gets a precomputed notification rectangle (or "defer"), and the runtime looks it up. This is precomputation, as the proposal suggests. **Only inside the launcher.** A stock app sees no pixels from other apps, so placement over third-party video is not possible (§3 below). |
| §6 OBS control | `~/tapestry` shows the pattern: a desktop peer, with the TV as a control surface. | OBS WebSocket v5 belongs in the household peer, not in TV code. The TV sends `act obs.scene Gameplay` and receives `field` values such as encoder load, bitrate and dropped frames. |
| §7 Resource observatory | `@graphic` with GVI-D/A/O, and `gvi.js` on the TV. Bloom telemetry for the household's other machines. | Live series data. GVI precomputes finite state models, so a graph needs a quantised model, for example a sparkline of N columns × K levels. **TV telemetry is the hard part**: a web app on webOS 22 has no supported API for system CPU or memory (to verify; §4). The observatory can show the household's machines and the launcher's own frame timings, but not the TV's load. |
| §8 Command palette | `remote-digit`, focus slots. | Text input from a stock remote means an on-screen keyboard; `@input` lowering is not done (roadmap §5a, Phase 3). A palette restricted to the launcher's own entries can be a filtered list of precomputed presences. |
| §9 Viuda-hosted widgets | Viuda runs Pairs natively on the TV (memory: viuda-status). | Not needed for the first slices. |
| §10 Launch other apps | **Nothing.** Arachnid has no Luna binding. | A Luna lowering for `com.webos.applicationManager/launch` (and `listLaunchPoints`, if a third-party app may call it). It must be a Goya construct with a generated runtime, like lanlink. This is the one capability without which the app is not a launcher. |
| §11 Targets | `check-arachnid.sh` measures structure in headless Chrome. | Latency, idle CPU and memory on the TV need a device harness. The ≤1% idle CPU target cannot be measured from inside a stock app. |

## 2. Corrections to the proposal

* **"Arachnid should negotiate two editions from one source."** Arachnid can
  refuse at compile time what an edition cannot do, with a diagnostic, as it
  already does for durable stores (`GY-AR-651`). That is the part of
  "negotiation" that fits the existing design. A System Edition, though, is
  not a build flag (§3).
* **"Notification placement ≤2 ms."** With precomputation the runtime only
  looks up a value, so the budget falls on the build instead. The number to
  report is the count of presence configurations and the build time.
* **"Precompute reachable notification configurations through GVI."** GVI is
  for drawings. The placement table belongs in Arachnid's presence engine,
  next to focus slots. GVI only draws the notification.
* **Home HUD reach.** The TV pairs with **one** peer per `@link`. A
  household view therefore comes from a peer that aggregates the others,
  not from the TV contacting each device.
* **Profiles and privacy.** "Private notifications" and a "PIN" on a shared
  TV are courtesy features. Anyone who can enable Developer Mode can read
  app storage. The UI text should not claim security it does not have.

## 3. Editions

The proposal is right that these are different things.

**Standard Edition** is an ordinary IPK, installed like Pairs and Tapestry.
It is a launcher *app*. The Home key still opens LG's home, the app does not
start at boot, and it sees nothing outside its own window. Every slice in §5
targets this edition.

**System Edition** means replacing or overlaying LG's system UI. On webOS
that requires root (for example the community Homebrew Channel tooling).
Rooting may void the warranty, may be closed by a firmware update, and on a
webOS Hub licensee such as the NEON its availability is unknown. It is not
something Arachnid can "negotiate" at build time. It is a decision about the
owner's TV. This plan does not touch it unless the owner asks, and it would
then start with a restore path.

## 4. Platform facts to verify before Slice 1

None of these has been observed on the NEON (webOS 22, Chromium 87):
1. Whether the stock remote's colour keys reach a third-party app as
   keydown 403–406, or whether the system takes any of them.
2. Whether a third-party web app may call
   `luna://com.webos.applicationManager/launch`, through which bridge
   (`PalmServiceBridge` / `webOSServiceBridge`), and whether it needs an
   `appinfo.json` permission.
3. Whether `listLaunchPoints` or `listApps` is available to a third-party
   app. If not, the app list must be declared in the scene (app IDs the
   owner lists) rather than discovered.
4. Whether any system telemetry API (CPU, memory) is open to third-party
   apps on webOS 22.

**Answers** (`examples/luna_probe.rs` on the NEON, 26 November 2026):
1. **Colour keys reach the app:** keydown/keyup 403–406, `key` =
   `Unidentified`; held keys repeat. `remote.rs` now records them as
   Observed.
2. **Bridges:** both `PalmServiceBridge` and `WebOSServiceBridge` exist.
   `launch` was called when OK was pressed, but its answer never reached
   the report. The probe was most likely backgrounded by Pairs coming to
   the front. **Whether Pairs opened depends on the owner's report.**
3. **`listLaunchPoints`, `listApps` and `getForegroundAppInfo` are denied**
   (`errorCode -1`, `Denied method call … for category "/"`). The
   Developer Mode SSH jail does not expose the application directories
   either. The launcher therefore keeps a **declared** app list: it cannot
   discover installed apps. A wrong id shows up as the launcher's own
   "Could not open APP: …" notice.
4. **No TV telemetry:** `memorymanager/getCurrentMemState` is denied and
   `com.webos.service.resourcemonitor` does not exist.
   `tv.systemproperty/getSystemInfo` works (model XR55N21WEB24, firmware
   04.56.35, sdkVersion 7.5.8, UHD true). The observatory (§7 of the
   proposal) can show the household's machines through Bloom, but not the
   TV's own load.

Item 1 was originally to be checked with `examples/input_probe.rs`. Items 2–4 need a probe
IPK. **A probe app that calls Luna is test tooling, not launcher code**: it
stays under `tests/`, like Tapestry's.

## 5. Proposed slices

Each slice ends with `scripts/check-arachnid.sh` and uni passing, and a
device run that depends on the owner's report.

**Slice 0: Probe.** Verify §4 on the TV. No language change.

**Slice 1: A launcher you can use.**
* `goya-lang`: `remote-colour: red|green|yellow|blue` on `@button`, admitted
  by the live profiles only. Other targets refuse it through
  `extended_live_construct`.
* Arachnid: consume 403–406 when a present button claims the colour, with a
  compile-time check that at most one present button claims each colour.
  Change their evidence to Observed after Slice 0.
* `goya-lang` + Arachnid: a launch request, for example the D2 request
  `system launch <app-id>`, checked at compile time against a declared app
  list (a new `GY-AR-6xx` code). The Luna call is generated runtime.
* `apps/launcher.gscene`: a profile picker (durable `profile`), a home grid
  of declared apps per profile, and four colour channels. Red is Apps; the
  other three open placeholder panels that say they are not connected.

**Slice 2: Household peer.** A desktop crate, outside the TV app, that
pairs over lanlink and offers `field` values from local `bloomd` nodes, plus
`act` requests for Wake-on-LAN. The launcher's Green panel binds them with
`text-from` and a quantised GVI gauge.

**Slice 3: Broadcast.** The same peer speaks OBS WebSocket v5 to a local OBS
and exposes scene selection, start and stop, and stats as acts and fields.
This is the Yellow panel.

**Slice 4: Placement.** A build-time occlusion cost over the launcher's
scene graph: protected text, focusable controls and `@iden`-weighted
importance, with motion from motion properties. It precomputes a
notification slot per presence configuration, or "defer". Notifications
arrive from the peer.

**Slice 5: Continuity.** A romántico transport over lanlink, and resume
for a Goya app the TV already runs (Pairs).

That order delivers the proposal's closing demonstration step by step:
profile, then wake the desktop, then OBS scene, then a placed notification,
and finally resume.

## 6. Owner decisions (26 November 2026)

1. **Scope now:** Slice 0 and Slice 1.
2. **Where:** a new branch `arachnid/v0.3` from `arachnid/v0.2`, worktree
   `~/goya-arachnid-v03`. Pairs on v0.2 is left as committed.
3. **Household peer (Slice 2):** in `~/bloom`.
4. **System Edition:** out of scope. Standard Edition only.

## 7. What was built

**Language** (`goya-lang`, live profiles only; GSSH/3 and GSSH/4 refuse
both with `GY-E1012`):
* `remote-colour: red|green|yellow|blue;` on a `@button`
  (`goya_lang::RemoteColour`). It may not be used on a cell or given twice.
* `@apps NAME { id: "…"; app APP: "platform.app.id"; … }`
  (`goya_lang::AppsDecl`). It needs at least one app, and app names are
  unique.
* `extended_live_construct` names both, so Maja and Sash refuse them.
  Aragon (`android_scene.rs`) and Emperor (`scene_build.rs`) refuse them at
  their range rather than dropping them.

**Arachnid:**
* **Colour keys** (`remote.rs`, `ontology.rs`, `emit.rs`). 403–406 now have
  `Handling::Colour`. A key activates the present button that claims it,
  focuses it, and prevents the default. With no claimant, the key is left to
  the platform. For every presence configuration, at most one present
  button may claim each colour (`GY-AR-650`). The compatibility report
  lists `colour_keys` separately. The keys' evidence stays `Documented`
  until the probe sees them.
* **`@apps`** (`apps.rs`, contract `arachnid.apps/0.0.1`).
  * Every request `NAME launch APP` is checked at compile time against the
    declared apps (`GY-AR-652`, new). App ids must look like webOS ids.
  * The generated `apps.js` calls
    `luna://com.webos.applicationManager/launch` through
    `PalmServiceBridge` (or `WebOSServiceBridge`), with an 8 s timeout.
  * It returns text: `Opening APP`, or `Could not open APP: <reason>`. It
    never throws, and nothing it is not told to launch can be launched.
  * A launch is a `network` effect: Luna is the system message bus, and
    this leaves the determinism contract unchanged. **Owner review point.**
* **Routing.** `Arachnid.provide(label, fn, route)` lets a provider serve
  only requests whose first word is its name, so `@link` and `@apps` can
  both serve `network` in one scene.
* **Probe** (`examples/luna_probe.rs`, test tooling, IPK
  `com.goya.arachnid.lunaprobe`).
  * It logs colour keys and makes the read-only Luna calls from §4:
    `listLaunchPoints`, `listApps`, `getForegroundAppInfo`,
    `getSystemInfo` and two telemetry candidates.
  * It launches Pairs only when OK is pressed.

**`apps/launcher.gscene`** ("Goya Launcher") is the whole app.
* **Profiles.** Owner, Family and Guest. `profile` is durable in
  `webos.local-storage.v1`, and the picker shows at every launch.
* **Favourites.** Each profile has its own favourites (3–4 apps), and the
  library shows six declared apps.
* **Channel bar.** Home, then red Applications, green Household, yellow
  Broadcast and blue Activity, then Profiles.
* **Placeholder panels.** Household, Broadcast and Activity say that
  nothing is connected yet.
* **Logo.** `@graphic logo`, drawn in the header beside "Goya": a navy tile
  with four rounded squares in the colour-key colours around a home point.
  Its `channel` dimension follows the open panel, so the other three
  squares fade to 30 %. It compiles to `graphics/logo.gvi` (1,856 bytes).
  * The app icon `assets/launcher-icon-80.png` is the same geometry,
    rendered at 80×80 by headless Chrome from an SVG copy of the shapes.
    The two are kept in step by hand: if the graphic changes, re-render the
    icon.
  * The harness checks logo pixels: all four squares at full strength on
    the picker, and green dimmed while Applications is open.
* **App logos.** Every app tile is a text-less `@button` with two cells: a
  16:9 `@draw` logo and a label. On Favourites the label sits under the
  logo; on Applications it sits beside it. The `@iden` label is unchanged.
  * YouTube, Netflix and Prime Video are brand marks from simple-icons
    (CC0 drawings). Prime Video comes from 9.0.0, because later releases
    removed it at Amazon's request.
  * Web Browser and Live TV use generic Material Design Icons (a globe and
    a TV).
  * Goya Pairs is drawn natively from Pairs' own star and heart cards.
  * Sources, versions, licences and trademark notes are in
    `assets/app-logos/SOURCES.md`. Conversion settings are in
    `assets/app-logos/logos.tsv`.
  * Each logo compiles to `graphics/logo_<app>.gvi`.
* **Live sky.** The root draws `sky: local`: the real sky over the TV's
  time-zone location, now. Every piece of text sits on an opaque card,
  because Arachnid refuses text on the sky:
  * the brand, profile name and headings are pills;
  * Household, Broadcast, Activity and Settings are cards;
  * the notice is a bar.
* **Colour groups** (owner decision, 26 November 2026). The colour keys
  select apps instead of panels.
  * Each app belongs to one colour, and its tile wears a chip in that
    colour: YouTube and Web Browser red; Netflix and Live TV green; Prime
    Video yellow; Goya Pairs blue.
  * On Home and Applications, a colour key jumps to the next included app
    of its colour and cycles. It is left to the platform where no such
    app is shown.
  * A legend pill beside each heading explains the keys.
  * Panels are reached from the channel bar, which no longer claims keys
    and is neutral in colour.
* **Settings** (channel 6). Each app has a row with its logo, its chip
  (solid when included, outlined when omitted) and **Include**/**Omit**.
  * These write a durable `cc_<app>` flag, which gates the tile's
    `colour-group`.
  * There are two buttons, not one toggle, because `set` has no negation.
    A pair of stacked `present-when` buttons would multiply the
    presence signatures past their bound.
* **App ids.** They are LG's usual ids and are **unconfirmed** on the NEON.
  The probe's `listLaunchPoints` decides them.
* **Authoring.** The button and interaction rows were typed out by a
  throwaway script, which is not shipped (as with Pairs). The `.gscene` is
  the source.

**Qualification** (`scripts/check-arachnid.sh`, all passing):
* The launcher builds byte-identically twice.
* Headless Chrome drives it with arrows, OK and colour keys (38 checks):
  * an unclaimed red key is left to the platform;
  * a profile is chosen and written as a D8 record;
  * the favourites are per profile;
  * the live sky draws, in WebGL under SwiftShader: blue at a pinned noon,
    dark at midnight;
  * colour keys jump to the first app of their colour, cycle and wrap,
    and are left to the platform where no app of the colour is shown,
    including on Household and Settings;
  * the channel bar opens Applications, Household and Settings;
  * Omit takes Live TV out of the green cycle, and its tile stays;
  * a launch makes the exact Luna call, with its D9 announcement;
  * a refused launch is reported, and so is a TV with no bridge;
  * Profiles returns to the picker.
* A relaunch restores the profile and the omission (4 checks).
* The sky runtime is checked under Node (18 checks).
* Each imported logo is regenerated with `arachnid import-svg` and must
  appear verbatim in the scene and ship as a GVI. The page must carry the
  rule that keeps a focused tile's logo and label above its focus ring.
* The Luna bridge in the harness is a stand-in.
* Unit tests: goya-lang 132, Arachnid 87.
* `check-arachnid.sh` used to let a FAIL inside a browser harness pass,
  because `set -e` ignores `&&` lists and `!` commands. Each harness now
  exits explicitly.
* uni: Arachnid 92, goya-lang 73 (the pre-existing gate failure, unchanged).

## 8. Next

1. **Slice 0 on the TV**, when it is on and in Developer Mode at
   `192.168.1.182`:
   `arachnid deploy <lunaprobe.ipk> --host 192.168.1.182 --key ~/.ssh/webos_tv_open --launch yes`.
   Read the log with `--report`.
2. **Act on the probe's answers:**
   * correct the app ids;
   * set colour-key evidence to `Observed` or record which keys the system
     takes;
   * if `launch` is refused, record that the Standard Edition cannot launch
     apps on this TV.
3. **Deploy the launcher** and wait for the owner's report of what the TV
   shows.
4. **Slice 2** in `~/bloom`.
