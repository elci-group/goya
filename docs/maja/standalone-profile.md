# Experimental standalone profile

The interactive successor, [`maja.web.local.v2`](interactive-profile.md), admits GSSH/3 scenes with nested elements, controls, session state and D1 interactions. This page describes only the v1 static profile, which is unchanged.

`maja.web.local.v1` currently admits only one strictly parsed root `@cell` with an explicit authored `id` and static `text`. All other authored properties, children, controls, interactions and effects are rejected with a source-linked diagnostic. U+0000 and carriage returns in text, and attribute-normalized control characters in IDs, are rejected. No implicit state, bindings, persistence, server requests or capabilities are introduced. The legacy `maja.web.v1` remains fail-closed.

```
maja inspect app.gscene
maja check app.gscene --target web --profile maja.web.local.v1
maja build app.gscene --profile maja.web.local.v1 --out new-output-directory
```

The output is an experimental static package: `index.html`, `app.css`, and `manifest.json`. The manifest contains a source SHA-256, text-origin byte range, compiler/profile identity, and SHA-256 values for HTML and CSS. It is an integrity aid, not an authenticity guarantee. Build output is deterministic for identical source and compiler configuration, contains no timestamps or local paths, and refuses to replace an existing output directory. Parent directory must already exist. `--release` is rejected because this profile is not release-qualified.

The generated document carries a restrictive Content Security Policy: all
resource types are denied by default, with only same-origin stylesheets and
`data:` images allowed. Base URI changes and form submissions are denied.
This policy limits browser capabilities; it does not qualify the profile's
overall security or accessibility.

In this v1 profile, no browser application logic, button, input, state binding, navigation, local storage, offline caching, optional remote services, machine-readable diagnostics, compiler service protocol or browser source map is implemented. The `maja-runtime` D1 emitter is used only by the v2 profile. Browser text rendering and cross-platform geometry, typography, accessibility and security are not qualified by the experimental admission.

Do not treat static rendering as Phase 1 completion. The interactive work is tracked in [wave 3](wave-3.md); no Frampton server is required for it.
