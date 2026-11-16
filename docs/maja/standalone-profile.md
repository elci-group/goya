# Experimental standalone profile

`maja.web.local.v1` currently admits only one strictly parsed root `@cell` with an explicit authored `id` and static `text`. All other authored properties, children, controls, interactions and effects are rejected with a source-linked diagnostic. U+0000 and carriage returns in text, and attribute-normalized control characters in IDs, are rejected. No implicit state, bindings, persistence, server requests or capabilities are introduced. The legacy `maja.web.v1` remains fail-closed.

```
maja inspect app.gscene
maja check app.gscene --target web --profile maja.web.local.v1
maja build app.gscene --profile maja.web.local.v1 --out new-output-directory
```

The output is an experimental static package: `index.html`, `app.css`, and `manifest.json`. The manifest contains a source SHA-256, text-origin byte range, compiler/profile identity, and SHA-256 values for HTML and CSS. It is an integrity aid, not an authenticity guarantee. Build output is deterministic for identical source and compiler configuration, contains no timestamps or local paths, and refuses to replace an existing output directory. Parent directory must already exist. `--release` is rejected because this profile is not release-qualified.

No browser application logic, button, input, state binding, navigation, local storage, offline caching, optional remote services, machine-readable diagnostics, compiler service protocol or browser source map is implemented. The isolated `maja-runtime` code generator can produce a pure checked integer-add function for a narrowly validated D1 interaction; it is not connected to this profile or an application state owner. Browser text rendering and cross-platform geometry, typography, accessibility and security are not qualified by the experimental admission.

Do not treat static rendering as Phase 1 completion. The first interactive application still needs a versioned shared contract for initial state, visible values, inputs and explicit local authority, then a browser state owner and differential tests against Goya's canonical executor. No Frampton server is required for that work.
