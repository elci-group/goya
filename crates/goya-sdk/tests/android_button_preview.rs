use goya_sdk::{
    emit_android_root_button_preview, emit_android_root_button_preview_mapped,
    AndroidRootButtonPreviewError,
};

const SOURCE: &str = "@ontology demo { @button increment { id: \"button.increment\"; text: \"Add $one\" } @interaction add { id: \"action.add\"; trigger: button.increment.activate; class: D1; state { read count: int; write count: int; } input { } set count = state.count + 1; } }";

#[test]
fn emits_root_button_with_validated_authored_d1_callback_and_source_map() {
    let preview = emit_android_root_button_preview_mapped(SOURCE).unwrap();
    assert_eq!(preview.button_id, "button.increment");
    assert_eq!(preview.action_id, "action.add");
    assert!(preview.kotlin.contains("class GoyaRootButtonPreview(context: Context, onD1Action: (String) -> Unit) : Button(context)"));
    assert!(preview.kotlin.contains("text = \"Add \\$one\""));
    assert!(preview
        .kotlin
        .contains("setOnClickListener { onD1Action(\"action.add\") }"));
    assert_eq!(
        &SOURCE[preview.source_range.start..preview.source_range.end],
        "@interaction add { id: \"action.add\"; trigger: button.increment.activate; class: D1; state { read count: int; write count: int; } input { } set count = state.count + 1; }"
    );
    assert_eq!(
        &preview.kotlin[preview.generated_range.start..preview.generated_range.end],
        "\"action.add\""
    );
    assert_eq!(
        preview.source_digest,
        goya_lang::handoff::admit(SOURCE)
            .unwrap()
            .parsed
            .source_digest
    );
    assert_eq!(
        emit_android_root_button_preview(SOURCE).unwrap(),
        preview.kotlin
    );
}

#[test]
fn strict_rejection_covers_unbound_or_unsupported_callbacks_and_visuals() {
    let with_input = "@ontology demo { @button b { id: \"b\"; text: \"Go\" } @interaction a { id: \"a\"; trigger: button.b.activate; class: D1; state { write value: int; } input { value: int; } set value = input.value; } }";
    for source in [
        "@ontology demo { @button b { id: \"b\"; text: \"Go\" } }",
        with_input,
        "@ontology demo { @button b { id: \"b\"; text: \"Go\" } @interaction a { id: \"a\"; trigger: button.b.activate; class: D2; state { } input { } } }",
        "@ontology demo { @button b { id: \"b\"; text: \"Go\"; extent: (100, 40, 0) } @interaction a { id: \"a\"; trigger: button.b.activate; class: D1; state { } input { } } }",
        "@ontology demo { @button b { id: \"b\"; text: \"Go\"; @cell child { id: \"child\"; } } @interaction a { id: \"a\"; trigger: button.b.activate; class: D1; state { } input { } } }",
    ] {
        assert!(matches!(
            emit_android_root_button_preview(source),
            Err(AndroidRootButtonPreviewError::Unsupported(_))
                | Err(AndroidRootButtonPreviewError::InvalidD1(_))
                | Err(AndroidRootButtonPreviewError::StrictParse(_))
        ), "unexpected acceptance: {source}");
    }
    assert!(matches!(
        emit_android_root_button_preview("@ontology demo { @button b { text: \"Go\" } }"),
        Err(AndroidRootButtonPreviewError::StrictParse(_))
    ));
}

#[test]
fn rejects_more_than_one_d1_action() {
    let source = "@ontology demo { @button b { id: \"b\"; text: \"Go\" } @interaction a { id: \"a\"; trigger: button.b.activate; class: D1; state { } input { } } @interaction c { id: \"c\"; trigger: button.b.activate; class: D1; state { } input { } } }";
    assert!(matches!(
        emit_android_root_button_preview(source),
        Err(AndroidRootButtonPreviewError::Unsupported(_))
            | Err(AndroidRootButtonPreviewError::StrictParse(_))
    ));
}
