# Cortados Phase 1 source analysis

Directive GOYA-DIS-CORT-001. Implementation: `crates/goya-cortados`, Rust.
Profile: `CORTADOS-JAVA-SOURCE/17.0`. Three Sol workers implemented parsing,
ingestion and evidence generation; the coordinator implemented the supervised
CLI, output publication and integration checks. Independent review hardened
worker limits and output commit behavior.

This milestone delivers a runnable, bounded source analyzer. It does not close
the full Cortado Core milestone: native emission, broad type attribution and the
substantive minimum viable application remain unavailable. No application is
assigned C1–C5, and no automatic C0 certificate is generated.

## Implemented behavior

`cortados analyse INPUT --output NEW_DIRECTORY` inventories directories or a single
Java source file. It hashes original resources and notices, parses Java sources,
and writes six versioned artifacts. It never runs Java, Maven, Gradle, annotation
processors, plugins or application initializers, and never retrieves dependencies.
Archive/class/DEX inputs remain explicitly deferred. Nested archives remain opaque
inventoried resources rather than expanded executable code.

Directory traversal pins Linux descriptors, refuses symlink traversal and special
files, checks inode/snapshot identity, bounds reads and records excluded dependency
or build trees. Portable path checks reject traversal, invalid encodings and
case/Unicode-normalized collisions. Intermediate ancestors are identity-checked;
inventoried directories/files receive snapshot checks. This avoids treating
unrelated changes to `/tmp` as changes to source material.

The syntax frontend uses pinned Tree-sitter 0.25.10 and tree-sitter-java 0.23.5.
Java Unicode escape eligibility, multiple `u` characters, surrogate pairs and
CR/LF translation retain mappings to original UTF-8 byte offsets. Syntax recovery
and unknown operations are preserved. The corpus exercises records, sealed types,
text blocks, switch expressions, instanceof type patterns, modules, nested/local
classes, lambdas, method references, exceptions and generics. Recognized later or
preview syntax is rejected under the Java 17 profile. Parser success is distinct
from Java semantic legality.

CSIR preserves implementation nodes and typed observations for declarations,
hierarchy headers, calls, state mutation, generic types, exceptions, concurrency,
platform effects and dependencies. No application intent is guessed. Resolution
is deliberately limited to unambiguous same-unit top-level type headers, with
imports, generics, nested declarations and multi-unit ambiguity conservatively
blocking the proof. Other type/call relationships receive COR-E110. Dynamic/JNI
observations retain their exact origin and explicit unresolved boundaries.

FAIR/2 and its reason register are unchanged. The shared library validates FAIR;
the attached CSIR, inventory and artifact hashes are checked against retained
original bytes. Strict deserialization, canonical serialization, origin/line
checks, link integrity, unknown preservation and proof-cycle checks reject
tampering. Internal validation also binds the retained extraction result; detached
validation cannot prove every semantic assertion without replaying extraction.

## Running and reading the result

```sh
cargo run -p goya-cortados --bin cortados -- analyse fixtures/cortados/source \
  --output /tmp/cortados-example
```

Use a fresh output name. The parent must exist and have trusted ownership and
write permissions; `/tmp` is supported through sticky-directory semantics.
Artifacts are created privately and committed with atomic no-replace rename.
Ordinary failures remove uncommitted stages. A postcommit durability failure
preserves the published files. Forced process termination can leave a private
`.cortados-stage-*` directory; there is no unconditional crash-cleanup guarantee.
The source inventory is captured before publication, including if output is placed
inside the source directory; future runs then see that changed input tree.

Exit 0 means the retained Java sources passed the implemented syntax/profile
checks, not that types were fully resolved. Exit 4 reports malformed sources,
unsupported commands/profiles, unsafe input/output, quota exhaustion or other
failure. Syntax failures retain a diagnostic bundle when possible. Fatal input
failures publish nothing. Inspect `bundle.json` and `diagnostics.json`; semantic
completeness and native conversion availability remain false.

The API is `analysis::analyse(&Path, &Limits)` for the fixed profile. CLI flags can
only lower profile quotas. The executable uses a separate resource-limited worker,
an independently armed monotonic kill timer and parent wall-time supervision.
It disables core dumps and privilege acquisition. These measures do not constitute
a general OS/network sandbox; offline operation follows from the implementation's
absence of input execution and dependency/network retrieval. Library embedders
must supply process-level resource isolation themselves.

## Remaining acceptance work

The upstream grammar uses Unicode XID identifier rules. Exact equivalence to the
Java 17 Character identifier tables has not been proved, so the corpus is not a
claim of universal JLS conformance. JDK/external symbol inventories are not supplied;
their versions, licenses and reachability stay unknown. Dependency/capability
reports do not select replacements or grant permissions. No Java process was run.

Full Java attribution, guaranteed parsing of every Java 17 source form, archive
frontends, source-to-native emission, JSER replacements, Java differential execution
and production release admission remain later acceptance work. The Phase 0 profile
and closure artifacts remain historical and unchanged; this report records the
implemented Phase 1 analysis slice and its limits.

Validation receipts and analyzer findings are recorded in
[phase1-evidence.json](phase1-evidence.json), [phase1-uni.json](phase1-uni.json) and
[phase1-amber.json](phase1-amber.json). Run the acceptance checks with:

```sh
deliver --spec docs/cortados/phase1.deliver.toml --base . --strict --allow-commands --format json
```

The dedicated target directory in that spec avoids contention with unrelated
workspace builds. Tests include an independent 1,000-input malformed-source
mutation corpus, deterministic output across input locations, linked artifact
tampering, symlink/FIFO and path attacks, lowered budgets, concurrent publication,
the postcommit sync regression and worker memory-limit failure.

The parser choice follows the [upstream Rust interface](https://docs.rs/tree-sitter-java/0.23.5/tree_sitter_java/).
The dependency audit found all eight direct dependencies used. Retain the grammar,
Unicode tables, cryptographic hashing and structured serialization: low call counts
do not make these equivalent to a small verified standard-library replacement.
