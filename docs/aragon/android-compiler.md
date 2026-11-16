# Aragon Android static-text compiler CLI

`aragon-androidc` is a standalone command around the SDK's experimental
static-text preview emitter. It reads a `.gscene` source file and writes
generated Kotlin:

```sh
cargo run --offline -p goya-sdk --bin aragon-androidc -- \
  scene.gscene Preview.kt [KotlinClassName]
```

The optional class name defaults to `GoyaStaticTextPreview`. The CLI writes
output only after the source has passed the SDK's strict admission and
static-text subset checks. Invalid, unsupported, or oversized scenes return a
nonzero status without creating an output file. Output creation is exclusive:
an existing path is rejected, which also prevents overwriting the input when
both arguments name the same file. Usage errors return status 2; source
admission and filesystem errors return status 1.

The accepted scene must contain one authored-ID `@cell` with static text. The
preview rejects children, interactions, buttons, inputs, layout, and styling
outside the SDK emitter's supported text size and fill properties. Text is
limited to 16 KiB of authored UTF-8. This CLI produces preview Kotlin only; it
does not build an Android app or qualify behavior on Android devices.
