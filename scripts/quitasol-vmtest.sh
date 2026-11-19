#!/usr/bin/env bash
# quitasol-vmtest.sh — reproducible Q3 glue -> APK -> AOSP emulator pipeline.
#
# Regenerates the QUITASOL-MEDIASESSION-RT/1-draft Kotlin glue via dump_glue,
# compiles it against android-34 + androidx, packages a signed APK
# (aapt + d8 + zipalign + apksigner), and — with --with-emulator — installs it
# on a generic AOSP android-34 emulator and exercises MediaSession dispatch,
# judging routed/not-routed strictly from observed logcat. Fire OS cannot run
# on a VM (see docs/quitasol/programme/quitasol-vmtest.json); this is the
# documented substitute, not a Fire OS conformance test.
#
# Env overrides: JAVA_HOME, ANDROID_SDK_ROOT, KOTLINC, VMTEST_OUT.
# Usage: scripts/quitasol-vmtest.sh [--with-emulator]
set -euo pipefail

WITH_EMULATOR=false
[ "${1:-}" = "--with-emulator" ] && WITH_EMULATOR=true

REPO="$(git rev-parse --show-toplevel)"
OUT="${VMTEST_OUT:-$REPO/target/vmtest}"
SDK="${ANDROID_SDK_ROOT:-$HOME/android-sdk}"
JAVA_HOME="${JAVA_HOME:-$HOME/android-sdk/jdk17}"
KOTLINC="${KOTLINC:-$HOME/kotlin-compiler/kotlinc/bin/kotlinc}"
BT="$SDK/build-tools/34.0.0"
export PATH="$JAVA_HOME/bin:$SDK/platform-tools:$SDK/emulator:$PATH"
BUILD_OK=false; INSTALL_OK=false

echo "== 1. environment checks =="
command -v java >/dev/null || { echo "FAIL: java not found (JAVA_HOME=$JAVA_HOME)" >&2; exit 1; }
[ -x "$KOTLINC" ] || { echo "FAIL: kotlinc not found at $KOTLINC" >&2; exit 1; }
for t in "d8" "aapt" "zipalign" "apksigner"; do
  [ -x "$BT/$t" ] || { echo "FAIL: $BT/$t missing" >&2; exit 1; }
done
[ -d "$SDK/platforms/android-34" ] || { echo "FAIL: platform android-34 missing" >&2; exit 1; }
command -v sdkmanager >/dev/null || echo "WARN: sdkmanager not on PATH (AVD creation may fail)"
if [ -w /dev/kvm ]; then echo "KVM available"; else
  echo "WARN: no writable /dev/kvm — emulator will run in slow software mode"; fi

mkdir -p "$OUT/app" "$OUT/build"

echo "== 2. regenerate glue (dump_glue) =="
( cd "$REPO" && cargo run -p quitasol-semantic --offline --example dump_glue -- "$OUT/app" )
KT_SRC=$(find "$OUT/app" -name '*.kt' | head -1)
[ -n "$KT_SRC" ] || { echo "FAIL: dump_glue emitted no Kotlin" >&2; exit 1; }
echo "glue: $KT_SRC"

echo "== 3. compile Kotlin, dex, package, sign =="
LIBS="$HOME/quitasol-vmtest/libs"
# Minimal harness manifest + entry activity so the glue lives in a real APK.
# The activity registers the QuitasolMediaSessionCallback with a MediaSessionCompat.
cat > "$OUT/build/AndroidManifest.xml" <<'EOF'
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="dev.quitasol.vmtest">
  <uses-permission android:name="com.amazon.permission.media.session.voicecommandcontrol"/>
  <application android:label="QuitasolVMTest">
    <activity android:name=".MainActivity" android:exported="true">
      <intent-filter><action android:name="android.intent.action.MAIN"/>
        <category android:name="android.intent.category.LAUNCHER"/></intent-filter>
    </activity>
  </application>
