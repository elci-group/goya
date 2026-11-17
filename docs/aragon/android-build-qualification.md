# Aragon Android build qualification

The Android IME library has a local Gradle build at
[`platform/android/gradle`](../../platform/android/gradle/README.md). It
requires JDK 17 or newer and Android SDK Platform 35. A device or emulator is
not required for compilation or its JVM adapter checks.

Run the inspection-only preflight from the repository root:

```sh
sh scripts/aragon-android-preflight.sh
```

The report is stable and contains no timestamp. It checks for Java 17 or
newer, `javac`, and the actual `platforms/android-35/android.jar`. An empty
`ANDROID_SDK_ROOT` falls back to `ANDROID_HOME`, then standard SDK locations
are checked. It exits `0` when those checked prerequisites are present and
`1` when one is missing or misconfigured. A zero exit status does **not** mean
the project has built: Gradle/AGP dependency resolution, Android Build Tools
availability, network/cache state, and compilation are not checked. The
command does not install packages or fetch Gradle. ADB/device detection is
reported separately because a connected device is not a build prerequisite.

## Current environment finding

At the time this qualification was recorded, this environment had no `java`,
`javac`, `adb`, `ANDROID_HOME`, or `ANDROID_SDK_ROOT` available, and no SDK was found in
the standard checked locations (`~/Android/Sdk`, `/usr/lib/android-sdk`, or
`/opt/android-sdk`). Therefore the Android build is **blocked pending toolchain
installation/configuration**. No Android device or emulator qualification was
performed.

On 17 November 2026 a user-local Temurin JDK 21.0.13+9 was added under
`~/.local/opt` (not on the default `PATH`). With `JAVA_HOME` set to it, the
preflight's JDK checks can pass; the Android SDK and `adb` are still absent, so
the preflight still reports missing prerequisites and the build stays blocked.

On 17 November 2026 the Android SDK packages in
[`android-toolchain-lock.md`](android-toolchain-lock.md) were installed under
`~/.local/opt/android-sdk`. With `JAVA_HOME` and `ANDROID_HOME` set to the
locked JDK and that SDK, the preflight reports all checked prerequisites
present (exit 0). `adb` is installed in `platform-tools` but is not on the
default `PATH`, and no device is attached. Generated Aragon projects are
built by `scripts/check-android-apk-build.sh`, described in the lock document.

After the preflight reports ready, run the project checks from
`platform/android/gradle`:

```sh
./gradlew check
```
