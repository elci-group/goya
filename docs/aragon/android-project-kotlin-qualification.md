# Offline check for the generated Android project

Run this from the repository root:

```sh
bash scripts/check-android-project-kotlin.sh
```

The check generates the same project twice from a fixed static-text scene and
compares every emitted file. It also checks that the expected manifest, build
files, preview class, and launcher activity exist. These checks use Cargo
offline mode and make no network requests.

When Java and Kotlin 2.0.21 compiler artifacts are already available locally,
the command compiles both generated Kotlin source files against tiny Android
API stubs. This checks Kotlin syntax, types, and that the generated activity
connects to the preview class. It does not use the Android SDK and does not
qualify Android API compatibility, Gradle, APK packaging, or runtime behavior.
If the JVM/compiler inputs are missing, the default run reports that compile
step as skipped after passing the deterministic structure check. Use
`--require-kotlin` when a missing compiler must fail the qualification.

On the recorded host, no Java runtime is installed, so the Kotlin compilation
branch is currently unavailable. The structural generation check can still
run. This outcome must not be reported as an Android SDK or app build.
