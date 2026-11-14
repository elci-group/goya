# Neptune source preview

Neptune currently provides an experimental Swift source preview for one `.gscene` cell with an authored `id` and static `text`. It does not provide an admitted iOS build. `check` reports failure until a native mapping is admitted; Apple compilation and device verification have not run.

`neptune inspect <file> --target ios --format json` returns a machine-readable inspection. `neptune preview <file> --module app/main.gscene --format json` returns a source-preview bundle with generated Swift, source map, capability inventory, hashes, and stage status. `--module` is the canonical project-relative identity of the input, supplied by the caller; it must use ASCII path components and cannot contain `.` or `..` components. It is a logical identity, not a file lookup.

`neptune preview <file> --module app/main.gscene --out <new-directory>` writes `App.swift`, `source-map.json`, `capabilities.json`, and `build-report.json`. The parent directory must exist and the output directory must not exist; existing output is never overwritten. If writing fails after directory creation, a partial directory may remain. This output is a source preview, and the report records that Apple validation has not run.

`neptune verify-preview <directory> [--format json]` checks that the three generated artifacts match their hashes in the report. This checks consistency only: a report and artifacts can be changed together, and no signature, Apple compilation, or device behavior is verified.

`neptune map-swift <directory> --source <scene.gscene> --line <n> --column <n> --format json` maps a location in `App.swift` to every matching Goya source range. Both coordinates are one-based and the column is a UTF-8 byte column; a worker must establish that its diagnostic uses that encoding before calling this command. The source bytes must match the input hash in the report, and generated artifacts must match their recorded hashes. Unmapped locations are reported explicitly. Mapping does not verify authenticity or execute Apple tools.

`neptune apple-plan <preview-directory> --lock <toolchain-lock.json> --format json` prepares a simulator type-check handoff from integrity-checked preview artifacts. The lock is a JSON object with string fields `schema_version` (`"1"`), `xcode_version`, `xcode_build`, `swift_version`, `sdk_version`, `sdk_build`, and `minimum_os`. Versions use two or three numeric components; build IDs use ASCII letters and digits. The minimum OS must not exceed the SDK version. Supply identities from the selected worker: this command does not detect or verify an installed toolchain.

The plan records fixed arguments and describes how a worker must obtain its SDK path and isolated copy of the source. It does not execute a command, reserve a worker, or validate Swift. A valid plan has status `BLOCKED` and exits with code 3 until a worker implementation runs it; malformed input exits with code 1 or 2. Artifact hashes establish consistency with the preview report, not authenticity.

`neptune preview-transition <file> --module app/main.gscene --profile experimental --format json` emits standalone Swift source for one Goya D1 interaction, together with the mapping of source state/input keys to generated fields. It stages values from the original snapshot and checks arithmetic and revision overflow before returning a new state. The generated code includes UTF-8 text, snapshot, and register budget checks derived from the Goya D1 executor. It is not an application and does not provide runtime adapters. The result records Apple validation and runtime resource-limit parity as not verified; the generated checks must be compiled and compared against the executor before this mapping is admitted.

`neptune test --suite d1-corpus --format json` runs the checked-in integer fixtures against Goya's D1 executor and an independent interpreter of Neptune's target representation. Its pass/fail applies only to those host-side fixtures; the report keeps generated Swift, Apple compilation, simulator, and device stages blocked. The scene and hand-authored expectations are in `conformance/`, and their hashes are included in the result. `neptune test --suite admission --worker mac-primary --format json` reports `BLOCKED` with exit code 3 while worker and device admission are unavailable.

## Local service protocol, version 1

Run `neptune serve` and send one JSON object per line to stdin. Each line must be at most 256 KiB including its newline. The service emits one JSON event per request on stdout and processes requests sequentially. Close stdin to end the session. A malformed JSON line gets a protocol failure; an oversized or unterminated line ends the session with failure.

Example request:

```json
{"schema_version":"1","job_id":"example-1","operation":"preview","module":"app/main.gscene","source":"@ontology example { @cell root { id: \"root\" text: \"Hello\" } }"}
```

Operations are `inspect`, `check`, `preview`, `preview-transition`, `map-swift`, and `test`. Accepted fields are `schema_version`, `job_id`, `operation`, `source`, `module`, `profile`, `bundle`, `line`, `column`, `column_encoding`, `raw_diagnostic`, and `suite`. Previews require `module`, and `preview-transition` additionally requires `"profile":"experimental"`. For `map-swift`, supply the original `source`, the complete inline preview `bundle`, positive `line` and `column`, and `"column_encoding":"utf8-byte"`. An optional `raw_diagnostic` string is returned unchanged and is limited to 16 KiB. The `test` operation accepts `"suite":"d1-corpus"` or `"admission"`. Preview bundles and source maps use schema version 2 for the module-qualified semantic identity. Job IDs contain 1–128 ASCII letters, digits, underscores, or hyphens; each ID is single-use within a session, including failed requests. Retrying requires a new ID. A session accepts at most 1,024 IDs.

Each event contains `schema_version`, `job_id`, `sequence` (currently always 1), `stage`, and `status`. A successful `inspect` means inspection completed; its result separately reports the failed iOS validation. A successful `preview` means source generation completed, with verification labeled `SOURCE_PREVIEW`. A `check` cannot currently pass. Build, cancellation, artifact retrieval, multi-event streaming, and persistent idempotency are not implemented and must not be inferred from this protocol.
