# Goya companion-system adapter boundary

`goya-ecosystem` provides the first typed integration boundary for companion
systems. Each operation requires a capability grant. Process adapters use
`Command` with argument vectors and never a shell.

| System | Adapter | Integration shape | Known limit |
|---|---|---|---|
| RUGID | `export_rugid`, `RugidRenderer` | Converts Goya scene geometry and visuals to RDF, then invokes the configured RUGID `rdf_render` binary through Cargo in a temporary directory | The RDF format has no Goya action identity; exports list omitted button actions. RUGID's binary is built on first use. |
| 3form | `Form3Agent` | Sends one NDJSON request to a configured `3form-agent` executable | The companion's optional `agent` feature must be enabled when building that executable. |
| Mesut | `MesutBridge` | Async typed host trait maps bounded `MesutTask` values (kind, label, payload, deadline) to the in-process Mesut `Work` API | This is a runtime-only seam: Mesut has no daemon or stable wire protocol, so an application embedding both runtimes must implement the bridge. A local consumer smoke test exercised this mapping against `MesuT` and `BlockingExecutor`. |
| Theosis | `TheosisClient` | Invokes the installed Theosis CLI for `check` and `update` | Updates require the distinct `InstallUpdate` grant and still follow Theosis/Baby policy. |

No adapters infer authority from request data. Missing capabilities, tools,
invalid data, oversized replies, and nonzero tool exits are returned as
errors. RUGID and 3form companion executables are optional installations;
Theosis may also be absent and is reported as unavailable.

Examples against local companion checkouts:

```sh
cargo run -p goya-ecosystem --example rugid_render -- /path/to/RUGID/Cargo.toml
cargo run --manifest-path /path/to/3form/Cargo.toml --features agent --bin 3form-agent
cargo run -p goya-ecosystem --example form3_smoke -- /path/to/3form-agent
THEOSIS_STATE=/tmp/goya-theosis cargo run -p goya-ecosystem --example theosis_check -- /path/to/theosis /path/to/application-repo /path/to/installed-binary
```
