# GY-TOV/1 — Tov compiler adapter

`goya-tov` provides a small process boundary around Tov's documented CLI. It
supports `tov check PATH` and `tov build PATH -o OUTPUT`, passing every path as
its own operating-system argument and never through a shell. `PATH` may name a
single `.tov` file or a Tov project directory.

```rust,no_run
use goya_tov::TovAdapter;

let tov = TovAdapter::new(); // resolves `tov` via PATH
let check = tov.check("app.tov")?;
let build = tov.build("app.tov", "target/app")?;
assert!(build.artifact.is_some());
# Ok::<(), Box<dyn std::error::Error>>(())
```

Use `TovAdapter::with_executable` to select a particular Tov installation.
When the CLI is unavailable the adapter returns `TovError::ToolUnavailable`
with an installation hint. Non-zero compiler status returns `TovError::Failed`
with stdout/stderr; build success is accepted only when the requested output
exists as a non-empty regular file. An output path that is empty, has no
parent directory, or names the input (including `.` / `..` forms and an
existing symlink to that input) is `TovError::InvalidOutput` and does not
launch the compiler.

This is a compiler-process adapter, not a Rust linkage to Tov's internals.
That boundary avoids a machine-specific path dependency and tracks the public
CLI surface documented by Tov. Tov remains a separately installed tool; no
Tov binary is vendored or silently downloaded by Goya.
