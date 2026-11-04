# Goya SDK, Studio starter, and local package

`goya-sdk` is the Rust facade for `.gscene` consumers. It parses source,
lowers to the shared GY-IR/2 module and node table, exposes authored scene
geometry, and constructs the synchronous event runtime. Platform code still
owns window creation and presentation.

`goya-studio` is an initial command-line authoring workflow:

```sh
cargo run -p goya-studio -- new apps/my_app.gscene
cargo run -p goya-studio -- check apps/my_app.gscene
cargo run -p goya-studio -- inspect apps/my_app.gscene
EDITOR=vi cargo run -p goya-studio -- edit apps/my_app.gscene
cargo run -p goya-studio -- preview apps/my_app.gscene
cargo run -p goya-studio -- preview apps/my_app.gscene --live
```

The live preview launches the packaged `goya-scene` host as a separate
process. This keeps the editor workflow usable without embedding platform
windowing code in the SDK. It is a small Studio starter, not a full visual
IDE: editing is delegated to the user's configured text editor.

## Reproducible local distribution

Run `scripts/package-local.sh [output-directory]` on Linux. It builds release
binaries, stages the Studio starter, scene host, docs, and licenses, then
creates a sorted tar archive with fixed ownership and timestamp metadata and
a SHA-256 sidecar. Set `SOURCE_DATE_EPOCH` to choose another fixed timestamp.
The script does not publish or upload artifacts.

The SDK is local workspace source; the tar package is a runnable native
bundle, not a crates.io publication. External release signing and distribution
remain release-operator actions.
