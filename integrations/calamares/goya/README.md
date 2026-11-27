# Calamares Goya frontend

This branch starts a `.gscene` frontend for Calamares. `shell.gscene` declares the wizard header, progress sidebar, content viewport, and navigation controls. The companion Rust tool admits it through Goya's strict GSSH/4 parser and resolves a 960×600 layout as JSON. The generated `shell.layout.json` is embedded in Calamares as a Qt resource. `CalamaresWindow` uses its colours, text sizes, and proportions to build the wizard chrome, while the navigation controls call the existing `ViewManager` slots.

From this directory:

```sh
cargo run --offline -- check shell.gscene
cargo run --offline -- layout shell.gscene
```

When changing the scene, regenerate `shell.layout.json` with `cargo run --offline -- layout shell.gscene > shell.layout.json` before building Calamares. The compiled shell falls back to the upstream widgets if the embedded layout is absent or invalid.

Current Calamares modules remain Qt widgets inside the scene's content viewport. The install and partition jobs have not been exercised through a Goya UI. Each module needs its own Goya view adapter before the Qt frontend can be retired. The main window has not been compiled in this workspace because the Qt development package is absent.

The local fork currently depends on Goya's canonical parser at `../../../../crates/goya-lang`. That path is relative to this workspace and must be replaced with a pinned package or vendored dependency before distributing the fork independently.
