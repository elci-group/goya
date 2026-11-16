# Neptune source preview

Neptune provides experimental Swift source previews for selected `.gscene`
subsets, ranging from a single static cell to observable button scenes with
bounded D1 state transitions. It does not provide an admitted iOS build.
`check` reports failure until a native mapping is admitted; Apple compilation
and device verification have not run.

From the workspace root, run the host-side tests and a source preview with:

```sh
cargo test -p neptune
cargo run -p neptune -- preview crates/neptune/tests/static.gscene --module app/main.gscene --format json
cargo run -p neptune -- test --suite d1-corpus --format json
```

The corpus compares host-side implementations; it does not execute generated
Swift. The optional `swift-parse` operation requires `swiftc` on `PATH`.
See [the pinned Swift bootstrap](../../docs/swift-bootstrap.md) for a Linux
toolchain; it does not provide an Apple SDK or device qualification.

`neptune inspect <file> --target ios --format json` returns a machine-readable inspection. `neptune preview <file> --module app/main.gscene --format json` returns a source-preview bundle with generated Swift, source map, capability inventory, hashes, and stage status. Its single static cell emits `Text(verbatim:)`, preserving authored text instead of treating it as a localization key. `--module` is the canonical project-relative identity of the input, supplied by the caller; it must use ASCII path components and cannot contain `.` or `..` components. It is a logical identity, not a file lookup.

`neptune preview-static-column <file> --module app/main.gscene --profile experimental --format json` emits a source-only SwiftUI column for 1–32 static `@cell` nodes, at most 16 levels deep. A cell may contain authored `text`, child cells, or both. Text is emitted before that cell's children, and sibling order follows the scene. The shared IR text sequence must match. Each text uses `Text(verbatim:)`, so the authored characters are the content. Fill, stroke, type size, anchor, wrap, scroll, absolute and proportional geometry, buttons, inputs, and interactions are rejected. A leaf with no text is rejected. The generated `VStack` records child order only; it does not reproduce solved rectangles, Dynamic Type, safe area, or accessibility behavior. Apple compilation and iOS admission stay unrun. The one-cell `preview` command is unchanged.

`neptune preview <file> --module app/main.gscene --out <new-directory>` writes `App.swift`, `source-map.json`, `capabilities.json`, and `build-report.json`. The parent directory must exist and the output directory must not exist; existing output is never overwritten. If writing fails after directory creation, a partial directory may remain. This output is a source preview, and the report records that Apple validation has not run.

`neptune verify-preview <directory> [--format json]` checks that the three generated artifacts match their hashes in the report. This checks consistency only: a report and artifacts can be changed together, and no signature, Apple compilation, or device behavior is verified.

`neptune map-swift <directory> --source <scene.gscene> --line <n> --column <n> --format json` maps a location in `App.swift` to every matching Goya source range. Both coordinates are one-based and the column is a UTF-8 byte column; a worker must establish that its diagnostic uses that encoding before calling this command. The source bytes must match the input hash in the report, and generated artifacts must match their recorded hashes. Unmapped locations are reported explicitly. Mapping does not verify authenticity or execute Apple tools.

`neptune apple-plan <preview-directory> --lock <toolchain-lock.json> --format json` prepares a simulator type-check handoff from integrity-checked preview artifacts. The lock is a JSON object with string fields `schema_version` (`"1"`), `xcode_version`, `xcode_build`, `swift_version`, `sdk_version`, `sdk_build`, and `minimum_os`. Versions use two or three numeric components; build IDs use ASCII letters and digits. The minimum OS must not exceed the SDK version. Supply identities from the selected worker: this command does not detect or verify an installed toolchain.

The plan records fixed arguments and describes how a worker must obtain its SDK path and isolated copy of the source. It does not execute a command, reserve a worker, or validate Swift. A valid plan has status `BLOCKED` and exits with code 3 until a worker implementation runs it; malformed input exits with code 1 or 2. Artifact hashes establish consistency with the preview report, not authenticity.

`neptune preview-transition <file> --module app/main.gscene --profile experimental --format json` emits standalone Swift source for one Goya D1 interaction, together with the mapping of source state/input keys to generated fields. It stages values from the original snapshot and checks arithmetic and revision overflow before returning a new state. The generated code includes UTF-8 text, snapshot, and register budget checks derived from the Goya D1 executor. It is not an application and does not provide runtime adapters. The result records Apple validation and runtime resource-limit parity as not verified; the generated checks must be compiled and compared against the executor before this mapping is admitted.

