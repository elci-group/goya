# Quitasol Q0 — Fire OS and Vega OS capability audit

Status: **proposed, not ratified**. This audit records what Amazon publicly
documents, as of 2026-11-18, about voice enablement and application
compilation for Fire TV. It asserts nothing about Goya or Quitasol
implementation; every entry is "as documented" with a source and a
confidence marker. Evidence record: [programme/quitasol-q0.json](programme/quitasol-q0.json).

## 1. Fire OS (Android-derived) voice mechanisms

The load-bearing finding: **for an arbitrary third-party Fire OS app, the
only self-serve, documented voice integration is the Media Session transport
path, plus app launch by name.** Everything else is partner-gated or has no
documented integration path.

| Mechanism | What it does | Integration path | Availability to a new third-party app | Confidence |
| --- | --- | --- | --- | --- |
| Media Session transport | Play/pause/stop, ±10s seek (no absolute seek), next/previous, restart | Android `MediaSession` callbacks + `com.amazon.permission.media.session.voicecommandcontrol` manifest permission (not required on Fire OS 14+) | **Self-serve** | documented — [Media Session API integration](https://developer.amazon.com/docs/fire-tv/mediasession-api-integration.html) |
| App launch by voice | "Alexa, open \<app>" opens the app | None (automatic for installed apps) | **Self-serve** | documented |
| Video Skills Kit (VSK) | Search-and-play, multi-turn disambiguation, channel change, deep-link launch | App-only broadcast receiver or cloudside Lambda | **Closed**: "No new partners will be onboarded to Video Skills Kit (VSK)"; catalog integration is select-partner-only | documented — [VSK app-only overview](https://developer.amazon.com/docs/video-skills-fire-tv-apps/integration-overview-app-only.html) |
| Voice scrolling & selection | Alexa maps commands to D-pad events | Back-end activation by Amazon after app verification | **Gated**, not self-serve | documented — [voice enablement](https://developer.amazon.com/docs/fire-tv/voice-enable-your-app-and-content.html) |
| Featured-content deep links | Fire TV UI deep links into content | Encoded intent submitted to an Amazon representative | Rep-mediated | documented (page updated Feb 2026) |
| In-app voice input | Speech recognition inside the app | — | **Not found.** Fire TV explicitly does not support the Android `RecognizerIntent` ("produces an error"); no dictation API documented | not found — [implementing search](https://developer.amazon.com/docs/fire-tv/implementing-search.html) |
| Microphone access | Raw `RECORD_AUDIO` | — | **Denied.** Fire OS silently denies mic permission to third-party apps without a dialog (Amazon moderator confirmation, community forum — not a formal policy page) | documented (moderator statement) |
| Embedded custom skill | App-invoked custom Alexa Skill | — | **Not found.** Skills are invoked conversationally through Alexa itself; no SDK for in-app skill integration | not found |

## 2. Vega OS

| Topic | Finding | Confidence |
| --- | --- | --- |
| SDK | Vega SDK 2.0 GA announced 2026-10-29 (out of open beta; formerly codename "Kepler"). One major release per year; minor releases keep a stable API surface. CLI is `vega` (formerly `kepler-cli`); tools install to `~/vega/sdk`. | documented — [Vega 2.0 launch](https://developer.amazon.com/apps-and-games/blogs/2026/10/vega-2-0-sdk-launch) |
| Development model | React Native for Vega (RNV): a fork of RN on the new architecture (Fabric + Turbo Modules), supporting RN 0.72 and 0.83 (React 19, Hermes v1). Component coverage is partial — TV focus helpers (TVFocusGuideView, TVEventHandler) and Vega extensions (FocusManager, KeplerAppState) are included; full RN parity is not claimed. | documented — [RNV overview](https://developer.amazon.com/docs/vega/2.0/vega-rn-overview) |
| Runtime delivery | The RN runtime and Hermes engine are **delivered by the OS**, not bundled in the app — apps ship JS bundles against the device-shipped runtime, so SDK/runtime version targeting matters more than bundling. | documented |
| Native path | Vega Native SDK: C++ with Turbo Modules over JSI/Node-API; W3C MSE/EME-spec media framework. **Rust: not mentioned in any official source** — a Rust backend would need a C ABI story at the Turbo Module seam. | documented (C++); not found (Rust) |
| Packaging | `.vpkg`, built with the VPT tool; submitted through the same Developer Console as APKs; an APK/AAB (Fire OS) and a VPKG (Vega) can share one Appstore listing. Known upload-validation issues reported Dec 2025. | documented |
| APK compatibility | Android APKs **do not run** on Vega devices; no consumer or developer APK sideloading. Existing Android apps must be ported/rewritten against the Vega SDK. | documented |
| Device status | Fire TV Stick 4K Select (Oct 2025) was the first Vega device; **Fire TV Stick 4K 3rd Gen (2026) ships Vega OS 2.0** — confirming the Quitasol proposal's device claim — while Fire TV Stick 4K Max and Fire TV Cube remain Fire OS. Amazon states all future Fire TV Sticks ship Vega-only. | documented for the named devices; partially documented for a complete list |
| Third-party voice integration | Platform-mediated: Alexa voice control of app navigation and media playback is built into the RNV framework, and Amazon documents migrating catalog and voice integration from Fire OS to Vega (legacy ADB-based integration is **not** compatible with Vega). The precise third-party voice **API contract** (what an app implements beyond platform defaults) is the thinnest-documented area; no Vega VSK equivalent, no AVS embedding for apps, and the Vega Virtual Device lacks Alexa voice services, so voice testing requires registered hardware. | partially documented / API contract not found |
| Framework support | React Native is the only officially supported app framework; C++ native modules are the sanctioned second path. No official Flutter support; no publicly documented working port of another runtime. | documented |
| Program terms | Vega developer program ran under NDA; terms not public. On-device testing requires Developer Mode (Vega CLI authentication) and registered devices; consumer sideloading blocked. | partially documented |

## 3. Appstore certification and policy constraints

| Constraint | Finding | Confidence |
| --- | --- | --- |
| Binary format | APK or AAB (Fire OS track); AAB converted by Amazon via bundletool; 64-bit required; up to 15 binaries per app | documented |
| Target SDK | Amazon issues action-required notices for Android 14 targets (~33–35 expected); exact current floor not in one public page | partially documented |
| DRM | Widevine L1/L3 hardware-dependent for HD/4K | documented |
| Submission | Privacy policy URL required; working test credentials for sign-in apps; no binding review SLA (commonly days) | documented / partially documented |
| Voice privacy | No dedicated public Appstore voice-data policy page for Fire TV apps was found | not found |
| Custom skills (if any) | Alexa skill policy requirements apply, including privacy review; child-directed skills bring COPPA and stricter kid-skill rules | documented |
| Vega program terms | The Vega developer program ran under NDA; its terms are not public. Building a compiler target means joining sight-unseen from a public-document perspective. | partially documented |

## 4. Consequences for the Quitasol lowering map

This audit re-anchors [intents.md](intents.md) §4 against evidence:

1. **The realistic Fire OS lowerings are transport controls and launch.** The
   media/playback tier of the integration hierarchy is self-serve today; the
   native-platform tier reduces to app launch by name; search/discovery via
   VSK is closed to new partners and must be recorded as an external
   constraint, not a compiler capability.
2. **In-app voice input does not exist as an integrable API.** Utterances can
   only be resolved by system Alexa; the Voice Accessibility Engine and
   Context Resolution Engine cannot assume app-side speech recognition.
3. **The deterministic fallback tier is not optional in practice** — it is
   the only tier guaranteed to every app. This validates the phase-0
   decision to make non-voice fallback mandatory.
4. **Vega voice support is platform-first, API-thin.** Navigation and media
   transport are voice-controllable on Vega by platform default, and Amazon
   documents a Fire OS→Vega voice migration path — but the third-party voice
   *API contract* is not documented, and no VSK equivalent or AVS embedding
   exists for Vega apps. Q3 (native Alexa media and navigation integration)
   must be planned per-OS: self-serve on Fire OS via Media Session;
   platform-default on Vega with an unresolved custom-action story.
5. **Dual-track packaging is a hard requirement**: APK/AAB for Fire OS
   devices and `.vpkg` for Vega devices under one listing. The Dual Backend
   Compiler subsystem table stands, with the Vega track additionally gated
   on the non-public developer-program terms.

## 5. BLOCKED and unchanged

- On-device voice testing on Vega: BLOCKED (no Alexa services in the Vega
  Virtual Device; sideloading gated).
- VSK/search-and-play for Quitasol-generated apps: BLOCKED (partner
  onboarding closed).
- Appstore conformance claims (Q8): no evidence; nothing submitted.
- Device-matrix RAM/perf rows: not individually verified; the authoritative
  source is Amazon's device specifications page, which must be re-checked at
  admission time.

## 6. Sources

Primary: developer.amazon.com Fire TV docs (Media Session integration, VSK
app-only overview, voice enablement, implementing search, deep linking,
device specifications), Vega docs (portal, VPT, live app testing, developer
mode, VVD limitations), Appstore submission and policy pages, Alexa custom
skills policy pages, Amazon Developer Community forum (mic permission),
press coverage for the Vega device split. Full per-claim URLs are in the
research notes and mirrored in [programme/quitasol-q0.json](programme/quitasol-q0.json).
Confidence markers: **documented** (Amazon page states it directly),
**partially documented** (stated but incomplete or secondary-sourced),
**not found** (no public documentation located; absence of evidence is
recorded as absence, not as impossibility).
