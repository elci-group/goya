use goya_sdk::emit_android_d1_runtime_mapped;
use std::collections::BTreeMap;

const SOURCE: &str = r#"@ontology counter {
    @button plus { id: "button.plus"; }
    @button label { id: "button.label"; }
    @button enabled { id: "button.enabled"; }
    @interaction increment {
        id: "action.increment.v1";
        trigger: button.plus.activate;
        class: D1;
        state { read count: int; write count: int; }
        input { amount: int; }
        set count = state.count + input.amount;
    }
    @interaction set_label {
        id: "action.label.v1";
        trigger: button.label.activate;
        class: D1;
        state { write label: text; }
        input { value: text; }
        set label = input.value;
    }
    @interaction set_enabled {
        id: "action.enabled.v1";
        trigger: button.enabled.activate;
        class: D1;
        state { write enabled: bool; }
        input { value: bool; }
        set enabled = input.value;
    }
}"#;

#[test]
fn mapped_runtime_is_deterministic_and_binds_the_admitted_d1_surface() {
    let first = emit_android_d1_runtime_mapped(SOURCE, "CounterRuntime").unwrap();
    let second = emit_android_d1_runtime_mapped(SOURCE, "CounterRuntime").unwrap();
    assert_eq!(first, second);
    assert_eq!(first.source_digest, second.source_digest);
    assert!(first.kotlin.contains("class CounterRuntime"));
    assert!(first.kotlin.contains("action.increment.v1"));
    assert!(first.kotlin.contains("action.label.v1"));
    assert!(first.kotlin.contains("action.enabled.v1"));
    assert!(
        first.kotlin.contains("checkedAdd"),
        "Long addition must reject overflow"
    );
    assert!(
        first.kotlin.contains("@Synchronized"),
        "dispatch must serialize state transitions"
    );
    assert!(first.kotlin.contains("toByteArray(Charsets.UTF_8)"));
    assert!(first.kotlin.contains("registerBytes += registerCost"));
    assert!(first.kotlin.contains("MAX_SNAPSHOT_BYTES = 1048576"));
    assert!(first.kotlin.contains("MAX_REGISTER_BYTES = 1048576"));
    assert_eq!(first.action_ranges.len(), 3);
    for (action_id, range) in &first.action_ranges {
        assert!(range.start < range.end);
        assert!(range.end <= SOURCE.len());
        assert!(SOURCE.is_char_boundary(range.start));
        assert!(SOURCE.is_char_boundary(range.end));
        assert!(SOURCE[range.start..range.end].contains(action_id));
    }
    let revised =
        emit_android_d1_runtime_mapped(&SOURCE.replace("count", "total"), "CounterRuntime")
            .unwrap();
    assert_ne!(first.source_digest, revised.source_digest);
}

#[test]
fn android_runtime_rejects_invalid_class_names_and_non_d1_or_unsupported_source() {
    assert!(matches!(
        emit_android_d1_runtime_mapped(SOURCE, "Counter-Runtime"),
        Err(goya_sdk::AndroidD1RuntimeError::InvalidClassName)
    ));
    let d2 = SOURCE.replace("class: D1;", "class: D2;");
    assert!(emit_android_d1_runtime_mapped(&d2, "CounterRuntime").is_err());
    let no_authored_ids = SOURCE.replace("id: \"action.increment.v1\";", "");
    assert!(emit_android_d1_runtime_mapped(&no_authored_ids, "CounterRuntime").is_err());
    assert!(matches!(
        emit_android_d1_runtime_mapped(SOURCE, "object"),
        Err(goya_sdk::AndroidD1RuntimeError::InvalidClassName)
    ));
}

