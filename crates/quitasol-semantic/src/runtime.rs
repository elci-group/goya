//! Q3 MediaSession runtime glue emission (proposed, not ratified).
//!
//! Lowers a Q2 [`FireOsPlan`] to deterministic Kotlin MediaSession glue:
//! a `QuitasolIntentBridge` interface and a
//! `QuitasolMediaSessionCallback` whose documented callbacks dispatch to
//! the app's intents. The emitted Kotlin is a candidate binding — it has
//! **never been compiled** (no Android toolchain is invoked) and no
//! on-device behaviour is claimed. Voice *navigation* is not emitted: the
//! Q0 audit records Amazon's voice scrolling & selection as back-end
//! activated per app, not self-serve.
//!
//! Fail-closed and deterministic; consumes `nevada` nothing — this module
//! depends only on the Q2 plan and the parsed scene.

use crate::fireos::{FireOsPlan, IntentLowering};
use goya_lang::intents::IntentDiagnostic;
use goya_lang::{Scene, SourceRange};

/// Contract identifier of the emitted runtime glue. Draft revision:
/// proposed, not ratified; the emitted shape may change.
pub const MEDIA_SESSION_RT_CONTRACT: &str = "QUITASOL-MEDIASESSION-RT/1-draft";

/// One callback-method binding: the MediaSession callback the glue
/// overrides and the intent it dispatches to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackBinding {
    /// e.g. `onPlay`, `onSkipToNext`, `onSeekTo` (restart guard applied).
    pub callback: &'static str,
    pub intent: String,
}

/// The deterministic Q3 output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSessionGlue {
    package: String,
    bindings: Vec<CallbackBinding>,
    unresolved: Vec<String>,
    kotlin: String,
}

impl MediaSessionGlue {
    /// Callback bindings, deterministic (plan order: sorted by intent name).
    pub fn bindings(&self) -> &[CallbackBinding] {
        &self.bindings
    }

    /// Intents with no documented self-serve lowering, sorted by name.
    /// Recorded honestly; never dropped.
    pub fn unresolved(&self) -> &[String] {
        &self.unresolved
    }

    /// The emitted Kotlin source. Pinned shape; never compiled here.
    pub fn kotlin(&self) -> &str {
        &self.kotlin
    }
}

