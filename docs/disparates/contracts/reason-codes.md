# FAIR reason-code register

Contract: [GY-FAIR/2](fair-2.md). Under FAIR/2, every reason code must be in
this table, and `goya_fair::validate` refuses any other. FAIR/1 documents keep
FAIR/1's rule (uppercase snake case only).

Codes are added by contract revision. They are never renamed or removed.
Assessment codes from [GY-COVERAGE/1](coverage-1.md) were admitted on
27 November 2026 (owner decision), so assessed documents validate as FAIR/2.
Their blocking categories stay in [../reason-codes.md](../reason-codes.md). The
table is sorted by code. `goya_fair::REASON_CODES` mirrors it, and the test
`register_matches_its_document` keeps the two equal.

| Code | Meaning | Introduced |
| --- | --- | --- |
| `ASAR_UNPACKED_NOT_SUPPLIED` | The file lives in `app.asar.unpacked`, which was not supplied. | FAIR/2 |
| `BRIDGE_KEY_COMPUTED` | A preload bridge key is not a string literal. | FAIR/2 |
| `CSS_AT_RULE_NOT_ANALYSED` | A CSS at-rule was recorded but not analysed. | FAIR/2 |
| `DEPENDENCY_TREE_NOT_ANALYSED` | An installed dependency tree was recorded but not inventoried. | FAIR/2 |
| `DYNAMIC_REQUIRE_UNRESOLVED` | A module specifier is computed, so its target is unknown. | FAIR/1 example |
| `ELECTRON_API_UNCLASSIFIED` | An Electron API outside the extractor's classification tables. | FAIR/2 |
| `EXCLUDED_DIRECTORY` | Material that is not application source, recorded and skipped. | FAIR/2 |
| `HOOK_DATAFLOW_NOT_EXTRACTED` | A hook was recorded without its data flow, so its state and effects cannot be assessed. | GY-COVERAGE/1 |
| `INLINE_HANDLER_NOT_ANALYSED` | Inline event-handler code in markup was not analysed. | FAIR/2 |
| `INLINE_SCRIPT_NOT_ANALYSED` | An inline script in markup was not analysed. | FAIR/2 |
| `INLINE_STYLE_NOT_ANALYSED` | An inline style block in markup was not analysed. | FAIR/2 |
| `IPC_CHANNEL_COMPUTED` | An IPC channel name is not a string literal. | FAIR/2 |
| `MAPPED_TO_ACTION_DECLARATION` | Interaction handlers map to declared Goya actions. | GY-COVERAGE/1 |
| `MAPPED_TO_ACTION_DISPATCH` | IPC requests and responses map to Goya actions; a simplification, not a transport mapping. | GY-COVERAGE/1 |
| `MAPPED_TO_CLIPBOARD_CAPABILITY` | Clipboard writes map to the capability-gated clipboard (GY-CLIPBOARD/1). | GY-COVERAGE/1 |
| `MAPPED_TO_NATIVE_WINDOW_HOST` | Window lifecycle maps to the native windowing host. | GY-COVERAGE/1 |
| `MAPPED_TO_SCENE_DECLARATION` | Component structure maps to scene node declarations. | GY-COVERAGE/1 |
| `MAPPED_TO_SCENE_STYLING` | Static styling maps to scene styling and layout constraints. | GY-COVERAGE/1 |
| `NATIVE_MODULE_OPAQUE` | A native module's behaviour cannot be read from source. | FAIR/1 example |
| `NON_UTF8_SOURCE` | Source text is not valid UTF-8. | FAIR/2 |
| `NO_COVERAGE_RULE` | No capability-map rule covers the construct, so assessment leaves it undetermined. | GY-COVERAGE/1 |
| `NO_FAIR_KIND` | The construct has no kind in the FAIR vocabulary. | FAIR/2 |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | Goya has no capability for spawning processes. | FAIR/1 example |
| `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` | Goya has no notification, dialog, shell or tray capability. | GY-COVERAGE/1 |
| `PARSE_FAILED` | The source could not be parsed. | FAIR/2 |
| `PERSISTENCE_CAPABILITY_UNRESOLVED` | Persistence has no declared Goya equivalent; a scoping decision is pending. | GY-COVERAGE/1 |
| `SYMLINK_NOT_FOLLOWED` | A symbolic link was recorded and not followed. | FAIR/2 |
