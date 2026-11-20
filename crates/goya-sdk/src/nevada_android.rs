//! Android TV project files for a statically verified Nevada scene.

use crate::android_scene::{compile_tv_scene, AndroidScene, AndroidSceneError, AndroidSceneReason};
use crate::kotlin::KotlinFile;
use goya_nevada::gate::VerifiedScene;
use std::fmt::Write;

/// Aragon sources and launcher resources for the Nevada project writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NevadaAndroid {
    /// The Aragon sources and source map, including the Nevada remote bridge.
    pub scene: AndroidScene,
    pub manifest: &'static str,
    pub banner: &'static [u8],
    pub icon: &'static str,
    pub activity: KotlinFile,
}

const MANIFEST: &str = r#"<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-feature android:name="android.software.leanback" android:required="true" />
    <uses-feature android:name="android.hardware.touchscreen" android:required="false" />
    <application android:theme="@android:style/Theme.Material.Light.NoActionBar"
                 android:label="Nevada" android:banner="@drawable/nevada_banner"
                 android:icon="@drawable/nevada_icon">
        <activity android:name="dev.goya.preview.MainActivity" android:exported="true"
                  android:windowSoftInputMode="stateAlwaysHidden|adjustNothing">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LEANBACK_LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#;

const BANNER: &[u8] = include_bytes!("../android-template/nevada_banner.png");

const ICON: &str = r##"<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="80dp" android:height="80dp" android:viewportWidth="160"
    android:viewportHeight="160">
    <path android:fillColor="#161B29" android:pathData="M0,0 H160 V160 H0 Z" />
    <path android:fillColor="#FBD780"
        android:pathData="M43,115 V45 H58 L102,91 V45 H117 V115 H102 L58,69 V115 Z" />
</vector>
"##;

const ACTIVITY: &str = r#"package dev.goya.preview

import android.app.Activity
import android.os.Bundle
import android.widget.Toast

class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(GoyaSceneView(this) { message ->
            Toast.makeText(this, message, Toast.LENGTH_LONG).show()
        })
    }
}
"#;

pub(crate) fn emit_nevada_android(
    source: &str,
    verified: &VerifiedScene,
) -> Result<NevadaAndroid, AndroidSceneError> {
    let mut scene = compile_tv_scene(source)?;
    let viewport = verified.viewport();
    if viewport.x != 0
        || viewport.y != 0
        || viewport.w <= 0
        || viewport.h <= 0
        || verified
            .graph()
            .iter()
            .any(|row| row.id as usize >= scene.cell_ids.len())
    {
        return Err(AndroidSceneError {
            reason: AndroidSceneReason::Unsupported("a focus receipt that does not fit the scene"),
            origin: None,
            range: goya_lang::SourceRange {
                start: 0,
                end: source.len(),
            },
        });
    }

    let mut remote = REMOTE.to_owned();
    write!(
        remote,
        "\n    companion object {{\n        const val WIDTH_DP: Double = {}L / 1000.0\n        const val HEIGHT_DP: Double = {}L / 1000.0\n        private const val ENTRY = {}\n",
        viewport.w,
        viewport.h,
        verified.initial_focus()
    )
    .unwrap();
    // Keep each generated JVM method bounded for scenes near the Aragon
    // cell limit. Successor slots follow Direction::ALL.
    writeln!(
        remote,
        "        private val graph: Map<Int, IntArray> = buildMap {{"
    )
    .unwrap();
    let chunks = verified.graph().chunks(32);
    for index in 0..chunks.len() {
        writeln!(remote, "            addRows{index}(this)").unwrap();
    }
    remote.push_str("        }\n");
    for (index, rows) in chunks.enumerate() {
        writeln!(
            remote,
            "        private fun addRows{index}(rows: MutableMap<Int, IntArray>) {{"
        )
        .unwrap();
        for row in rows {
            let successors = row.successors.map(|id| id.map_or(-1, i64::from));
            let [up, down, left, right] = successors;
            writeln!(
                remote,
                "            rows[{}] = intArrayOf({up}, {down}, {left}, {right})",
                row.id
            )
            .unwrap();
        }
        remote.push_str("        }\n");
    }
    remote.push_str("    }\n}\n");
    scene.files.push(KotlinFile {
        name: "NevadaRemote.kt".to_owned(),
        text: remote,
    });
    Ok(NevadaAndroid {
        scene,
        manifest: MANIFEST,
        banner: BANNER,
        icon: ICON,
        activity: KotlinFile {
            name: "MainActivity.kt".to_owned(),
            text: ACTIVITY.to_owned(),
        },
    })
}

