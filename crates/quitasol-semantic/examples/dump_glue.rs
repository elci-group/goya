//! VM-test harness: emit the Q3 MediaSession glue for the phase fixture
//! scene and write an installable Android app skeleton around it.
//!
//! Usage: cargo run -p quitasol-semantic --example dump_glue -- <output-dir>
//!
//! The scene is the runtime-test fixture: one `play_pause` transport intent
//! (`playback`) and one unresolved intent (`catalogue`). Emitted files:
//! - `<out>/glue.kt` — the Q3-generated MediaSession callback (verbatim
//!   `emit_media_session_glue` output);
//! - `<out>/MainApplication.kt` — app-side bridge implementation wiring the
//!   glue to a live `MediaSessionCompat`;
//! - `<out>/AndroidManifest.xml` — minimal manifest with the Fire OS
//!   MediaSession voice permission declared.

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

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: dump_glue <output-dir>");
    let scene = parse(SCENE).expect("fixture scene parses");
    let resolved = quitasol_semantic::resolve_manifest(
        &scene,
        "oem.acme.voice-remote/1.0.0",
        800.0,
        600.0,
    )
    .expect("fixture resolves");
    let plan =
        quitasol_semantic::emit_fire_os_plan(&scene, &resolved).expect("plan emits");
    let glue = quitasol_semantic::emit_media_session_glue(&scene, &plan)
        .expect("glue emits");
    std::fs::create_dir_all(&out).expect("create output dir");
    std::fs::write(format!("{out}/glue.kt"), glue.kotlin()).expect("write glue.kt");
    std::fs::write(
        format!("{out}/MainApplication.kt"),
        MAIN_APP,
    )
    .expect("write MainApplication.kt");
    std::fs::write(format!("{out}/AndroidManifest.xml"), MANIFEST)
        .expect("write AndroidManifest.xml");
    eprintln!(
        "wrote {} bindings, {} unresolved intents to {out}",
        glue.bindings().len(),
        glue.unresolved().len()
    );
}

/// App-side bridge: logs every dispatched intent (visible via logcat, tagged
/// `QuitasolBridge`) and records the last intent for adb verification.
const MAIN_APP: &str = r#"// Test harness bridge — not generated; app-side wiring of the Q3 contract.
package app.quitasol

import android.app.Application
import android.content.Context
import android.support.v4.media.session.MediaSessionCompat
import android.util.Log

class QuitasolTestBridge : QuitasolIntentBridge {
    override fun invoke(intent: String): Boolean {
        Log.i("QuitasolBridge", "intent_dispatched:$intent")
        MainApplication.lastDispatchedIntent = intent
        return true
    }
}

class MainApplication : Application() {
    companion object {
        @JvmStatic
        var lastDispatchedIntent: String? = null
    }

    override fun onCreate() {
        super.onCreate()
        val session = MediaSessionCompat(this, "QuitasolSession")
        session.setCallback(QuitasolMediaSessionCallback(QuitasolTestBridge()))
        session.isActive = true
        Log.i("QuitasolBridge", "session_active")
    }
}
"#;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="app.quitasol">
    <!-- Fire OS MediaSession voice transport permission (Q2 plan emission).
         Unknown to stock Android; harmless to declare on AOSP images. -->
    <uses-permission android:name="com.amazon.permission.media.session.voicecommandcontrol" />
    <application
        android:name=".MainApplication"
        android:label="QuitasolVMTest"
        android:allowBackup="false">
    </application>
</manifest>
"#;