`neptune model-d1-state <file> --module app/main.gscene --profile experimental --initial <state.json> --format json` validates explicit initial values and lowers all D1 actions into a state-only model. The initial-state transport is a JSON array of `[key, type, value]` triples, where type is `int`, `text`, or `bool`; it is transport data for this experimental command, not a new `.gscene` grammar or a complete project manifest. The JSON result contains the supplied initial values, typed action expressions, source identities, and stage status. It does not validate presentation or generate an app.

`neptune preview-d1-owner <file> --module app/main.gscene --profile experimental --initial <state.json> --format json` produces an experimental Swift state-owner preview for 1–64 D1 actions. It uses one main-actor owner, action-specific typed inputs, staged writes, and checks derived from the shared Goya D1 limits. The result maps generated field and method names to authored identities and includes the Swift source, hashes, and action source ranges. It contains initial values in the generated Swift. It does not generate views, persistence, state restoration, or a complete app; Swift compilation and runtime parity remain unverified.

`neptune preview-native-button <file> --module app/main.gscene --profile experimental --initial <state.json> --format json` emits a source-only SwiftUI app preview for one root button with a literal nonempty label and one input-free D1 action. Explicit initial values use the same transport as the state-owner preview. Other presentation properties and interactions are rejected. The response contains generated Swift, hashes, source ranges and capability metadata; it does not establish Swift compilation, accessibility behavior, persistence, simulator execution or device admission. Initial values are embedded in the Swift.

`neptune preview-observable-button` takes the same arguments and the same scene. It adds a readout after the button: each admitted state cell in UTF-8 key order, then the revision. Integers and the revision are decimal text, booleans are `true` or `false`, and text is the stored characters. The readout reads the published state. The button action is the only dispatch, so a failed action leaves the previous readout in place. The button preview does not include this readout. Swift compilation, accessibility, and comparison with the D1 application on a device remain unrun.

`neptune preview-observable-column` takes the same arguments. The scene is static text plus exactly one button with one input-free D1 action, at most 32 nodes and 16 levels deep. Child order follows the scene. The readout follows the column and uses the same encodings. A failed action leaves the previous readout. `preview-static-column` and `preview-native-button` reject this scene. Swift compilation, accessibility, and device comparison remain unrun.

`neptune preview-observable-buttons` takes the same arguments. The scene is static text plus at least two buttons, each with its own input-free D1 action, within the same node and depth limits. Each button calls the owner method bound to that button. Those methods follow authored action-id order, so the first button on screen can call `dispatch1` while a later button calls `dispatch0`. The readout follows the column and uses the same encodings. A failed action leaves the previous readout. `preview-observable-column` rejects this scene. Swift compilation, accessibility, and device comparison remain unrun.

`neptune trace-observable-buttons` takes those arguments plus `--activations <actions.json>`, a JSON array of admitted action ids, at most 32 entries. It runs that sequence on the shared D1 application owner and reports the initial state plus each activation. A passing step publishes the new state and revision. A failing step records an error class and keeps the previous state and revision. Each step names the generated button method for that action. The command checks that the generated source calls each method once from a button. It does not execute Swift. Apple compilation and iOS admission stay unrun.

`neptune trace-observable-column` takes the same arguments as `trace-observable-buttons`. The scene is one that `preview-observable-column` accepts: static text plus exactly one button. The trace uses that button's `dispatch0` method. A failed action keeps the previous state and revision. `trace-observable-buttons` rejects this one-button scene. Swift execution, Apple compilation, and iOS admission stay unrun.

The `NPT-D1-INIT/1` initial-state hash is SHA-256 over the ASCII domain `NPT-D1-INIT/1` followed by a zero byte, the number of cells as a little-endian u64, then cells in UTF-8 key order. Each cell contains its key length as a little-endian u64, key bytes, and a value: tag `0` plus a little-endian i64, tag `1` plus a little-endian u64 byte length and unnormalized UTF-8 bytes, or tag `2` plus one byte `0` or `1` for boolean. This hashes the validated state, independent of input triple order.

