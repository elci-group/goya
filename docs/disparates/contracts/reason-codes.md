# FAIR reason-code register

Contract: [GY-FAIR/2](fair-2.md). Under FAIR/2, every reason code must be in
this table, and `goya_fair::validate` refuses any other. FAIR/1 documents keep
FAIR/1's rule (uppercase snake case only).

Codes are added by contract revision. They are never renamed or removed. The
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
| `INLINE_HANDLER_NOT_ANALYSED` | Inline event-handler code in markup was not analysed. | FAIR/2 |
| `INLINE_SCRIPT_NOT_ANALYSED` | An inline script in markup was not analysed. | FAIR/2 |
| `INLINE_STYLE_NOT_ANALYSED` | An inline style block in markup was not analysed. | FAIR/2 |
| `IPC_CHANNEL_COMPUTED` | An IPC channel name is not a string literal. | FAIR/2 |
| `NATIVE_MODULE_OPAQUE` | A native module's behaviour cannot be read from source. | FAIR/1 example |
| `NON_UTF8_SOURCE` | Source text is not valid UTF-8. | FAIR/2 |
| `NO_FAIR_KIND` | The construct has no kind in the FAIR vocabulary. | FAIR/2 |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | Goya has no capability for spawning processes. | FAIR/1 example |
| `PARSE_FAILED` | The source could not be parsed. | FAIR/2 |
| `SYMLINK_NOT_FOLLOWED` | A symbolic link was recorded and not followed. | FAIR/2 |
