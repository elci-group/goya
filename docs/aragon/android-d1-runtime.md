# Aragon Kotlin D1 runtime emitter

`goya_sdk::emit_android_d1_runtime` emits a standalone Kotlin class named
`GoyaD1Runtime`. The mapped form accepts a Kotlin class name and returns the
exact source digest plus a source range for each emitted action. It strictly
admits the source, compiles every interaction with Goya's canonical D1
compiler, and rejects D2, effects, missing action IDs, invalid schemas, and
other unsupported declarations before emission.

The generated class supports the current GY-LANG D1 scalar model: signed
64-bit integers, UTF-8 text, booleans, state/input reads, literals, and checked
integer addition. Construct it with the complete explicit state map, using
`GoyaD1Runtime.Value.IntValue`, `TextValue`, or `BoolValue`; it does not create
default values. `requiredStateTypes` reports the exact key/type schema.
`dispatch(actionId, inputs)` requires an exact input map, evaluates into local
registers and staged writes, and updates the complete state and revision only
after all checks pass. Dispatch is synchronized. The optional initial revision
and snapshot revision use Kotlin `ULong`, preserving Goya's unsigned 64-bit
revision range; `snapshot()` reads the state and revision together.

The generated runtime applies the D1 limits for 256 state keys, 64 KiB UTF-8
text values, 1 MiB measured snapshots, 1 MiB live register storage, and checked
revision increments. During dispatch, malformed UTF-16 text returns
`INVALID_TEXT`; text exceeding the per-value limit returns `TEXT_LIMIT`.
Failures are returned as `DispatchResult.Failure` with an `ErrorCode` and leave
both state and revision unchanged. Construction rejects invalid state with
`IllegalArgumentException`: schema mismatch reports `STATE_SCHEMA`, type
mismatch reports `STATE_TYPE`, malformed UTF-16 reports `INVALID_TEXT`, a
text value above the per-value limit reports `TEXT_LIMIT`, and a state map
above the measured snapshot limit reports `SNAPSHOT_LIMIT`.

The opt-in strict Kotlin qualification runs the generated-runtime conformance
test and fails if its Kotlin/Java toolchain is unavailable:

```sh
bash scripts/check-android-d1-kotlin.sh
```

The test executes generated Kotlin for its covered dispatch and constructor
cases, including atomic failures and the revision boundary. A separate Rust
test exercises the canonical D1 executor; these checks are not a direct
differential comparison. This is a standalone Kotlin/JVM check, not an Android
SDK build or platform qualification. The runtime does not provide persistence,
migration, input-widget binding, or UI layout; Android compilation and device
qualification remain separate gates.
