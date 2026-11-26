# goya-disparate-electron

Status: **v0.2.0, extraction only.** This is the Disparates programme's Electron
extractor (GY-ENT-001, Phase 1). Directive DEL-TD-001 calls it Delectable. It
reads an Electron application and writes a [GY-FAIR/2](../../docs/disparates/contracts/fair-2.md)
document: elements, typed relationships, and a project record. It does not
run the application, use the network, judge representability, or generate
Goya source. GY-ENT-001 withholds reconstruction.

```sh
cargo run -p goya-disparate-electron -- extract fixtures/disparates/electron-counter --summary
cargo run -p goya-disparate-electron -- extract path/to/app.asar --root-label app.asar
```

The canonical FAIR JSON goes to stdout. `--summary` writes counts per status to
stderr. Exit codes: 0 done, 2 usage, 3 I/O, 4 invalid input or budget
exceeded, 5 the extractor produced an invalid FAIR document (a bug).

## Input

* **A directory.** Entries are read in byte order. Symbolic links are recorded
  and never followed. `node_modules` and `.git` are recorded and not
  descended into.
* **An `app.asar` archive.** It is read entirely in memory and nothing is
  written to disk, so no entry can escape a root. Entry names must be single
  path components; `..`, `/`, `\` and NUL are refused. Offsets and sizes are
  bounds-checked, and SHA-256 integrity records are verified. Links and
  `unpacked` entries are recorded and not read, because
  `app.asar.unpacked` is not supplied.
* **Budgets** (`material::Budgets`): 20,000 files, 32 MiB per file, 512 MiB in
  total, nesting depth 64, and a 64 MiB ASAR header. Exceeding any of them
  stops extraction (exit 4).

Every file read is in the SHA-256 source manifest. A directory and the ASAR
built from it give byte-identical FAIR (tested).

## What it extracts

Oxc remains the JavaScript/JSX/TypeScript parser for now (owner decision,
26 November 2026). A Goya-owned replacement is a **high-priority planned work
item**; implementation has not started. See the
[forward plan and replacement gates](../../docs/disparates/roadmap.md#forward-planning-replace-oxc-high-priority).

| Source | Pass | Elements |
| --- | --- | --- |
| `package.json` | serde_json | `package_manifest` with name, version, `main`, licence and dependency maps |
| `.js .cjs .mjs .jsx .ts .tsx …` | Oxc 0.146 | `window_lifecycle` (`new BrowserWindow` with literal options, `app.*`, window `loadFile`/`loadURL`), `ipc_channel` (both sides, channel, method, module state the handler mutates), `preload_bridge` (key, members, the IPC each member calls), `event_handler` (`addEventListener` and `on<event> =`, with DOM target, `preventDefault` and `window.*` calls), `state_store` (module-scope `let`/`var` assigned inside a function; `localStorage`, `sessionStorage`, `indexedDB.open`), `os_api_call`, `filesystem_operation`, `child_process`, `native_module`, `dynamic_module_resolution`, `module_import`, `logic_unit` (top-level function declarations), `bridge_call` (`window.<key>.<member>()` where a preload exposes `<key>`), and `unknown` for unclassified Electron APIs. IPC and DOM handlers also record the module-scope state they `reads` and `mutates`. |
| `.html .htm` | [`goya-html-tokenizer`](../goya-html-tokenizer) | `component` for every start tag, with attributes, direct text and parent; `event_handler` for inline `on*` attributes |
| `.css` | [`goya-css-syntax`](../goya-css-syntax) | `style_rule` with selector and declarations in source order (duplicates kept; declarations the CSS specification discards are kept as `unparsed`) |

Each element carries `process`: `main`, `preload` or `renderer` if
topology reached its file, otherwise `unresolved`. `main` comes from
`package.json`. `preload` comes from the window's
`webPreferences.preload` when it is `path.join(__dirname, '…')`. The renderer
page comes from `loadFile`, and renderer scripts and styles come from that
page's `<script src>` and `<link rel=stylesheet>`.

IDs follow `<file>#<kind>.<nnnn>`, numbered per kind per file in emission order
(`ID_RULE` in `src/lib.rs`).

