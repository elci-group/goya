# Neptune source preview

Neptune currently provides an experimental Swift source preview for one `.gscene` cell with an authored `id` and static `text`. It does not provide an admitted iOS build. `check` reports failure until a native mapping is admitted; Apple compilation and device verification have not run.

`neptune inspect <file> --target ios --format json` returns a machine-readable inspection. `neptune preview <file> --format json` returns a source-preview bundle with generated Swift, source map, capability inventory, hashes, and stage status.

## Local service protocol, version 1

Run `neptune serve` and send one JSON object per line to stdin. Each line must be at most 256 KiB including its newline. The service emits one JSON event per request on stdout and processes requests sequentially. Close stdin to end the session. A malformed JSON line gets a protocol failure; an oversized or unterminated line ends the session with failure.

Example request:

```json
{"schema_version":"1","job_id":"example-1","operation":"preview","source":"@ontology example { @cell root { id: \"root\" text: \"Hello\" } }"}
```

Operations are `inspect`, `check`, and `preview`. The only accepted fields are `schema_version`, `job_id`, `operation`, and `source`. Job IDs contain 1–128 ASCII letters, digits, underscores, or hyphens; each ID is single-use within a session, including failed requests. Retrying requires a new ID. A session accepts at most 1,024 IDs.

Each event contains `schema_version`, `job_id`, `sequence` (currently always 1), `stage`, and `status`. A successful `inspect` means inspection completed; its result separately reports the failed iOS validation. A successful `preview` means source generation completed, with verification labeled `SOURCE_PREVIEW`. A `check` cannot currently pass. Build, cancellation, artifact retrieval, multi-event streaming, and persistent idempotency are not implemented and must not be inferred from this protocol.
