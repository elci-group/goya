# Goya Android IME library build

This standalone Gradle project compiles the Kotlin sources already maintained
under `../ime/src/main` and `../ime/src/androidMain` as an Android library.
It also compiles the existing JVM checks under `../ime/src/test` and runs
`GoyaImeAdapterTest` as part of `check`.

## Requirements

- JDK 17 or newer (AGP 8.7.3 requires JDK 17; JDK 21 is supported).
- Android SDK Platform 35. Set `ANDROID_HOME` or `ANDROID_SDK_ROOT` to the SDK
  root, or install the SDK in the standard location recognized by Gradle.
- Android Build Tools are resolved by AGP; no emulator or system image is
  needed for compilation or the JVM checks.

## Build and JVM checks

From this directory, run:

```sh
./gradlew check
```

This compiles the Android library and runs the adapter's plain JVM test entry
point without starting an emulator. To run just those checks, use
`./gradlew imeJvmTest`. To produce the library AAR, use
`./gradlew assembleRelease`.

Gradle 8.9, Android Gradle Plugin 8.7.3, Kotlin Android plugin 2.0.21, and
`compileSdk` 35 are pinned in this project so the toolchain and Android API
surface are explicit.
