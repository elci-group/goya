# Aragon deterministic Android preview project

`aragon-android-project` packages either the experimental static-text Kotlin
preview or an opt-in root-button D1 preview in a small Android application
project. The static-text form is:

```sh
cargo run --offline -p goya-sdk --bin aragon-android-project -- \
  scene.gscene generated-preview
```

The command accepts the same strict single-cell static text subset as
`aragon-androidc`. It emits a launcher `Activity`, the generated `TextView`
subclass, manifest, Gradle Kotlin build files, and Android/Kotlin plugin
versions pinned in the generated root build file. Source validation happens
before the destination is created. The generated file layout is checked for
duplicate or conflicting paths before output begins. The destination must not
already exist. An I/O failure while writing can leave a partially written
destination.
Generated manifests disable Android app-data backup and cleartext network
traffic for the preview and scene profiles. The Nevada TV manifest applies the
same defaults.
Generated file contents and paths are deterministic for the same source; no
timestamps, generated identifiers, or machine-specific paths are included.

The project carries the locked toolchain described in
[`android-toolchain-lock.md`](android-toolchain-lock.md): the Gradle 8.9
wrapper (`gradlew`, `gradlew.bat`, the official wrapper jar, and a
distribution pinned by SHA-256), `gradle/verification-metadata.xml` with
SHA-256 checksums for every plugin and build dependency, an explicit
`buildToolsVersion`, and `android.builder.sdkDownload=false`. These files are
copied verbatim from `crates/goya-sdk/android-template/`. With the locked JDK
and SDK packages installed, `./gradlew --dependency-verification strict
assembleRelease` builds an unsigned release APK; the first build needs network
access to fetch Gradle and the verified dependencies unless they are cached.

Project generation itself does not run Gradle or create an APK.
`scripts/check-android-apk-build.sh` builds the three qualification projects
offline, twice each, and checks the APKs against recorded hashes. No generated
APK has been installed or run, so emulator, device and platform behavior are
not established.

## Compiled scene project

`--scene` compiles a GSSH/3 scene with the Aragon scene compiler
([`android-scene.md`](android-scene.md)) instead of using a preview:

```sh
cargo run --offline -p goya-sdk --bin aragon-android-project -- \
  scene.gscene generated-scene --scene
```

It writes the generated specification, view and D1 runtime sources, a
`MainActivity` that shows failures in a toast, and `aragon-source-map.json`
at the project root. A refused construct is reported with its byte range,
and nothing is written.

## Root-button D1 project

The root-button form emits the validated root button, the standalone Kotlin D1
runtime, and a launcher activity that dispatches clicks using the authored
interaction ID:

```sh
cargo run --offline -p goya-sdk --bin aragon-android-project -- \
  scene.gscene generated-button --root-button --initial count=int:0
```

Repeat `--initial key=type:value` for every state key used by the D1 actions.
Types are `int`, `text`, and `bool`; integers must fit signed 64-bit, booleans
must be `true` or `false`, and text is taken literally after the first colon.
The generator rejects missing, extra, duplicate, or mistyped state values
before creating the output directory. Stateless actions need no `--initial`
arguments. The generated runtime checks the schema again when it is created.

This experimental local adapter keeps values in memory for the activity
instance, dispatches an empty input map because this preview subset does not
accept D1 inputs, and does not render state changes or persist state. It does
not establish Android compilation, emulator, or device behavior; the same
platform qualification limits above apply.