#[test]
fn android_runtime_refuses_state_names_that_collapse_to_one_kotlin_identifier() {
    const COLLIDING_KEYS: &str = r#"@ontology collisions {
        @button go { id: "button.go"; }
        @interaction set_both {
            id: "action.set-both";
            trigger: button.go.activate;
            class: D1;
            state { write alpha-beta: int; write alpha_beta: int; }
            input { }
            set alpha-beta = 1;
            set alpha_beta = 2;
        }
    }"#;
    let generated = emit_android_d1_runtime_mapped(COLLIDING_KEYS, "CollisionRuntime").unwrap();
    assert!(generated.kotlin.contains("val s_k_616c7068612d62657461"));
    assert!(generated.kotlin.contains("val s_k_616c7068615f62657461"));
}

#[test]
fn android_runtime_constructor_classifies_initial_text_and_snapshot_limits() {
    let generated = emit_android_d1_runtime_mapped(SOURCE, "CounterRuntime").unwrap();
    assert!(generated.kotlin.contains(
        "if (!validUnicode(initialState)) throw IllegalArgumentException(\"INVALID_TEXT\")"
    ));
    assert!(generated.kotlin.contains(
        "if (!withinTextLimit(initialState)) throw IllegalArgumentException(\"TEXT_LIMIT\")"
    ));
    assert!(generated.kotlin.contains(
        "if (mapSize(initialState) > MAX_SNAPSHOT_BYTES) throw IllegalArgumentException(\"SNAPSHOT_LIMIT\")"
    ));
}

#[test]
fn generated_ranges_are_valid_utf8_boundaries_and_point_into_their_sources() {
    let source = SOURCE.replace("counter", "cøunter");
    let generated = emit_android_d1_runtime_mapped(&source, "CounterRuntime").unwrap();
    for (action_id, range) in generated.action_ranges {
        assert!(range.start <= range.end);
        assert!(range.end <= source.len());
        assert!(source.is_char_boundary(range.start));
        assert!(source.is_char_boundary(range.end));
        assert!(source[range.start..range.end].contains(&action_id));
    }
}

#[test]
fn emitted_kotlin_dispatch_matches_d1_transition_and_failure_atomicity_when_toolchain_exists() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    let generated = emit_android_d1_runtime_mapped(SOURCE, "CounterRuntime").unwrap();
    let mut states = String::new();
    let mut assignments = String::new();
    for index in 0..17 {
        states.push_str(&format!("write text_{index}: text; "));
        assignments.push_str(&format!("set text_{index} = input.value; "));
    }
    let aggregate_source = format!(
        "@ontology aggregate {{ @button go {{ id: \"button.go\"; }} @interaction fill {{ id: \"action.fill\"; trigger: button.go.activate; class: D1; state {{ {states} }} input {{ value: text; }} {assignments} }} }}"
    );
    let aggregate = emit_android_d1_runtime_mapped(&aggregate_source, "AggregateRuntime").unwrap();
    let kotlin = format!("{}\n{}", generated.kotlin, aggregate.kotlin);

    let kotlin_required = std::env::var("ARAGON_REQUIRE_KOTLIN").as_deref() == Ok("1");
    let toolchain_available = ["kotlinc", "java"].iter().all(|tool| {
        Command::new(tool)
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
    });
    if !toolchain_available {
        if kotlin_required {
            panic!("Kotlin execution conformance required by ARAGON_REQUIRE_KOTLIN=1, but kotlinc/java are missing or their -version checks failed");
        }
        eprintln!("Kotlin execution conformance skipped: kotlinc/java are unavailable or their -version checks failed");
        return;
    }

    let harness = r#"
