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
///
/// VM-test findings baked in (observed on the AOSP 34 emulator, 2026-11-19):
/// - a `MediaSessionCompat` without a `PlaybackState` is NOT the media
///   button session — `cmd media_session dispatch` routes nowhere until
///   `setPlaybackState` is called (state PLAYING, actions
///   PLAY|PAUSE|FAST_FORWARD);
/// - kotlin-stdlib must be dexed into the APK (NoClassDefFoundError:
///   kotlin/jvm/internal/Intrinsics otherwise);
/// - androidx.versionedparcelable must be on the classpath — SystemUI's
///   MediaController attach crashes the process without it (real builds
///   fetch androidx.versionedparcelable:versionedparcelable; the VM test
///   stubbed it because Google Maven was unreachable from the test host).
const MAIN_APP: &str = r#"// Test harness bridge — not generated; app-side wiring of the Q3 contract.
package app.quitasol

import android.app.Activity
import android.app.Application
import android.support.v4.media.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.util.Log

class TestActivity : Activity()

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
        // Without a PlaybackState the session is never chosen as the media
        // button session on API 34 and dispatch routes nowhere.
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(
                    PlaybackStateCompat.ACTION_PLAY
                        or PlaybackStateCompat.ACTION_PAUSE
                        or PlaybackStateCompat.ACTION_FAST_FORWARD
                )
                .setState(PlaybackStateCompat.STATE_PLAYING, 0L, 1.0f)
                .build()
        )
        session.isActive = true
        Log.i("QuitasolBridge", "session_active")
    }
}
"#;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="app.quitasol">
    <!-- targetSdk-0 APKs are refused at install (INSTALL_FAILED_DEPRECATED_SDK_VERSION). -->
    <uses-sdk android:minSdkVersion="21" android:targetSdkVersion="34" />
    <!-- Fire OS MediaSession voice transport permission (Q2 plan emission).
         Unknown to stock Android; harmless to declare on AOSP images. -->
    <uses-permission android:name="com.amazon.permission.media.session.voicecommandcontrol" />
    <application
        android:name=".MainApplication"
        android:label="QuitasolVMTest"
        android:allowBackup="false">
        <!-- A launcher activity: without one the process never starts on a
             clean emulator install (nothing to monkey/am start). -->
        <activity android:name=".TestActivity" android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#;