/// Emit the MediaSession runtime glue for a scene and its Q2 plan.
///
/// Fail-closed:
/// - the plan's intent set must equal the scene's (`QS-E2004`), and
/// - no MediaSession callback method may be bound by two intents
///   (`QS-E2005`) — e.g. two `play_pause` intents would both override
///   `onPlay`; that is a compile-time refusal, never a silent last-wins.
///
/// Deterministic: bindings in plan order (intents sorted by name), Kotlin
/// text a pure function of the inputs.
pub fn emit_media_session_glue(
    scene: &Scene,
    plan: &FireOsPlan,
) -> Result<MediaSessionGlue, IntentDiagnostic> {
    // Plan/scene agreement, both directions.
    let scene_names: std::collections::BTreeSet<&str> = scene
        .intents
        .iter()
        .map(|intent| intent.name.as_str())
        .collect();
    let plan_names: std::collections::BTreeSet<&str> = plan
        .intents()
        .iter()
        .map(|intent| intent.name.as_str())
        .collect();
    if scene_names != plan_names {
        return Err(IntentDiagnostic {
            code: "QS-E2004",
            name: "plan and scene carry different intent sets".to_owned(),
            range: SourceRange { start: 0, end: 0 },
        });
    }

    let mut bindings: Vec<CallbackBinding> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut seen_callbacks: std::collections::BTreeMap<&'static str, &str> =
        std::collections::BTreeMap::new();
    for intent in plan.intents() {
        match &intent.lowering {
            IntentLowering::MediaSessionTransport(actions) => {
                for action in actions {
                    // `onSeekTo(0)` and `onSeekTo` are the same override;
                    // normalise the restart form before the conflict check.
                    let method = action
                        .callback
                        .strip_suffix("(0)")
                        .unwrap_or(action.callback);
                    if let Some(previous) = seen_callbacks.get(method) {
                        return Err(IntentDiagnostic {
                            code: "QS-E2005",
                            name: format!(
                                "callback {method} is bound by intents {previous} and {}: MediaSession callbacks must not conflict",
                                intent.name
                            ),
                            range: SourceRange { start: 0, end: 0 },
                        });
                    }
                    seen_callbacks.insert(method, &intent.name);
                    bindings.push(CallbackBinding {
                        callback: method,
                        intent: intent.name.clone(),
                    });
                }
            }
            IntentLowering::Unresolved => unresolved.push(intent.name.clone()),
        }
    }

    let package = format!("{}.quitasol", scene.name);
    let mut kotlin = String::new();
    kotlin.push_str(
        "// GENERATED by quitasol-semantic — QUITASOL-MEDIASESSION-RT/1-draft. Do not edit.\n",
    );
    kotlin.push_str("// Profile: quitasol.intent.v0 | lowering tier: unresolved (candidate bindings; never compiled here).\n");
    kotlin.push_str(
        "// Anchor: docs/quitasol/q0-capability-audit.md | plan: QUITASOL-FIREOS-PLAN/1-draft\n",
    );
    kotlin.push_str(&format!("package {package}\n\n"));
    kotlin.push_str("import android.support.v4.media.session.MediaSessionCompat\n\n");
    kotlin.push_str(
        "interface QuitasolIntentBridge {\n    fun invoke(intent: String): Boolean\n}\n\n",
    );
    kotlin.push_str(
        "class QuitasolMediaSessionCallback(\n    private val bridge: QuitasolIntentBridge,\n) : MediaSessionCompat.Callback() {\n",
    );
    for binding in &bindings {
        if binding.callback == "onSeekTo" {
            kotlin.push_str(&format!(
                "    override fun onSeekTo(pos: Long) {{\n        if (pos == 0L) {{\n            bridge.invoke(\"{}\")\n        }}\n    }}\n\n",
                binding.intent
            ));
        } else {
            kotlin.push_str(&format!(
                "    override fun {}() {{\n        bridge.invoke(\"{}\")\n    }}\n\n",
                binding.callback, binding.intent
            ));
        }
    }
    kotlin.push_str("}\n");
    if !unresolved.is_empty() {
        kotlin.push_str(&format!(
            "\n// Not voice-reachable (no documented self-serve lowering; VSK closed per Q0): {}\n",
            unresolved.join(", ")
        ));
    }

    Ok(MediaSessionGlue {
        package,
        bindings,
        unresolved,
        kotlin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fireos::emit_fire_os_plan;
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

    fn glue(scene_text: &str) -> Result<MediaSessionGlue, IntentDiagnostic> {
        let scene = parse(scene_text).unwrap();
        let resolved = resolve_manifest(&scene, "oem.acme.voice-remote/1.0.0", 800.0, 600.0)
            .expect("scene resolves");
        let plan = emit_fire_os_plan(&scene, &resolved).expect("plan emits");
        emit_media_session_glue(&scene, &plan)
    }

    #[test]
    fn glue_binds_documented_callbacks_and_records_unresolved() {
        let glue = glue(SCENE).unwrap();
        assert_eq!(glue.package, "app.quitasol");
        let bindings = glue.bindings();
        assert_eq!(
            bindings,
            &[
                CallbackBinding {
                    callback: "onPlay",
                    intent: "playback".to_owned()
                },
                CallbackBinding {
                    callback: "onPause",
                    intent: "playback".to_owned()
                },
            ]
        );
        assert_eq!(glue.unresolved(), ["catalogue"]);
        assert!(glue.kotlin().contains("override fun onPlay() {"));
        assert!(glue.kotlin().contains("bridge.invoke(\"playback\")"));
        assert!(glue
            .kotlin()
            .contains("// Not voice-reachable (no documented self-serve lowering; VSK closed per Q0): catalogue"));
    }

    #[test]
    fn restart_binds_onguarded_onseekto() {
        let scene_text = SCENE.replacen("transport: play_pause;", "transport: restart;", 1);
        let glue = glue(&scene_text).unwrap();
        assert_eq!(glue.bindings().len(), 1);
        assert_eq!(glue.bindings()[0].callback, "onSeekTo");
        assert!(glue.kotlin().contains("override fun onSeekTo(pos: Long) {"));
        assert!(glue.kotlin().contains("if (pos == 0L) {"));
    }

    #[test]
    fn conflicting_callback_bindings_are_refused_fail_closed() {
        // Two play_pause intents: both would override onPlay.
        let duplicated = SCENE.replacen(
            "action: find;\n        voice {",
            "action: find;\n        transport: play_pause;\n        voice {",
            1,
        );
        let error = glue(&duplicated).unwrap_err();
        assert_eq!(error.code, "QS-E2005");
        assert!(
            error
                .name
                .contains("bound by intents catalogue and playback"),
            "{error}"
        );
    }

    #[test]
    fn plan_scene_mismatch_is_refused() {
        let scene = parse(SCENE).unwrap();
        let resolved =
            resolve_manifest(&scene, "oem.acme.voice-remote/1.0.0", 800.0, 600.0).unwrap();
        let plan = emit_fire_os_plan(&scene, &resolved).unwrap();
        // A scene whose intents were renamed: plan no longer matches.
        let renamed = SCENE.replacen("@intent playback {", "@intent other {", 1);
        let error = emit_media_session_glue(&parse(&renamed).unwrap(), &plan).unwrap_err();
        assert_eq!(error.code, "QS-E2004");
    }

    #[test]
    fn kotlin_output_is_deterministic_and_pinned() {
        let a = glue(SCENE).unwrap();
        let b = glue(SCENE).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.kotlin(), b.kotlin());
        let expected = "\
// GENERATED by quitasol-semantic — QUITASOL-MEDIASESSION-RT/1-draft. Do not edit.
// Profile: quitasol.intent.v0 | lowering tier: unresolved (candidate bindings; never compiled here).
// Anchor: docs/quitasol/q0-capability-audit.md | plan: QUITASOL-FIREOS-PLAN/1-draft
package app.quitasol

import android.support.v4.media.session.MediaSessionCompat

interface QuitasolIntentBridge {
    fun invoke(intent: String): Boolean
}

class QuitasolMediaSessionCallback(
    private val bridge: QuitasolIntentBridge,
) : MediaSessionCompat.Callback() {
    override fun onPlay() {
        bridge.invoke(\"playback\")
    }

    override fun onPause() {
        bridge.invoke(\"playback\")
    }

}

// Not voice-reachable (no documented self-serve lowering; VSK closed per Q0): catalogue
";
        assert_eq!(a.kotlin(), expected);
    }

    #[test]
    fn zero_transport_scenes_emit_an_empty_callback_class() {
        let scene_text = "@ontology app { @button go { position: (0, 0, 0) } @interaction a { trigger: button.go.activate; class: D1; state { } input { } } @intent main { action: a; voice { utterance: \"Go\"; } fallback { remote: go; } } }";
        let glue = glue(scene_text).unwrap();
        assert!(glue.bindings().is_empty());
        assert_eq!(glue.unresolved(), ["main"]);
        assert!(glue
            .kotlin()
            .contains("class QuitasolMediaSessionCallback("));
        assert!(!glue.kotlin().contains("override fun"));
    }
}