fun main() {
    val malformed = String(charArrayOf(0xD800.toChar()))
    val invalidInitialText = runCatching { CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(0L),
        "label" to CounterRuntime.Value.TextValue(malformed),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    )) }.exceptionOrNull()
    check(invalidInitialText is IllegalArgumentException && invalidInitialText.message == "INVALID_TEXT")

    val tooLongInitialText = "x".repeat(65537)
    val oversizedInitialText = runCatching { CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(0L),
        "label" to CounterRuntime.Value.TextValue(tooLongInitialText),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    )) }.exceptionOrNull()
    check(oversizedInitialText is IllegalArgumentException && oversizedInitialText.message == "TEXT_LIMIT")

    val maxText = "é".repeat(32768)
    check(maxText.toByteArray(Charsets.UTF_8).size == 65536)
    val boundaryInitialState = CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(0L),
        "label" to CounterRuntime.Value.TextValue(maxText),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    ))
    check(boundaryInitialState.state["label"] == CounterRuntime.Value.TextValue(maxText))

    val aggregateInitialState = (0 until 17).associate { index ->
        "text_$index" to AggregateRuntime.Value.TextValue(maxText)
    }
    val oversizedSnapshot = runCatching { AggregateRuntime(aggregateInitialState) }.exceptionOrNull()
    check(oversizedSnapshot is IllegalArgumentException && oversizedSnapshot.message == "SNAPSHOT_LIMIT")

    val rt = CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(4L),
        "label" to CounterRuntime.Value.TextValue("start"),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    ))
    check(rt.dispatch("action.increment.v1", mapOf("amount" to CounterRuntime.Value.IntValue(3L))) == CounterRuntime.DispatchResult.Success(1uL))
    check(rt.state["count"] == CounterRuntime.Value.IntValue(7L) && rt.revision == 1uL)
    val committed = rt.state
    check(rt.dispatch("action.increment.v1", mapOf("amount" to CounterRuntime.Value.IntValue(1L), "extra" to CounterRuntime.Value.BoolValue(true))) == CounterRuntime.DispatchResult.Failure(CounterRuntime.ErrorCode.INPUT_SCHEMA))
    check(rt.state == committed && rt.revision == 1uL)
    check(rt.dispatch("action.label.v1", mapOf("value" to CounterRuntime.Value.TextValue("café 🧪"))) == CounterRuntime.DispatchResult.Success(2uL))
    check(rt.dispatch("action.label.v1", mapOf("value" to CounterRuntime.Value.TextValue(maxText))) == CounterRuntime.DispatchResult.Success(3uL))
    val afterUnicode = rt.state
    val tooLong = "é".repeat(32769)
    check(rt.dispatch("action.label.v1", mapOf("value" to CounterRuntime.Value.TextValue(tooLong))) == CounterRuntime.DispatchResult.Failure(CounterRuntime.ErrorCode.TEXT_LIMIT))
    check(rt.state == afterUnicode && rt.revision == 3uL)
    val overflow = CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(Long.MAX_VALUE),
        "label" to CounterRuntime.Value.TextValue("start"),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    ))
    check(overflow.dispatch("action.increment.v1", mapOf("amount" to CounterRuntime.Value.IntValue(1L))) == CounterRuntime.DispatchResult.Failure(CounterRuntime.ErrorCode.ARITHMETIC_OVERFLOW))
    check(overflow.state["count"] == CounterRuntime.Value.IntValue(Long.MAX_VALUE) && overflow.revision == 0uL)
    check(rt.dispatch("action.enabled.v1", mapOf("value" to CounterRuntime.Value.BoolValue(true))) == CounterRuntime.DispatchResult.Success(4uL))
    val minimum = CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(0L),
        "label" to CounterRuntime.Value.TextValue("start"),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    ))
    check(minimum.dispatch("action.increment.v1", mapOf("amount" to CounterRuntime.Value.IntValue(Long.MIN_VALUE))) == CounterRuntime.DispatchResult.Success(1uL))
    check(minimum.state["count"] == CounterRuntime.Value.IntValue(Long.MIN_VALUE))
    val revisionEdge = CounterRuntime(mapOf(
        "count" to CounterRuntime.Value.IntValue(0L),
        "label" to CounterRuntime.Value.TextValue("start"),
        "enabled" to CounterRuntime.Value.BoolValue(false)
    ), ULong.MAX_VALUE)
    val revisionState = revisionEdge.state
    check(revisionEdge.dispatch("action.increment.v1", mapOf("amount" to CounterRuntime.Value.IntValue(1L))) == CounterRuntime.DispatchResult.Failure(CounterRuntime.ErrorCode.REVISION_OVERFLOW))
    check(revisionEdge.state == revisionState && revisionEdge.revision == ULong.MAX_VALUE)
    val malformedResult = rt.dispatch("action.label.v1", mapOf("value" to CounterRuntime.Value.TextValue(malformed)))
    check(malformedResult == CounterRuntime.DispatchResult.Failure(CounterRuntime.ErrorCode.INVALID_TEXT))
    check(rt.revision == 4uL)
}
"#;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("goya-d1-kotlin-{nonce}"));
    fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("Conformance.kt");
    let jar_path = dir.join("conformance.jar");
    fs::write(&source_path, format!("{kotlin}\n{harness}")).unwrap();
    let compile = Command::new("kotlinc")
        .arg(&source_path)
        .arg("-include-runtime")
        .arg("-d")
        .arg(&jar_path)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "kotlinc failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("java")
        .arg("-jar")
        .arg(&jar_path)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "generated Kotlin conformance failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn authored_interactions_match_the_canonical_goya_d1_executor() {
    use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
    use goya_lang::{compile_d1_interactions, parse};

    let compiled = compile_d1_interactions(&parse(SOURCE).unwrap()).unwrap();
    let by_id = |id: &str| compiled.iter().find(|i| i.name() == id).unwrap();
    let count = by_id("increment")
        .execute(
            &D1Snapshot::new(0, BTreeMap::from([("count".into(), D1Value::Int(4))])).unwrap(),
            &InputSnapshot::new(BTreeMap::from([("amount".into(), D1Value::Int(3))])).unwrap(),
        )
        .unwrap();
    assert_eq!(count.next_state["count"], D1Value::Int(7));

    let minimum = by_id("increment")
        .execute(
            &D1Snapshot::new(0, BTreeMap::from([("count".into(), D1Value::Int(0))])).unwrap(),
            &InputSnapshot::new(BTreeMap::from([("amount".into(), D1Value::Int(i64::MIN))]))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(minimum.next_state["count"], D1Value::Int(i64::MIN));

    let label = by_id("set_label")
        .execute(
            &D1Snapshot::new(
                0,
                BTreeMap::from([("label".into(), D1Value::Text("start".into()))]),
            )
            .unwrap(),
            &InputSnapshot::new(BTreeMap::from([(
                "value".into(),
                D1Value::Text("café 🧪".into()),
            )]))
            .unwrap(),
        )
        .unwrap();
    assert_eq!(label.next_state["label"], D1Value::Text("café 🧪".into()));

    let enabled = by_id("set_enabled")
        .execute(
            &D1Snapshot::new(
                0,
                BTreeMap::from([("enabled".into(), D1Value::Bool(false))]),
            )
            .unwrap(),
            &InputSnapshot::new(BTreeMap::from([("value".into(), D1Value::Bool(true))])).unwrap(),
        )
        .unwrap();
    assert_eq!(enabled.next_state["enabled"], D1Value::Bool(true));
}

#[test]
fn input_free_actions_emit_explicitly_typed_kotlin_collections() {
    // Kotlin cannot infer `setOf()` or `mapOf()` with no elements. Compiling
    // the generated root-button project exposed this; keep it typed.
    let source = r#"@ontology demo {
        @button increment { id: "button.increment"; text: "Increment" }
        @interaction increment {
            id: "action.increment";
            trigger: button.increment.activate;
            class: D1;
            state { write count: int; }
            input { }
            set count = 1;
        }
    }"#;
    let runtime = emit_android_d1_runtime_mapped(source, "GoyaD1Runtime").unwrap();
    assert!(runtime.kotlin.contains("inputs.keys != setOf<String>()"));
    assert!(runtime.kotlin.contains("val snapshot = mapOf<String, Value>("));
    assert!(!runtime.kotlin.contains("setOf()"));
    assert!(!runtime.kotlin.contains("= mapOf()"));
}
