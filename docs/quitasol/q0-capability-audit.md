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
| Development model | React Native for Vega via the Vega SDK (v0.24 as of Oct 2026; 2.0 out of open beta). Existing Android APKs do not run on Vega devices. | documented |
| Packaging | `.vpkg`, built and validated with the VPT tool; submitted through the same Developer Console as APKs; an APK and a VPKG can share one Appstore listing. | documented |
| Device status | Fire TV Stick 4K Select (Oct 2025) is the first Vega device; Fire TV Stick HD (Apr 2026) and Fire TV Stick 4K 3rd Gen (Sept 2026) follow; the 4K 3rd Gen ships Vega OS 2.0 with Alexa+. Amazon states all future Fire TV Sticks ship Vega-only. | partially documented (press-verified device split; full RAM table not row-verified) |
| Third-party voice integration | **Unsettled.** No documented Alexa directive API for third-party Vega apps surfaced; the Vega Virtual Device explicitly lacks Alexa voice services (directives, dictation), so voice testing requires registered physical hardware. Community sources indicate playback-state/Alexa integration is in progress/proposal stage. | partially documented / treat as undocumented |
| Sideloading | Blocked on Vega devices; on-device testing requires Developer Mode via Vega CLI authentication and device registration in the Developer Portal. | documented |

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
4. **Vega voice support must be treated as BLOCKED.** No third-party Vega
   voice API is documented; Q3 (native Alexa media and navigation
   integration) is achievable on Fire OS only until Amazon documents a Vega
   path.
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
