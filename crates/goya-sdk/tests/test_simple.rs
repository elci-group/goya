//! Simple test to verify the enhanced Android emission capabilities
use goya_sdk::*;

#[test]
fn test_android_input_preview() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Enter username";
        }
    }"#;
    
    let kotlin = emit_android_input_preview(source).expect("Should emit input preview");
    assert!(kotlin.contains("GoyaRootInputPreview"));
    assert!(kotlin.contains("context: Context"));
    assert!(kotlin.contains("inputType"));
}

#[test]
fn test_android_scrollable_preview() {
    let source = r#"@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
        }
    }"#;
    
    let kotlin = emit_android_scrollable_preview(source).expect("Should emit scrollable preview");
    assert!(kotlin.contains("GoyaScrollablePreview"));
    assert!(kotlin.contains("ScrollView"));
    assert!(kotlin.contains("contentLayout"));
}

#[test]
fn test_enhanced_d1_runtime() {
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
    assert!(kotlin.contains("validateFormInputs"));
    assert!(kotlin.contains("ErrorCode.VALIDATION"));
}

#[test]
fn test_named_enhanced_d1_runtime() {
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
    
    let kotlin = emit_android_d1_runtime_with_form_named(source, "MyCustomFormRuntime").expect("Should emit named enhanced D1 runtime");
    assert!(kotlin.contains("MyCustomFormRuntime"));
    assert!(kotlin.contains("formInputs"));
}

#[test]
fn test_backward_compatibility() {
    // Test static text preview
    let text_source = r#"@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
        }
    }"#;
    
    let kotlin = emit_android_static_text_preview(text_source).expect("Should emit static text preview");
    assert!(kotlin.contains("GoyaStaticTextPreview"));
    assert!(kotlin.contains("text = \"Hello World\""));
    
    // Test button preview
    let button_source = r#"@ontology demo {
        @button go {
            id: "go";
            text: "Go";
        }
        @interaction increment {
            trigger: button.go.activate;
            class: D1;
            state {
                read count: int;
                write count: int;
            }
            input {
                amount: int;
            }
            set count = state.count + input.amount;
        }
    }"#;
    
    let kotlin = emit_android_root_button_preview(button_source).expect("Should emit button preview");
    assert!(kotlin.contains("GoyaRootButtonPreview"));
    assert!(kotlin.contains("onD1Action"));
}