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
revision increments. Invalid UTF-16 input is rejected as `INVALID_TEXT` before
UTF-8 byte accounting. Failures are returned as `DispatchResult.Failure` with
an `ErrorCode`; failures leave both state and revision unchanged. Initial
state schema/type/budget errors are rejected by construction. The runtime
does not provide persistence, migration, input-widget binding, or UI layout.
Kotlin compilation and Android/device qualification remain separate gates.
