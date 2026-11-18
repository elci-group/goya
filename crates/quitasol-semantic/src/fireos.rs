//! Q2 Fire OS integration emission over the Media Session transport path
//! (proposed, not ratified).
//!
//! The Q0 capability audit anchors this module: the **only self-serve,
//! documented Fire OS voice integration** for a third-party app is the
//! Media Session transport path plus automatic app launch by name. Search
//! and discovery (VSK) is closed to new partners; in-app voice input does
//! not exist. This module therefore emits a deterministic integration
//! *plan* — manifest permission entries and per-intent Media Session
//! transport actions — and records everything else as `Unresolved`. It
//! produces **no APK, no project, and no runtime**: a transport mapping is
//! a candidate binding, not working voice control.
//!
//! Consumes the Q1 [`ResolvedManifest`]; fail-closed and deterministic.

use crate::resolve::ResolvedManifest;
use goya_lang::intents::IntentDiagnostic;
use goya_lang::{Scene, SourceRange, TransportClass};

/// Contract identifier of the emitted plan. Draft revision: proposed, not
/// ratified; the plan shape may change.
pub const FIRE_OS_PLAN_CONTRACT: &str = "QUITASOL-FIREOS-PLAN/1-draft";

/// The Fire OS manifest permission Amazon documents for Media Session voice
/// transport control. Q0 records that this permission "is not required for
/// Fire OS 14 and later" — recorded in the plan as a note, never resolved
/// against a device.
pub const MEDIA_SESSION_PERMISSION: &str =
    "com.amazon.permission.media.session.voicecommandcontrol";

/// Q0 audit citation recorded in every plan for evidence traceability.
pub const Q0_AUDIT_DOC: &str = "docs/quitasol/q0-capability-audit.md";

/// One documented Android MediaSession callback action, mapped 1:1 from a
/// [`TransportClass`]. Names follow the Android `MediaSession.Callback`
/// methods the app would implement; no code is generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportAction {
    /// e.g. `onPlay`, `onSkipToNext`.
    pub callback: &'static str,
    /// The authored transport keyword the mapping came from.
    pub keyword: &'static str,
}

/// Map one authored transport class to its documented Media Session
/// callback actions. Seek classes map to offset-based callbacks only —
/// absolute-time seek is not supported by the Fire OS voice path (Q0).
pub fn transport_actions(class: TransportClass) -> &'static [TransportAction] {
    const PLAY_PAUSE: &[TransportAction] = &[
        TransportAction { callback: "onPlay", keyword: "play_pause" },
        TransportAction { callback: "onPause", keyword: "play_pause" },
    ];
    const SKIP_NEXT: &[TransportAction] =
        &[TransportAction { callback: "onSkipToNext", keyword: "skip_next" }];
    const SKIP_PREVIOUS: &[TransportAction] =
        &[TransportAction { callback: "onSkipToPrevious", keyword: "skip_previous" }];
    const SEEK_FORWARD: &[TransportAction] =
        &[TransportAction { callback: "onSeekTo+offset", keyword: "seek_forward" }];
    const SEEK_BACKWARD: &[TransportAction] =
        &[TransportAction { callback: "onSeekTo-offset", keyword: "seek_backward" }];
    const RESTART: &[TransportAction] =
        &[TransportAction { callback: "onSeekTo:0", keyword: "restart" }];
    match class {
        TransportClass::PlayPause => PLAY_PAUSE,
        TransportClass::SkipNext => SKIP_NEXT,
        TransportClass::SkipPrevious => SKIP_PREVIOUS,
        TransportClass::SeekForward => SEEK_FORWARD,
        TransportClass::SeekBackward => SEEK_BACKWARD,
        TransportClass::Restart => RESTART,
    }
}

