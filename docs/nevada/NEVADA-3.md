# NEVADA-3 Android TV project compiler

Generate a TV project and build its unsigned release APK and AAB with the [locked Aragon toolchain](../aragon/android-toolchain-lock.md).

```sh
cargo run --locked --offline -p goya-sdk --bin aragon-android-project -- \
  fixtures/nevada/counter.gscene /tmp/nevada-counter \
  --tv --profile oem.generic.tv/1.0.0

cd /tmp/nevada-counter
./gradlew --offline --dependency-verification strict assembleRelease bundleRelease
```

Gradle writes `app/build/outputs/apk/release/app-release-unsigned.apk` and `app/build/outputs/bundle/release/app-release.aab`. The project uses Aragon's locked toolchain and `dev.goya.preview` application ID. Its Leanback launcher includes an xhdpi banner and a density-independent icon. The manifest marks the touchscreen feature optional. [Android TV app requirements](https://developer.android.com/training/tv/get-started/create)

## Supported scene and runtime

Nevada accepts a subset of GSSH/3 with at least one editable field or button bound to an admitted D1 interaction. The scene adapter preserves Aragon's depth-first cell IDs and computes geometry at 960 × 540 logical dp. Every supported field and button is essential. The topmost, then leftmost control receives initial focus. The generated runtime fits this viewport uniformly inside the display area and follows the verified directional graph. D-pad select invokes the original D1 action for a button and requests the enabled Android input method for an editable field.

The scene compiler rejects scroll viewports, D2, unsupported gesture declarations, and unbound or unreachable controls. Draft Quitasol intent grammar is outside this path. Optional OEM capabilities can be modeled by the typed declaration, policy, and runtime observation APIs. The generated Android project uses the baseline D-pad and system keyboard routes. The Kotlin bridge checks keyboard availability on activation. The Rust runtime recheck API accepts actual observations from a device adapter and requires the chosen emitted routes to remain usable.

## Conformance evidence

Gate `GY-NV-GATE-11` owns the Android TV compiler's static verification step at Emperor stage 11. Project generation requires an exact registered OEM profile, reachable essential focus controls inside the viewport, and a wired remote route for each admitted interaction. The gate refuses without creating the output directory on a conformance failure. Existing board- or programme-specific gates keep their original identifiers.

The phase-2 `orchestrate::execute` entry point continues to evaluate caller-supplied plans under its phase-2 contract. Use the SDK compiler or the CLI above for a source-bound gate result and a Gradle project.

`nevada-conformance.txt` records the static verdict, exact profile, initial focus, source SHA-256, and the per-interaction authored binding, source byte range, and wired remote route. `aragon-source-map.json` ties generated scene bytes to the same admitted source. Runtime conformance is recorded as blocked until execution on Android TV. Signing, real hardware census, and OEM input qualification are release follow-up work.

## Fixture build evidence

On 20 November 2026 the locked toolchain emitted both unsigned release archives for `fixtures/nevada/counter.gscene` with strict dependency verification and Gradle offline. ZIP integrity checks passed for both. APK badging confirmed the Leanback launch activity, required Leanback feature, optional touchscreen, launcher icon, and banner.

| Artifact | SHA-256 |
| --- | --- |
| APK | `7fa512bfe6a53f71309b7a4399b933d7f8a3785390bb411c33a7a9fb73311304` |
| AAB | `796241d645b10860cb981ab43965ac5433ecca0303bee8e6a75559dd2862d241` |