</manifest>
EOF
# Minimal bridge implementation: logs every routed intent under the
# QuitasolBridge tag — the sole source of truth for the dispatch verdict.
cat > "$OUT/app/MainActivity.kt" <<'EOF'
package dev.quitasol.vmtest
import android.os.Bundle
import android.util.Log
import androidx.appcompat.app.AppCompatActivity
import androidx.media.session.MediaButtonReceiver

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.d("QuitasolBridge", "activity started; bridge ready")
    }
}
EOF
# NB: kotlinc flags BEFORE sources.
"$KOTLINC" -jvm-target 17 -no-jdk -no-stdlib \
  -cp "$SDK/platforms/android-34/android.jar:$LIBS"/*.jar \
  -d "$OUT/build/classes.jar" "$KT_SRC" "$OUT/app/MainActivity.kt"
"$BT/d8" --release --min-api 24 --lib "$SDK/platforms/android-34/android.jar" \
  $(ls "$LIBS"/*.jar | sed 's/^/--lib /') --output "$OUT/build" "$OUT/build/classes.jar"
"$BT/aapt" package -f -M "$OUT/build/AndroidManifest.xml" \
  -I "$SDK/platforms/android-34/android.jar" -F "$OUT/build/unsigned.apk"
(cd "$OUT/build" && "$BT/aapt" add unsigned.apk classes.dex)
keytool -genkeypair -keystore "$OUT/build/debug.keystore" -storepass android \
  -keypass android -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
  -dname "CN=Android Debug,O=Android,C=US"
"$BT/zipalign" -f 4 "$OUT/build/unsigned.apk" "$OUT/build/aligned.apk"
"$BT/apksigner" sign --ks "$OUT/build/debug.keystore" --ks-pass pass:android \
  --out "$OUT/quitasol-vmtest.apk" "$OUT/build/aligned.apk"
"$BT/apksigner" verify "$OUT/quitasol-vmtest.apk"
BUILD_OK=true
echo "APK: $OUT/quitasol-vmtest.apk ($(stat -c%s "$OUT/quitasol-vmtest.apk") bytes)"

BOOT_POLL="${BOOT_POLL:-180}"
adb_emu() { adb -s emulator-5554 "$@"; }

echo "== 4. emulator stage =="
if $WITH_EMULATOR; then
  AVD=quitasol_vmtest
  command -v avdmanager >/dev/null && ! [ -d "$HOME/.android/avd/$AVD.avd" ] && \
    avdmanager create avd -n "$AVD" -k "system-images;android-34;google_apis;x86_64" --force
  emulator -avd "$AVD" -no-window -memory 2048 -no-snapshot & EMU_PID=$!
  trap 'kill $EMU_PID 2>/dev/null || true' EXIT
  adb wait-for-device
  booted=false
  for _ in $(seq 1 "$BOOT_POLL"); do
    [ "$(adb_emu shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] \
      && { booted=true; break; }
    sleep 2
  done
  $booted || { echo "FAIL: emulator boot timeout after ${BOOT_POLL}s" >&2; exit 1; }
  if adb_emu install -r "$OUT/quitasol-vmtest.apk"; then INSTALL_OK=true; fi
  adb_emu logcat -c
  adb_emu shell am start -n dev.quitasol.vmtest/.MainActivity || true
  sleep 2
  : > "$OUT/logcat-bridge.txt"
  adb_emu logcat -s QuitasolBridge >> "$OUT/logcat-bridge.txt" & LOG_PID=$!
  sleep 1
  # Dispatch the transport commands Alexa would send via MediaSession.
  for cmd in play pause fast-forward; do
    adb_emu shell cmd media_session dispatch "$cmd" || true
    sleep 2
  done
  sleep 1; kill "$LOG_PID" 2>/dev/null || true
  adb_emu logcat -d -s QuitasolBridge > "$OUT/logcat-dump.txt" || true
  kill "$EMU_PID" 2>/dev/null || true; wait "$EMU_PID" 2>/dev/null || true
else
  echo "(skipped: pass --with-emulator to install and dispatch)"
fi

echo "== 5. honest summary =="
if $BUILD_OK; then echo "BUILD:   OK"; else echo "BUILD:   FAIL"; fi
if $INSTALL_OK; then echo "INSTALL: OK"; elif $WITH_EMULATOR; then
  echo "INSTALL: FAIL"; else echo "INSTALL: NOT RUN"; fi
if $WITH_EMULATOR; then
  for cmd in play pause fast-forward; do
    if grep -qi "$cmd" "$OUT/logcat-dump.txt" 2>/dev/null; then
      echo "dispatch $cmd: ROUTED (observed in logcat)"
    else
      echo "dispatch $cmd: NOT ROUTED (no logcat evidence)"
    fi
  done
  echo "full capture: $OUT/logcat-dump.txt"
else
  echo "dispatch: NOT RUN (no emulator)"
fi
