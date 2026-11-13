# Android clipboard transport (experimental)

This standalone Android library provides synchronous, explicitly invoked clipboard writes. It is not wired into a Goya editor or any native command route. Access defaults to denial; the host must supply a fresh policy decision for every user-initiated operation, on the main thread. Policy exceptions deny access. Do not use reads as availability probes or poll the clipboard.

Writes accept only copy or cut and place bounded Unicode text on Android's clipboard. Empty text returns `NoChange` without writing, preserving the clipboard. Writes reject malformed UTF-16 and text over the provisional 1 MiB UTF-8 limit. After access checks, reads return `UNSUPPORTED` without querying Android's clipboard until a bounded acquisition path is established. No payloads are logged.

The host must validate replacements, enforce protected-control and other policy masks, manage lifecycle/focus/revision reservations, and commit to the editor. For cut, it must reserve and validate first, export before removal, then revalidate and commit; on failed revalidation it must retain text and report the partial export. A successful write is only the observable return of Android's API, not proof of delivery or a durable clipboard. The transport does not guard other menu, IME, accessibility, or framework routes and must not be used to claim control or platform qualification.

With JDK 17+ and Android SDK 35 installed, run `../gradle/gradlew -p . check` in this directory. JVM tests cover the text validator, empty writes and disabled reads; device tests, route enforcement, policy integration and lifecycle/race tests remain necessary.