## Relationships and the project record (FAIR/2)

`src/relate.rs` builds these from the elements:

| Relation | From → to | Basis |
| --- | --- | --- |
| `contains` | parent component → child component | extracted (markup nesting) |
| `exposes` | preload bridge → IPC call inside one of its members | extracted (span containment) |
| `invokes` | renderer IPC sender → main IPC receiver with the same literal channel (`invoke`→`handle`/`handleOnce`, `send`/`sendSync`/`postMessage`→`on`/`once`, `webContents.send`→renderer `on`/`once`) | derived |
| `invokes` | DOM handler or bridge call → the preload bridge with that key and member | derived |
| `handles` | DOM handler → the component with the target's `id` | derived |
| `styles` | `#id` style rule → the component with that `id` | derived |
| `reads`, `mutates` | IPC or DOM handler → module-scope state of the same name in the same file | derived |
| `loads` | window `loadFile` → the loaded page's first root component | derived |

A derived relationship names both ends as evidence. If a join matches zero
elements, or more than one, no relationship is produced.

`project.entry_points` records `main` (from `package.json`), `preload` (from the
window options), the renderer page (from `loadFile`) and the page's scripts and
stylesheets, each with the element that establishes it.
`project.framework_detections` records Electron, with its declared version,
when the manifest depends on it or a file imports it.

## What it does not do

Each of the following is recorded as an `undetermined` element with a reason
code from the [FAIR/2 register](../../docs/disparates/contracts/reason-codes.md),
never left out:

| Code | Meaning |
| --- | --- |
| `DYNAMIC_REQUIRE_UNRESOLVED` | `require()` or `import()` with a computed specifier |
| `IPC_CHANNEL_COMPUTED`, `BRIDGE_KEY_COMPUTED` | the channel or bridge key is not a string literal |
| `ELECTRON_API_UNCLASSIFIED` | an Electron API outside this version's tables |
| `INLINE_SCRIPT_NOT_ANALYSED`, `INLINE_STYLE_NOT_ANALYSED`, `INLINE_HANDLER_NOT_ANALYSED` | inline code in HTML |
| `CSS_AT_RULE_NOT_ANALYSED` | `@media`, `@font-face` and other at-rules |
| `PARSE_FAILED` | a syntax error. Oxc stops at fatal errors, so the file then has only this element |
| `NON_UTF8_SOURCE` | the source text is not UTF-8 |
| `SYMLINK_NOT_FOLLOWED`, `DEPENDENCY_TREE_NOT_ANALYSED`, `EXCLUDED_DIRECTORY`, `ASAR_UNPACKED_NOT_SUPPLIED` | material that was seen and not read |

Known limits of v0.1.0:

* Names are resolved through bindings only, with no scope analysis. A local
  that shadows `ipcMain` is read as `ipcMain`.
* There are no control-flow, data-flow or call graphs. Relationships join
  literal values (channels, bridge keys, DOM IDs, binding names). A computed
  channel or ID produces no relationship. `reads` and `mutates` match by
  binding name and inherit the no-scope-analysis limit above.
* HTML is tokenized, not tree-built: implied end tags, foster parenting and the
  other tree-construction repairs are not applied, so parents come from the
  literal tag nesting. Every component carries the exact span of its start
  tag. A tag the input ends inside is recorded as `html_unterminated_tag`
  (`PARSE_FAILED`) rather than dropped.
* There is no framework analysis (React, Vue), no source maps and no
  bundle splitting. Windows installers, macOS bundles and AppImages are not
  unpacked.

## Tests

`tests/extract.rs` covers:

* a byte-for-byte golden file for the DEL-B01 fixture, including its 30
  relationships and project record
  (`tests/golden/electron-counter.fair.json`; regenerate with `GOLDEN_UPDATE=1`)
* determinism
* the fixture README's oracle, as assertions, and the full chain from the
  click handler to the `count` state
* no relationship from an ambiguous join
* ASAR-versus-directory equivalence
* hostile archives (escaping names, out-of-range data, a wrong integrity hash,
  bad headers)
* budgets, links and dependency directories
* the constructs the fixture does not exercise