/// The Q2 lowering status of one intent. An intent whose declaration
/// carries a `transport` key maps to its Media Session actions; every
/// other intent stays `Unresolved` — search or discovery lowerings would
/// require VSK, which the Q0 audit records as closed to new partners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentLowering {
    /// The intent declared a transport class; these are its documented
    /// Media Session callback actions.
    MediaSessionTransport(Vec<TransportAction>),
    /// No documented self-serve lowering exists for this intent. Recorded
    /// honestly; never silently dropped.
    Unresolved,
}

/// One per-intent plan entry, sorted by intent name in the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireOsIntent {
    pub name: String,
    pub lowering: IntentLowering,
}

/// App launch support. Automatic for every installed Fire OS app (Q0);
/// recorded per plan for evidence, never as developer work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchSupport {
    /// Voice launch by app name is provided by the platform.
    pub by_voice: bool,
}

/// The deterministic Fire OS integration plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FireOsPlan {
    intents: Vec<FireOsIntent>,
    permission_declared: bool,
}

impl FireOsPlan {
    /// Per-intent entries, sorted by intent name (deterministic).
    pub fn intents(&self) -> &[FireOsIntent] {
        &self.intents
    }

    /// The plan's manifest permission entries: the Media Session voice
    /// permission exactly when at least one intent declares transport.
    pub fn permission_entries(&self) -> Vec<&'static str> {
        if self.permission_declared {
            vec![MEDIA_SESSION_PERMISSION]
        } else {
            Vec::new()
        }
    }

    /// App launch support (platform-provided for every installed app).
    pub fn launch(&self) -> LaunchSupport {
        LaunchSupport { by_voice: true }
    }

    /// Deterministic JSON serialisation. Byte-identical for equal plans.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"contract\":");
        crate::push_json_string(&mut out, FIRE_OS_PLAN_CONTRACT);
        out.push_str(",\"profile\":");
        crate::push_json_string(&mut out, crate::INTENT_PROFILE);
        out.push_str(",\"lowering_tier\":");
        crate::push_json_string(&mut out, crate::LOWERING_TIER);
        out.push_str(",\"anchor\":");
        crate::push_json_string(&mut out, Q0_AUDIT_DOC);
        out.push_str(",\"launch\":{\"by_voice\":true}");
        out.push_str(",\"permissions\":[");
        for (i, permission) in self.permission_entries().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            crate::push_json_string(&mut out, permission);
        }
        out.push(']');
        out.push_str(",\"intents\":[");
        for (i, intent) in self.intents.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            crate::push_json_string(&mut out, &intent.name);
            out.push_str(",\"lowering\":");
            match &intent.lowering {
                IntentLowering::MediaSessionTransport(actions) => {
                    out.push_str("{\"media_session_transport\":[");
                    for (j, action) in actions.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        out.push_str("{\"callback\":");
                        crate::push_json_string(&mut out, action.callback);
                        out.push_str(",\"keyword\":");
                        crate::push_json_string(&mut out, action.keyword);
                        out.push('}');
                    }
                    out.push_str("]}");
                }
                IntentLowering::Unresolved => {
                    out.push_str("\"unresolved\"");
                }
            }
            out.push('}');
        }
        out.push_str("]}");
        out
    }
}

