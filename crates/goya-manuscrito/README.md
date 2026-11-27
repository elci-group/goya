# Goya Manuscrito

Manuscrito is the initial, analysis-only TypeScript project inventory for the
Goya workspace. It establishes the crate and machine-readable report contract
before semantic translation is attempted.

```sh
cargo run -p goya-manuscrito -- analyse ./project
MANUSCRITO_TYPESCRIPT_PATH=/trusted/tools/typescript-5.4.5/lib/typescript.js \
  cargo run -p goya-manuscrito -- graph ./project
MANUSCRITO_TYPESCRIPT_PATH=/trusted/tools/typescript-5.4.5/lib/typescript.js \
  cargo run -p goya-manuscrito -- gaps ./project
```

The command emits deterministic JSON with discovered `.ts`, `.tsx`, `.mts`,
`.cts`, `.d.ts`, `.d.mts`, `.d.cts`, and JavaScript files, `tsconfig` files,
package manifests, supported lockfile names, and declared dependencies from the
root `package.json`. Dependencies are deliberately reported as
`unclassified`; this inventory does not parse lockfile contents, resolve
workspaces, import graphs, package versions, or package substitutions. It
does not execute source or package lifecycle scripts and skips symlinks,
`node_modules`, and common generated output directories.

The inventory report schema is in `schemas/analysis-report.schema.json`. It
always states `mode: analysis-only` and `translated: false`. The `analyse`
command does not parse TypeScript, resolve types, produce MIR, emit `.gscene`,
or claim behavioral equivalence. Goya's current `.gscene` grammar is scene/UI oriented;
backend language capabilities and canonical lowering require a separate
design and validation effort. No T3 Code source revision is present in this
repository, so T3 acceptance is not claimed.

`graph` invokes the TypeScript 5.4.5 compiler API supplied through
`MANUSCRITO_TYPESCRIPT_PATH`. The path must refer to a trusted Manuscrito tool
installation; the analyzed project’s `node_modules` is never loaded as
compiler code. It reads `tsconfig.json`, creates a compiler program without
emitting output, and reports project source modules plus import, re-export,
literal dynamic-import, and literal `require` edges with source spans and
resolved paths. Nonliteral dynamic imports, `require`, and `module.require`
appear as unresolved loader boundaries instead of disappearing. It also
reports top-level declarations, their checker-resolved
type strings, function parameter and return types, and TypeScript compiler
diagnostics. It does not install dependencies, execute package scripts,
or emit translated code. Missing TypeScript, an unsupported version, invalid
configuration, and unresolved edges are reported explicitly. The graph schema
is in `schemas/graph-report.schema.json`. The report includes a versioned
`goya.manuscrito.mir/1` extraction containing source-bound function, branch,
iteration, suspension, throw, return, invocation, mutation, and binding nodes.
The Rust `MirReport` and `MirNode` types validate the version and node shape
before the CLI returns a successful graph report.
It walks module statements and nested function declarations, expressions,
arrows, methods, constructors, and accessors. It is an AST-derived inventory:
invocation effects remain unclassified, and
this MIR does not claim full runtime semantics or lowering equivalence. A
separate capability observation list recognizes selected Node built-in module
imports at call sites (filesystem, processes, networking, cryptography,
environment, streams, scheduling, terminal, and workers). This is API-name
classification only; it does not prove equivalent behavior or operation
semantics, and unknown calls remain unclassified. The report also includes
strongly connected components and cycles among resolved in-project modules;
external and unresolved edges are excluded from SCC analysis. Reachable package
imports are aggregated with resolved TypeScript package versions when available
and the root manifest constraint when declared. They remain `unclassified` and
`not-translated`. For npm `package-lock.json` v2/v3, the report also walks the
transitive dependency closure from resolved import instances, retaining
install paths, versions, npm integrity strings, and dev/optional/peer flags.
Missing entries, workspace links, root manifest/lock mismatches, and missing or
malformed integrity strings are reported. Reached package nodes retain their
source importers across transitive traversal. Integrity values are recorded
but package bytes are not fetched or checked, so lockfile integrity is not
verified and no substitution is claimed.
Non-literal dynamic loading, custom resolution hooks, nested declaration
graphs, and runtime-loaded modules remain outside the reported edge set.

`gaps` emits a machine-readable comparison between extracted MIR domains and
the current canonical `GY-LANG/1` / GSSH scene profiles. It explicitly marks
backend functions, general control flow, async tasks, exceptions, resources,
and backend capabilities unsupported, while identifying interaction-scoped
effects and state assignments as domain-limited support. It never emits
`.gscene`; the schema is `schemas/gscene-gap-report.schema.json`.

## Tests

`cargo test -p goya-manuscrito` is hermetic by default: it covers the
inventory, the Rust MIR/1 contract, and CLI argument handling without Node.js.
The compiler-graph and gap-report end-to-end tests additionally require a
trusted TypeScript 5.4.5 installation and only run when its
`lib/typescript.js` path is provided:

```sh
MANUSCRITO_TEST_TYPESCRIPT_PATH=/trusted/tools/typescript-5.4.5/lib/typescript.js \
  cargo test -p goya-manuscrito
```

Translation should only begin after backend semantics can be represented in
canonical `.gscene` and validated through the standard Goya language pipeline.
