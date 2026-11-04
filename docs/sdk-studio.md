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

`serve` validates the scene, then opens a local browser editor and visual
preview at `http://127.0.0.1:8765` (or the optional port argument). It binds
to IPv4 loopback only. The editor starts with the file's current source;
**Update preview** sends edits to the local process for validation and
in-memory rendering. The geometry inspector lets you select a scene node and
edit its X/Y position and width/height in points or parent fractions. It
updates only the selected node's geometry properties in the source editor;
the scene is reparsed when you update the preview. You can also drag a node
in the preview canvas; the drag updates its position fields and applies the
same guarded in-memory source edit. The keyboard-operable inspector remains
available for precise edits. **Download source** saves a copy through the
browser.
The Studio server never writes edits to the scene file. Preview requests are
limited to 128 KiB and same-origin requests. Stop the server with Ctrl-C.

The geometry inspector is an initial visual editing aid, not a full IDE. It
supports selecting nodes from the scene list and editing their geometry; it
does not provide automatic file save, live reload, collaboration, or
platform-native rendering. If a source construct cannot be
matched safely to the selected node, or the source text has been edited since
the inspector loaded, it leaves the source unchanged and asks you to reload
the editor before applying geometry.

The live preview launches the packaged `goya-scene` host as a separate
process. This keeps the editor workflow usable without embedding platform
windowing code in the SDK. The configured text editor remains available for
direct file editing; `serve` provides a bounded browser editing session.

## Reproducible local distribution

Run `scripts/package-local.sh [output-directory]` on Linux. It builds release
binaries, stages the Studio starter, scene host, docs, and licenses, then
creates a sorted tar archive with fixed ownership and timestamp metadata and
a SHA-256 sidecar. Set `SOURCE_DATE_EPOCH` to choose another fixed timestamp.
Pass `--signing-key ED25519_PRIVATE_KEY.pem` to create a detached signature
over the archive. At install time, `--trusted-key ED25519_PUBLIC_KEY.pem`
verifies that signature against a public key supplied independently by the
operator. SHA-256 alone checks corruption, not publisher authenticity. The
script does not publish or upload artifacts.

The SDK is local workspace source; the tar package is a runnable native
bundle, not a crates.io publication. The local detached-signature mechanism
is implemented; publisher key custody, trusted-key distribution, publisher
identity, and release publication remain release-operator actions.