const REMOTE: &str = r#"package dev.goya.preview

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.LayerDrawable
import android.graphics.drawable.StateListDrawable
import android.os.Build
import android.view.KeyEvent
import android.view.View
import android.view.WindowInsets
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import kotlin.math.roundToInt

/** Remote navigation for the same viewport and successor graph as the compiler. */
internal class NevadaRemote(
    private val context: Context,
    private val report: (String) -> Unit,
    private val control: (Int) -> View?,
) {
    private var controls: Map<Int, View> = emptyMap()
    private var entered = false
    private var pressedSelect: View? = null
    private var scale = 0.0
    var offsetX = 0
        private set
    var offsetY = 0
        private set

    fun resize(width: Int, height: Int) {
        // Letterbox to preserve the verified focus geometry on different TVs.
        scale = minOf(width / WIDTH_DP, height / HEIGHT_DP)
        offsetX = ((width - WIDTH_DP * scale) / 2).roundToInt()
        offsetY = ((height - HEIGHT_DP * scale) / 2).roundToInt()
    }

    fun scale(mp: Long): Double = mp.toDouble() * scale / 1000.0

    fun attach() {
        controls = graph.keys.associateWith { index ->
            checkNotNull(control(index)).apply {
                isFocusableInTouchMode = true
                val pixel = context.resources.displayMetrics.density.roundToInt().coerceAtLeast(1)
                foreground = StateListDrawable().apply {
                    // White on dark and black on light, even for custom cell fills.
                    val outer = GradientDrawable().apply { setStroke(pixel * 4, Color.BLACK) }
                    val inner = GradientDrawable().apply { setStroke(pixel * 2, Color.WHITE) }
                    val ring = LayerDrawable(arrayOf(outer, inner)).apply {
                        setLayerInset(1, pixel, pixel, pixel, pixel)
                    }
                    addState(intArrayOf(android.R.attr.state_focused), ring)
                    addState(intArrayOf(), GradientDrawable())
                }
            }
        }
    }

    fun restoreFocus(focused: View?) {
        if (!entered || focused !in controls.values) {
            entered = controls.getValue(ENTRY).requestFocus()
        }
    }

    fun dispatch(event: KeyEvent, focused: View?): Boolean {
        val direction = when (event.keyCode) {
            KeyEvent.KEYCODE_DPAD_UP -> 0
            KeyEvent.KEYCODE_DPAD_DOWN -> 1
            KeyEvent.KEYCODE_DPAD_LEFT -> 2
            KeyEvent.KEYCODE_DPAD_RIGHT -> 3
            else -> null
        }
        val select = event.keyCode == KeyEvent.KEYCODE_DPAD_CENTER ||
            event.keyCode == KeyEvent.KEYCODE_ENTER
        if (direction == null && !select) return false

        // When the system keyboard is open, let it own the remote keys.
        if (focused is EditText && Build.VERSION.SDK_INT >= 30 &&
            focused.rootWindowInsets?.isVisible(WindowInsets.Type.ime()) == true) {
            pressedSelect = null
            return false
        }

        val current = controls.entries.firstOrNull { it.value === focused }?.key
        if (current == null) {
            pressedSelect = null
            if (event.action == KeyEvent.ACTION_DOWN) restoreFocus(focused)
            return true
        }
        if (direction != null) {
            if (event.action == KeyEvent.ACTION_DOWN) {
                pressedSelect = null
                val next = graph.getValue(current)[direction]
                if (next >= 0) controls.getValue(next).requestFocus()
            }
        } else if (event.action == KeyEvent.ACTION_DOWN && event.repeatCount == 0) {
            pressedSelect = focused
        } else if (event.action == KeyEvent.ACTION_UP) {
            val activate = !event.isCanceled && pressedSelect === focused
            pressedSelect = null
            if (activate) {
                val target = controls.getValue(current)
                if (target is EditText) showKeyboard(target) else target.performClick()
            }
        }
        return true
    }

    @Suppress("DEPRECATION")
    private fun showKeyboard(field: EditText) {
        val ime = context.getSystemService(InputMethodManager::class.java)
        if (ime == null || ime.enabledInputMethodList.isEmpty() ||
            !ime.showSoftInput(field, InputMethodManager.SHOW_IMPLICIT)) {
            report("The TV keyboard is unavailable.")
        }
    }
"#;
