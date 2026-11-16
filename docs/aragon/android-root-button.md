# Aragon Android root-button D1 source preview

The SDK exposes `emit_android_root_button_preview` and its source-mapped
variant for a deliberately small source subset: one root `@button`, a
nonempty static text label, and exactly one valid D1 interaction bound to
that button. The interaction must have no declared inputs. D2 interactions,
extra actions, children, layout declarations, styling, missing source origins,
and invalid button/action bindings are rejected.

Before emitting Kotlin, the SDK strictly admits the source, lowers the scene
through canonical GY-IR, and compiles the D1 declaration with Goya's D1
compiler. It verifies that the lowered root is a button, its authored button
and action IDs match the node table, and the compiled D1 trigger is the same
IR `ActionId` as the button's activation. The callback receives the authored
interaction ID, which can be passed to the host-owned
`goya_sdk::D1Application::dispatch` method. The mapped result carries the
exact source digest and interaction declaration byte range, plus the
generated callback literal range.

This is Kotlin source generation only. It does not create an Android project,
choose initial state, persist or restore state, prove layout or accessibility
equivalence, or establish Android compilation, emulator, or device behavior.
The optional `aragon-android-project --root-button` mode packages this preview
with the standalone Kotlin D1 runtime and requires explicit `--initial`
arguments for every state key. It wires the callback to that runtime but does
not render state changes or persist state. Aragon release readiness remains
blocked pending the existing build and platform qualification gates.
