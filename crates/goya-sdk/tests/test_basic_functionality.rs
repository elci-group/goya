//! Basic functionality tests for Phase 1 Android Emitter Implementation
use goya_sdk::*;

#[test]
fn test_input_preview_basic() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
        }
    }"#;
    
    let kotlin = emit_android_input_preview(source).expect("Should emit input preview");
    assert!(kotlin.contains("GoyaRootInputPreview"));
    assert!(kotlin.contains("context: Context"));
}

#[test]
fn test_scrollable_preview_basic() {
    let source = r#"@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
        }
    }"#;
    
    let kotlin = emit_android_scrollable_preview(source).expect("Should emit scrollable preview");
    assert!(kotlin.contains("GoyaScrollablePreview"));
    assert!(kotlin.contains("ScrollView"));
}

#[test]
fn test_enhanced_d1_runtime_basic() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
        }
        @interaction submit {
            trigger: button.submit.activate;
            class: D1;
            state {
                read user_count: int;
            }
            input {
                username: text;
            }
            set user_count = state.user_count + 1;
        }
        @button submit {
            id: "submit";
            text: "Submit";
        }
    }"#;
    
    let kotlin = emit_android_d1_runtime_with_form(source).expect("Should emit enhanced D1 runtime");
    assert!(kotlin.contains("GoyaD1RuntimeForm"));
    assert!(kotlin.contains("formInputs"));
}

#[test]
fn test_backward_compatibility() {
    let source = r#"@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
        }
    }"#;
    
    let kotlin = emit_android_static_text_preview(source).expect("Should emit static text preview");
    assert!(kotlin.contains("GoyaStaticTextPreview"));
    assert!(kotlin.contains("text = \"Hello World\""));
}