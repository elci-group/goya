# Offline check for the generated Android project

Run this from the repository root:

```sh
bash scripts/check-android-project-kotlin.sh
```

The check generates projects twice from fixed qualification scenes and
compares every emitted file. It also checks the expected manifest, build
files, preview/runtime classes, and launcher activity. These checks use Cargo
offline mode and make no network requests. The root-button project form
includes the standalone D1 runtime; Kotlin compilation is checked against the
local Android API stubs, not an installed Android SDK.

When Java and Kotlin 2.0.21 compiler artifacts are already available locally,
the command compiles the generated Kotlin sources against tiny Android API
stubs. This checks Kotlin syntax, types, and that the generated activity
connects to its preview/runtime classes. It does not use the Android SDK and
does not qualify Android API compatibility, Gradle, APK packaging, or runtime
behavior.
If the JVM/compiler inputs are missing, the default run reports that compile
step as skipped after passing the deterministic structure check. Use
`--require-kotlin` when a missing compiler must fail the qualification.

For strict generated D1 runtime behavior qualification, run:

```sh
bash scripts/check-android-d1-kotlin.sh
```

This executes the exact Kotlin/JVM conformance test with
`ARAGON_REQUIRE_KOTLIN=1`; a missing Java or Kotlin compiler is a failure. It
does not qualify Android SDK compilation or device behavior. On 17 November
2026 both commands passed on this host with Temurin JDK 21.0.13+9 and
kotlinc-jvm 2.0.21 selected through `JAVA_HOME` and `PATH` (see
`release-readiness.md`). No Android SDK Platform 35 is installed, so Android
compilation remains blocked pending toolchain setup. Do not report structural
generation or Kotlin/JVM results as an Android SDK or app build.
