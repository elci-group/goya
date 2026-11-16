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
Generated file contents and paths are deterministic for the same source; no
timestamps, generated identifiers, or machine-specific paths are included.

The project intentionally has no Gradle wrapper or dependency lock. It is a
starter project, not a reproducible Android toolchain claim. Building it needs
an installed Gradle, JDK, Android SDK, and network/cache availability for the
pinned plugins. Project generation does not run Gradle, create an APK, or
establish Android compatibility or device behavior. Aragon's Android build
and qualification gates therefore remain blocked until those checks are
performed and recorded.

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
