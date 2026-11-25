# Hot reload — frozen contract

Status: frozen for the swarm that implements it. Agents implement this contract. They do not renegotiate it.

Date: 2026-11-25. Crate: `crates/goya-resolver`. Contract string stays `GY-RESOLVER/0.1`.

This note freezes headless reload and the CLI watcher that calls it. `--watch` polls the same file, compares bytes, and calls `Instance::reload`. A rejected replacement is printed and does not swap. The process keeps running. It does not open a surface. `--benchmark` still exits 2. `--trace` reports sites and layout generations and is not a timer.

## Behaviour

`Instance::reload(&mut self, source: &str) -> Result<ReloadReport, Diagnostic>`

A failed reload leaves the instance untouched: phase, revision, state values, field text, dispatch count, layout cache, and application identity.

Allowed phases: `initialising`, `running`, `suspended`. `terminated` and `failed` return `GY-RESOLVER-PHASE`.

Steps, and any error before the final swap aborts the swap:

1. Reject source over `max_source_bytes` with `GY-RESOLVER-BUDGET`.
2. Admit with the contract the running application already uses (`SemanticContract::parse` of its label). Do not auto-select a different profile.
3. Prepare. A D2 refusal returns `GY-RESOLVER-D2` with the existing refusal text.
4. Lower. `UnsupportedGesture` records IR unavailable. Any other lower error is `GY-RESOLVER-LOWER`.
5. Classify state. Same key and same value kind (`int`, `text`, `bool`) keeps the current value. A new key takes the authored initial. A key absent from the replacement is dropped. A kind change is `GY-RESOLVER-RELOAD` and the message is `state key {key} changed from {old} to {new}`. Its range is that key's declaration in the replacement source.
6. Restore the replacement owner with the classified D1 values and the **current** revision. The revision does not reset and does not bump.
7. Preserve field text for ids that still exist when the text fits the replacement `max-length`. A preserved field that exceeds the new max is `GY-RESOLVER-FIELD` and does not swap. New fields take authored seed text. Removed fields disappear.
8. Swap the application, owner, rest map, and fields. Clear the cached frame. Do not reset `layout_generations` or `dispatches`. The next `frame()` calls layout again because geometry may have changed.

`ReloadReport` lists `preserved_keys`, `initialised_keys`, `dropped_keys`, and `preserved_fields`, each sorted.

Component ids are recomputed for the replacement source. They are not stable across a reload.

## CLI

`goya-resolve --watch <file>` starts the instance, prints the summary, and then polls that file every 50 ms. The interval is a poll, not a measured budget. A change is the file bytes, not the modification time. The snapshot is taken from the bytes that were admitted, before the summary is printed. The new bytes must be UTF-8. `Instance::reload` applies them. Success prints:

`reload revision {n} preserved {keys-or-dash} initialised {keys-or-dash} dropped {keys-or-dash}`

The three lists are the sorted key lists from `ReloadReport`. An empty list is `-`. A read error, a non-UTF-8 file, or a failed reload is printed on stderr and does not swap. The same bytes are not reported twice. The process keeps running until it is killed. `--validate-only` with `--watch` exits 2 (`--validate-only does not watch`). `--benchmark` still exits 2. With `--trace`, a successful reload also prints `trace reload revision {n} layout {g}` before the closed surface is printed again. `layout` is the generation counter, which reload does not reset. With `--present`, a successful reload prints the closed surface again. `opened` stays false. No window is updated.

## File ownership

| Owner | Path |
| --- | --- |
| J1 | `crates/goya-resolver/src/execution.rs` |
| J2 | `crates/goya-resolver/src/reload.rs` |
| J3 | `crates/goya-resolver/src/lib.rs` |
| J4 | `crates/goya-resolver/tests/reload.rs` |

Each agent acquires a locksmith lease on its path before editing and releases it after. No agent runs Cargo. The parent formats, tests, and clippy.

## Evidence

Locksmith 0.1.4 is installed at `~/.local/bin/locksmith` by `baby --user`. The baby recipe names only that binary, so `locksmithd` was built with `cargo build --release -p locksmithd` and installed beside it. `systemctl --user` unit `locksmithd.service` binds `127.0.0.1:7700` and stores leases in `~/.local/share/locksmith/locksmith.db`.

The four agents held exclusive leases as `goya-resolver-j1` through `goya-resolver-j4`. The lib.rs lease expired once because its heartbeat lapsed, and that agent acquired it again before releasing. The parent lease on the test file also expired with the 90s heartbeat timeout.

Parent review: the one-line D2 fixture did not admit under the running GSSH/4-live contract (`GY-E1008`, count was used and not declared). The test scene now declares `count` and a root button, so a compiled D2 replacement returns `GY-RESOLVER-D2` and leaves count at 1.

| Command | Result |
| --- | --- |
| `cargo fmt -p goya-resolver` | Exit 0 |
| `CARGO_TARGET_DIR=/tmp/goya-resolver-reload cargo test -p goya-resolver --offline --lib --tests --bins` | 22 passed, 0 failed |
| `CARGO_TARGET_DIR=/tmp/goya-resolver-reload cargo clippy -p goya-resolver --offline --all-targets --no-deps -- -D warnings` | Exit 0 |
