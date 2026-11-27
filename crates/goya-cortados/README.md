# Cortados Java source analyzer

Phase 1 implements offline Java source ingestion, syntax parsing, conservative
semantic observations and FAIR-linked CSIR evidence. It does not generate native
Goya source or certify converted applications.

```sh
cargo run -p goya-cortados --bin cortados -- analyse fixtures/cortados/source \
  --output /tmp/cortados-example
```

The output directory must be new and its parent must exist. On Linux, the parent
must be owned by the current user or root, without unprotected group/other write
access; sticky shared directories such as `/tmp` are supported. Existing output
is never overwritten. Artifact filenames are fixed and publication is atomic.

The six outputs are `application.fair.json`, `application.csir.json`,
`diagnostics.json`, `dependencies.json`, `capabilities.json` and `bundle.json`.
FAIR/2 is admitted by `goya-fair`; CSIR binds canonical FAIR and exact source bytes
by SHA-256. The manifest records independent syntax/semantic completeness.
`syntax_complete` covers retained Java sources; exclusions remain visible and
overall semantic completeness stays false. No capability grants or certificates
are inferred from names, comments or successful parsing.

Exit 0 means syntax analysis completed, including unresolved semantic findings.
Exit 4 covers usage, ingestion, syntax/profile, quota and output failures. A
malformed-source run still writes its diagnostic bundle when validation permits.
Fatal ingestion produces structured stderr diagnostics and no bundle.

Library callers use `analysis::analyse(&Path, &Limits)`, then
`AnalysisBundle::validate()` and `artifacts()`. Detached artifact admission uses
`bundle::validate_artifacts(&artifacts, &original_sources)`. The library does not
change its caller's process limits; the CLI supervises a separate worker with
memory, CPU, monotonic wall-time and file-size caps. Input and output handling
currently require Linux with procfs available.

The pinned Java grammar is complemented by Unicode escape translation, original
byte mapping, literal/profile checks and a Java 17 fixture corpus. It is a syntax
frontend, not `javac` attribution. Complete JLS identifier equivalence is not
established: the upstream grammar uses Unicode XID rules. JDK/external symbol
inventories, overload resolution and broad scope attribution remain unresolved.
Only narrowly proved same-unit type-header references are resolved. All method
calls remain unresolved. Build files are read as metadata and never executed.

See [Phase 1 evidence and limits](../../docs/cortados/phase1.md).
