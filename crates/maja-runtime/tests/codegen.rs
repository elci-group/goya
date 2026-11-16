use maja_runtime::{emit_checked_increment, CodegenError};

const SOURCE: &str = r#"@ontology app { @cell root { id: "root" @button inc { id: "button.inc" } } @interaction add { id: "action.add"; trigger: button.inc.activate; class: D1; state { read count: int; write count: int; } input { } set count = state.count + 1; } }"#;

#[test]
fn emits_stable_pure_specialization_for_canonical_action() {
    let scene = goya_lang::parse(SOURCE).unwrap();
    let action = emit_checked_increment(&scene).unwrap();
    assert_eq!(action.action_id, "action.add");
    assert_eq!(action.state_key, "count");
    assert_eq!(action, emit_checked_increment(&scene).unwrap());
    assert!(action.javascript.contains("value + (1n)"));
    assert!(action.javascript.contains("arithmetic-overflow"));
    assert!(action.javascript.contains("revision-overflow"));
}

#[test]
fn rejects_unadmitted_or_unvalidated_action_shapes() {
    let input = SOURCE.replace(
        "input { } set count = state.count + 1;",
        "input { amount: int; } set count = state.count + input.amount;",
    );
    assert_eq!(
        emit_checked_increment(&goya_lang::parse(&input).unwrap()).unwrap_err(),
        CodegenError::UnsupportedShape
    );
    let no_id = SOURCE.replace("id: \"action.add\";", "");
    assert_eq!(
        emit_checked_increment(&goya_lang::parse(&no_id).unwrap()).unwrap_err(),
        CodegenError::MissingActionIdentity
    );
    let broken_trigger = SOURCE.replace("button.inc.activate", "button.missing.activate");
    assert!(matches!(
        emit_checked_increment(&goya_lang::parse(&broken_trigger).unwrap()),
        Err(CodegenError::CanonicalValidation(_))
    ));
}
