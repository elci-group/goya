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
cargo run -p goya-studio -- serve apps/my_app.gscene
```

`serve` validates the scene, then serves a static visual preview at
`http://127.0.0.1:8765` (or the optional port argument). It binds to IPv4
loopback only, accepts a bounded header-only `GET /` request, and does not
write or edit scene files. Stop it with Ctrl-C. The preview is a visual
snapshot when the command starts; restart it to pick up file changes. This is
an initial authoring tool, not a full IDE: it has no browser editing, live
reload, or platform-native rendering.

The live preview launches the packaged `goya-scene` host as a separate
process. This keeps the editor workflow usable without embedding platform
windowing code in the SDK. Editing is delegated to the user's configured text
editor; `serve` adds a local visual preview to that workflow.

## Reproducible local distribution

Run `scripts/package-local.sh [output-directory]` on Linux. It builds release
binaries, stages the Studio starter, scene host, docs, and licenses, then
creates a sorted tar archive with fixed ownership and timestamp metadata and
a SHA-256 sidecar. Set `SOURCE_DATE_EPOCH` to choose another fixed timestamp.
The script does not publish or upload artifacts.

The SDK is local workspace source; the tar package is a runnable native
bundle, not a crates.io publication. External release signing and distribution
remain release-operator actions.