/// Emit the Fire OS integration plan for a resolved manifest.
///
/// Fail-closed: the Q1 resolution must already have succeeded (it is passed
/// in as a value, so failure ordering is enforced by construction); the
/// scene's intents and the resolved manifest must agree on the intent set
/// (`QS-E2004` otherwise — a mismatch means the manifest and scene are from
/// different sources). Deterministic: entries sorted by intent name.
pub fn emit_fire_os_plan(
    scene: &Scene,
    resolved: &ResolvedManifest,
) -> Result<FireOsPlan, IntentDiagnostic> {
    let mut intents = Vec::new();
    for intent in &scene.intents {
        // Manifest agreement: the Q1 resolution covers exactly this intent.
        if resolved.fallbacks().iter().all(|fallback| fallback.intent != intent.name) {
            return Err(IntentDiagnostic {
                code: "QS-E2004",
                name: format!(
                    "intent {} has no Q1 fallback resolution; manifest and scene are from different sources",
                    intent.name
                ),
                range: SourceRange { start: 0, end: 0 },
            });
        }
        let lowering = match intent.transport {
            Some(class) => {
                IntentLowering::MediaSessionTransport(transport_actions(class).to_vec())
            }
            None => IntentLowering::Unresolved,
        };
        intents.push(FireOsIntent {
            name: intent.name.clone(),
            lowering,
        });
    }
    // Deterministic output regardless of declaration order.
    intents.sort_by(|a, b| a.name.cmp(&b.name));
    let permission_declared = intents
        .iter()
        .any(|intent| matches!(intent.lowering, IntentLowering::MediaSessionTransport(_)));
    Ok(FireOsPlan {
        intents,
        permission_declared,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::resolve_manifest;
    use goya_lang::parse;

    const SCENE: &str = "\
@ontology app {
    @button play { position: (100, 100, 0) extent: (200, 60, 0) }
    @button search { position: (400, 100, 0) extent: (200, 60, 0) }
    @interaction toggle {
        trigger: button.play.activate;
        class: D1;
        state { }
        input { }
    }
    @interaction find {
        trigger: button.search.activate;
        class: D1;
        state { }
        input { }
    }
    @intent playback {
        action: toggle;
        transport: play_pause;
        voice { utterance: \"Pause playback\"; confirmation: true; }
        fallback { remote: play; }
    }
    @intent catalogue {
        action: find;
        voice { utterance: \"Find {query}\"; }
        fallback { remote: search; }
    }
}
";

    fn resolve(scene_text: &str) -> ResolvedManifest {
        resolve_manifest(
            &parse(scene_text).unwrap(),
            "oem.acme.voice-remote/1.0.0",
            800.0,
            600.0,
        )
        .expect("scene resolves")
    }

    #[test]
    fn transport_intents_map_to_documented_callbacks_others_stay_unresolved() {
        let resolved = resolve(SCENE);
        let plan = emit_fire_os_plan(&parse(SCENE).unwrap(), &resolved).unwrap();
        let intents = plan.intents();
        assert_eq!(intents.len(), 2);
        assert_eq!(intents[0].name, "catalogue");
        assert_eq!(intents[0].lowering, IntentLowering::Unresolved);
        assert_eq!(intents[1].name, "playback");
        let actions = match &intents[1].lowering {
            IntentLowering::MediaSessionTransport(actions) => actions,
            other => panic!("expected transport, got {other:?}"),
        };
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].callback, "onPlay");
        assert_eq!(actions[1].callback, "onPause");

        // Permission present exactly when transport is declared.
        let permissions = plan.permission_entries();
        assert_eq!(permissions, vec![MEDIA_SESSION_PERMISSION]);
        assert!(plan.launch().by_voice);
    }

    #[test]
    fn no_transport_means_no_permission_entry() {
        let stripped = SCENE
            .replacen("transport: play_pause;\n        ", "", 1);
        let scene = parse(&stripped).unwrap();
        let plan = emit_fire_os_plan(&scene, &resolve(&stripped)).unwrap();
        assert!(plan.permission_entries().is_empty());
        assert!(plan
            .intents()
            .iter()
            .all(|intent| intent.lowering == IntentLowering::Unresolved));
    }

    #[test]
    fn every_transport_class_maps_to_its_callbacks() {
        for (class, callbacks) in [
            (TransportClass::SkipNext, vec!["onSkipToNext"]),
            (TransportClass::SkipPrevious, vec!["onSkipToPrevious"]),
            (TransportClass::SeekForward, vec!["onSeekTo+offset"]),
            (TransportClass::SeekBackward, vec!["onSeekTo-offset"]),
            (TransportClass::Restart, vec!["onSeekTo:0"]),
        ] {
            let actions: Vec<&str> =
                transport_actions(class).iter().map(|action| action.callback).collect();
            assert_eq!(actions, callbacks, "{class:?}");
        }
    }

    #[test]
    fn plan_json_is_deterministic_across_declaration_order() {
        // Same intents in swapped declaration order — transport stays with
        // `playback`, authored explicitly rather than renamed.
        let swapped = "\
@ontology app {
    @button play { position: (100, 100, 0) extent: (200, 60, 0) }
    @button search { position: (400, 100, 0) extent: (200, 60, 0) }
    @interaction toggle {
        trigger: button.play.activate;
        class: D1;
        state { }
        input { }
    }
    @interaction find {
        trigger: button.search.activate;
        class: D1;
        state { }
        input { }
    }
    @intent catalogue {
        action: find;
        voice { utterance: \"Find {query}\"; }
        fallback { remote: search; }
    }
    @intent playback {
        action: toggle;
        transport: play_pause;
        voice { utterance: \"Pause playback\"; confirmation: true; }
        fallback { remote: play; }
    }
}
";
        let a = emit_fire_os_plan(&parse(SCENE).unwrap(), &resolve(SCENE)).unwrap();
        let b = emit_fire_os_plan(&parse(swapped).unwrap(), &resolve(swapped)).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_json(), b.to_json());
    }

    #[test]
    fn plan_json_shape_is_pinned() {
        let resolved = resolve(SCENE);
        let json = emit_fire_os_plan(&parse(SCENE).unwrap(), &resolved)
            .unwrap()
            .to_json();
        assert!(
            json.starts_with(
                "{\"contract\":\"QUITASOL-FIREOS-PLAN/1-draft\",\"profile\":\"quitasol.intent.v0\",\"lowering_tier\":\"unresolved\",\"anchor\":\"docs/quitasol/q0-capability-audit.md\",\"launch\":{\"by_voice\":true}"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "\"permissions\":[\"com.amazon.permission.media.session.voicecommandcontrol\"]"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "\"lowering\":{\"media_session_transport\":[{\"callback\":\"onPlay\",\"keyword\":\"play_pause\"},{\"callback\":\"onPause\",\"keyword\":\"play_pause\"}]}"
            ),
            "{json}"
        );
        assert!(json.contains("\"lowering\":\"unresolved\""), "{json}");
    }

    #[test]
    fn manifest_scene_mismatch_is_rejected_fail_closed() {
        // Resolve one scene, then emit against a scene with a different
        // intent set: QS-E2004, never a partial plan.
        let resolved = resolve(SCENE);
        let other = SCENE.replacen("@intent playback {", "@intent other {", 1);
        let error = emit_fire_os_plan(&parse(&other).unwrap(), &resolved).unwrap_err();
        assert_eq!(error.code, "QS-E2004");
        assert!(error.name.contains("no Q1 fallback resolution"), "{error}");
    }

    #[test]
    fn zero_intents_emit_an_empty_plan_with_launch_only() {
        let scene_text = "@ontology app { @button go { position: (0, 0, 0) } @interaction a { trigger: button.go.activate; class: D1; state { } input { } } }";
        let resolved = resolve_manifest(
            &parse(scene_text).unwrap(),
            "oem.generic.tv/1.0.0",
            800.0,
            600.0,
        )
        .unwrap();
        let plan = emit_fire_os_plan(&parse(scene_text).unwrap(), &resolved).unwrap();
        assert!(plan.intents().is_empty());
        assert!(plan.permission_entries().is_empty());
        assert!(plan.launch().by_voice);
        assert_eq!(
            plan.to_json(),
            "{\"contract\":\"QUITASOL-FIREOS-PLAN/1-draft\",\"profile\":\"quitasol.intent.v0\",\"lowering_tier\":\"unresolved\",\"anchor\":\"docs/quitasol/q0-capability-audit.md\",\"launch\":{\"by_voice\":true},\"permissions\":[],\"intents\":[]}"
        );
    }
}
