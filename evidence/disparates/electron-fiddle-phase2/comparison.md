# Phase 2 deficit comparison: electron-fiddle vs Reactotron reference

Reference: `evidence/disparates/reactotron-phase1/reference.deficit.json`
(345 elements). This run: `evidence/disparates/electron-fiddle-phase2/extraction.deficit.json`
(401 elements, `electron/fiddle` pinned at `82fb8290…`, tag v0.4.0).

## Element counts

| Kind | Reactotron | electron-fiddle | Note |
|---|---|---|---|
| component | 22 | 73 | |
| event_handler | 87 | 43 | |
| hook | 63 | 0 | fiddle v0.4.0 predates React hooks usage in its renderer |
| style_rule | 130 | 0 | fiddle v0.4.0 uses Less via webpack, not inline style scanning targets |
| ipc_channel | 9 | 8 | fiddle's 19 declared channels are mediated by `IpcMainManager`, so only template-data channels register as direct sites |
| os_api_call | 12 | 66 | inflated by `static/templates/**` example data |
| filesystem_operation | 9 | 93 | inflated by templates |
| state_store | 3 | 34 | |
| child_process | 9 | 10 | |
| window_lifecycle | 1 | 45 | almost entirely template data |
| dynamic_module_resolution | 0 | 29 | computed `require()`s; new vs Reactotron |
| **total** | **345** | **401** | |

## Assessed totals

| Status | Reactotron | electron-fiddle |
|---|---|---|
| representable | 249 (72%) | 169 (42%) |
| undetermined | 75 (22%) | 156 (39%) |
| unrepresentable | 21 (6%) | 76 (19%) |

## Deficit reason codes (unrepresentable populations)

| Reason code | Reactotron | electron-fiddle | Category |
|---|---|---|---|
| `PERSISTENCE_CAPABILITY_UNRESOLVED` | 12 | 127 (filesystem_operation 93, state_store 34) | Goya capability decision pending |
| `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` | 12 | 66 (os_api_call 66) | Goya capability deficit |
| `DYNAMIC_REQUIRE_UNRESOLVED` | 0 | 29 (dynamic_module_resolution) | extractor `undetermined` (programme debt), surfaced by assess |
| `NO_GOYA_CAPABILITY_CHILD_PROCESS` | 9 | 10 | Goya capability deficit |
| `HOOK_DATAFLOW_NOT_EXTRACTED` | 63 | 0 (no hooks at this pin) | assessment-method deficit (programme debt) |

## Prioritisation signal

1. `PERSISTENCE_CAPABILITY_UNRESOLVED` grows ~10x and becomes the dominant
   blocker on the more complex application — the strongest signal that a
   filesystem/persistence contract decision should precede phase 4.
2. `NO_GOYA_CAPABILITY_DESKTOP_OS_INTEGRATION` grows ~5x — but a large share
   of the fiddle population is template *data* (see honesty notes in
   `run.json`), so the app-owned deficit is smaller than the raw count
   suggests; a scan-scope mechanism is needed before this code's priority is
   set from template-inflated counts.
3. `DYNAMIC_REQUIRE_UNRESOLVED` appears for the first time (29 elements),
   confirming the roadmap's criterion-2 prediction; it is extractor debt, not
   a Goya capability gap.
4. No new reason codes were required; all four codes come from the existing
   register. Flagged for future register discussion (not emitted anywhere):
   a code distinguishing *data-file* constructs scanned from shipped example
   content (e.g. `TEMPLATE_DATA_NOT_APP_SOURCE`) would let coverage reports
   separate app semantics from shipped sample semantics without a scan-scope
   extractor mechanism.