`neptune test --suite d1-corpus --format json` runs checked-in integer, text, and boolean fixtures against Goya's D1 executor, the shared Goya application state owner, and an independent interpreter of Neptune's target representation. The cases cover state updates, arithmetic and revision overflow, missing or mistyped state and input, and Unicode byte preservation. Its pass/fail applies only to those host-side fixtures; the report keeps generated Swift, Apple compilation, simulator, and device stages blocked. The scenes and hand-authored expectations are in `conformance/`, and their hashes are included in the version 2 report. The compiler uses `goya-sdk` for this host-side oracle; generated iOS source does not link that crate. `neptune test --suite admission --worker mac-primary --format json` reports `BLOCKED` with exit code 3 while worker and device admission are unavailable.

## Local service protocol, version 1

Run `neptune serve` and send one JSON object per line to stdin. Each line must be at most 256 KiB including its newline. The service emits one JSON event per request on stdout and processes requests sequentially. Close stdin to end the session. A malformed JSON line gets a protocol failure; an oversized or unterminated line ends the session with failure.

Example request:

```json
{"schema_version":"1","job_id":"example-1","operation":"preview","module":"app/main.gscene","source":"@ontology example { @cell root { id: \"root\" text: \"Hello\" } }"}
```

Operations are `inspect`, `check`, `preview`, `preview-static-column`, `preview-transition`, `model-d1-state`, `preview-d1-owner`, `preview-native-button`, `preview-observable-button`, `preview-observable-column`, `preview-observable-buttons`, `trace-observable-buttons`, `trace-observable-column`, `map-swift`, and `test`. Accepted fields are `schema_version`, `job_id`, `operation`, `source`, `module`, `profile`, `initial`, `activations`, `bundle`, `line`, `column`, `column_encoding`, `raw_diagnostic`, and `suite`. Previews and state modeling require `module`; `preview-static-column`, `preview-transition`, `model-d1-state`, `preview-d1-owner`, `preview-native-button`, `preview-observable-button`, `preview-observable-column`, `preview-observable-buttons`, `trace-observable-buttons`, and `trace-observable-column` also require `"profile":"experimental"`. For `model-d1-state`, `preview-d1-owner`, `preview-native-button`, `preview-observable-button`, `preview-observable-column`, `preview-observable-buttons`, `trace-observable-buttons`, and `trace-observable-column`, `initial` is the same array of triples accepted by the CLI. For `trace-observable-buttons` and `trace-observable-column`, `activations` is an array of admitted action ids. For `map-swift`, supply the original `source`, the complete inline preview `bundle`, positive `line` and `column`, and `"column_encoding":"utf8-byte"`. An optional `raw_diagnostic` string is returned unchanged and is limited to 16 KiB. The `test` operation accepts `"suite":"d1-corpus"` or `"admission"`. Preview bundles and source maps use schema version 2 for the module-qualified semantic identity. Job IDs contain 1–128 ASCII letters, digits, underscores, or hyphens; each ID is single-use within a session, including failed requests. Retrying requires a new ID. A session accepts at most 1,024 IDs.

Each event contains `schema_version`, `job_id`, `sequence` (currently always 1), `stage`, and `status`. A successful `inspect` means inspection completed; its result separately reports the failed iOS validation. A successful `preview` means source generation completed, with verification labeled `SOURCE_PREVIEW`. A `check` cannot currently pass. Build, cancellation, artifact retrieval, multi-event streaming, and persistent idempotency are not implemented and must not be inferred from this protocol.

`neptune serve` accepts `validate-deploy-manifest` with a typed `manifest` object matching `goya.neptune.deploy/0.0.1`. The minimum OS version must have two or three nonempty numeric components, such as `17.0` or `18.1.2`. This validates structure and supported settings; it does not read the referenced IPA, validate Apple signatures, or contact a device. The CLI `dock-verify` additionally checks the IPA SHA-256 against actual bytes.

`neptune swift-parse <generated.swift>` reads a UTF-8 source file of at most
256 KiB and invokes the installed Swift frontend in parse-only mode, passing
the source on standard input. A pass confirms syntax parsing; it does not
resolve Apple frameworks, type-check against an iOS SDK, link, or run the app.
Missing Swift returns exit code 3; an unreadable or oversized file fails before
the compiler is started. Compiler diagnostics refer to standard input.
The JSON-lines service accepts the same operation as `swift-parse` with a
`source` string; request size is bounded by the protocol limit. The service
allows 15 seconds for the compiler and its pipes to finish, terminates and
reaps the direct compiler process on timeout, and limits returned diagnostics
to 16 KiB. A failed compiler response indicates whether diagnostics were
truncated. The standalone CLI uses the same timeout and diagnostic limit.
