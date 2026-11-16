//! Comprehensive test coverage for enhanced Android emission capabilities.
//!
//! Tests cover:
//! - Enhanced Android Input Preview with IME integration and validation
//! - Enhanced Android Scrollable Preview with viewport management
//! - Enhanced Android D1 Runtime with form state management
//! - Integration between parser-enhanced properties and Android components
//! - Gesture integration (focus, blur, select, submit)
//! - Real-time input validation and error handling
//! - Form submission validation and dispatch

use goya_sdk::*;

/// Test enhanced Android input preview with IME integration and validation.
#[test]
fn test_enhanced_android_input_preview_with_ime() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Enter username";
            max-length: 20;
        }
    }"#;

    // Test basic input preview emission
    let kotlin = emit_android_input_preview(source).expect("Should emit input preview");
    assert!(kotlin.contains("class GoyaStaticTextPreview(context: Context)"));
    assert!(kotlin.contains("inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_NORMAL"));
    assert!(kotlin.contains("hint = \"Enter username\""));
    assert!(kotlin.contains("setFilters(arrayOf(InputFilter.LengthFilter(20)))"));
    assert!(kotlin.contains("validateInput"));
    assert!(kotlin.contains("onFocusChanged"));
    assert!(kotlin.contains("submitForm"));
}

/// Test enhanced Android input preview with password input type.
#[test]
fn test_enhanced_android_input_preview_with_password() {
    let source = r#"@ontology demo {
        @cell password {
            id: "password";
            text: "Enter password";
            max-length: 12;
        }
    }"#;

    let kotlin = emit_android_input_preview(source).expect("Should emit password input preview");
    assert!(kotlin.contains("inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD"));
    assert!(kotlin.contains("setInputType(InputType.TYPE_TEXT_VARIATION_PASSWORD)"));
}

/// Test enhanced Android input preview with email input type.
#[test]
fn test_enhanced_android_input_preview_with_email() {
    let source = r#"@ontology demo {
        @cell email {
            id: "email";
            text: "Enter email";
        }
    }"#;

    let kotlin = emit_android_input_preview(source).expect("Should emit email input preview");
    assert!(kotlin.contains("inputType = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS"));
}

/// Test enhanced Android scrollable preview with viewport management.
#[test]
fn test_enhanced_android_scrollable_preview() {
    let source = r#"@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
            @cell content {
                id: "content";
                extent: (800, 600, 0);
            }
        }
    }"#;

    let kotlin = emit_android_scrollable_preview(source).expect("Should emit scrollable preview");
    assert!(kotlin.contains("class GoyaStaticTextPreview(context: Context)"));
    assert!(kotlin.contains("ScrollView(context)"));
    assert!(kotlin.contains("LinearLayout(context)"));
    assert!(kotlin.contains("setOnScrollChangeListener(this)"));
    assert!(kotlin.contains("getViewport()"));
    assert!(kotlin.contains("getContentSize()"));
    assert!(kotlin.contains("scrollTo(x: Int, y: Int)"));
}

/// Test enhanced Android D1 runtime with form state management.
#[test]
fn test_enhanced_android_d1_runtime_with_form() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
        }
        @interaction validate_username {
            trigger: button.submit.activate;
            class: D1;
            state {
                read user_count: int;
                write user_count: int;
            }
            input {
                username: text;
            }
            set user_count = state.user_count + 1;
        }
        @button submit {
            id: "submit";
            text: "Submit";
            position: (0, 0, 0);
            extent: (100, 40, 0);
        }
    }"#;

    let kotlin = emit_android_d1_runtime_with_form(source).expect("Should emit enhanced D1 runtime with form management");
    assert!(kotlin.contains("class GoyaD1RuntimeForm(initialState: Map<String, GoyaD1RuntimeForm.Value>, initialRevision: ULong = 0uL)"));
    assert!(kotlin.contains("formInputs: Map<String, Value> = mutableMapOf()"));
    assert!(kotlin.contains("gestureListeners: MutableMap<String, () -> Unit> = mutableMapOf()"));
    assert!(kotlin.contains("validationErrors: MutableMap<String, String?> = mutableMapOf()"));
    assert!(kotlin.contains("validateFormInputs"));
    assert!(kotlin.contains("applyInputGestures"));
    assert!(kotlin.contains("updateInputState"));
    assert!(kotlin.contains("ErrorCode.VALIDATION"));
    assert!(kotlin.contains("ErrorCode.FOCUS"));
    assert!(kotlin.contains("ErrorCode.BLUR"));
    assert!(kotlin.contains("ErrorCode.SELECT"));
    assert!(kotlin.contains("ErrorCode.SUBMIT"));
}

/// Test enhanced Android D1 runtime with named class.
#[test]
fn test_enhanced_android_d1_runtime_with_form_named() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
        }
        @interaction validate_username {
            trigger: button.submit.activate;
            class: D1;
            state {
                read user_count: int;
                write user_count: int;
            }
            input {
                username: text;
            }
            set user_count = state.user_count + 1;
        }
        @button submit {
            id: "submit";
            text: "Submit";
            position: (0, 0, 0);
            extent: (100, 40, 0);
        }
    }"#;

    let kotlin = emit_android_d1_runtime_with_form_named(source, "MyCustomFormRuntime").expect("Should emit enhanced D1 runtime with custom class name");
    assert!(kotlin.contains("class MyCustomFormRuntime(initialState: Map<String, MyCustomFormRuntime.Value>, initialRevision: ULong = 0uL)"));
}

/// Test backward compatibility with existing static text and button previews.
#[test]
fn test_backward_compatibility() {
    // Test static text preview
    let text_source = r#"@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
            text-size: 18;
            fill: #FF0000;
        }
    }"#;

    let kotlin = emit_android_static_text_preview(text_source).expect("Should emit static text preview");
    assert!(kotlin.contains("class GoyaStaticTextPreview(context: Context)"));
    assert!(kotlin.contains("text = \"Hello World\""));
    assert!(kotlin.contains("setTextSize(TypedValue.COMPLEX_UNIT_PT, 18.0f)"));
    assert!(kotlin.contains("setTextColor(android.graphics.Color.rgb(255, 0, 0))"));

    // Test button preview
    let button_source = r#"@ontology demo {
        @button go {
            id: "go";
            text: "Go";
            position: (0, 0, 0);
            extent: (100, 40, 0);
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
    assert!(kotlin.contains("class GoyaRootButtonPreview(context: Context, onD1Action: (String) -> Unit)"));
    assert!(kotlin.contains("text = \"Go\""));
}

/// Test integration between parser-enhanced input properties and Android components.
#[test]
fn test_parser_enhanced_input_integration() {
    let source = r#"@ontology demo {
        @cell complex_input {
            id: "complex_input";
            text: "Complex input with all properties";
            max-length: 50;
            text-size: 16;
            fill: #E0E0E0;
            anchor: middle;
        }
    }"#;

    let kotlin = emit_android_input_preview(source).expect("Should emit input with all parser-enhanced properties");
    assert!(kotlin.contains("max-length: 50"));
    assert!(kotlin.contains("text-size: 16"));
    assert!(kotlin.contains("fill: #E0E0E0"));
    assert!(kotlin.contains("anchor: middle"));
    assert!(kotlin.contains("setTextSize(TypedValue.COMPLEX_UNIT_PT, 16.0f)"));
    assert!(kotlin.contains("setTextColor(android.graphics.Color.parseColor(\"#E0E0E0\"))"));
}

/// Test gesture integration support in Android runtime.
#[test]
fn test_gesture_integration() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
        }
        @interaction validate_username {
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

    let kotlin = emit_android_d1_runtime_with_form(source).expect("Should emit runtime with gesture integration");
    assert!(kotlin.contains("gestureListeners: MutableMap<String, () -> Unit> = mutableMapOf()"));
    assert!(kotlin.contains("applyInputGestures"));
    assert!(kotlin.contains("addGestureListener"));
    assert!(kotlin.contains("ErrorCode.FOCUS"));
    assert!(kotlin.contains("ErrorCode.BLUR"));
    assert!(kotlin.contains("ErrorCode.SELECT"));
    assert!(kotlin.contains("ErrorCode.SUBMIT"));
}

/// Test form submission validation and error handling.
#[test]
fn test_form_submission_validation() {
    let source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
            max-length: 20;
        }
        @interaction submit_form {
            trigger: button.submit.activate;
            class: D1;
            state {
                read form_valid: bool;
            }
            input {
                username: text;
            }
            set form_valid = inputs.username.length > 0;
        }
        @button submit {
            id: "submit";
            text: "Submit";
        }
    }"#;

    let kotlin = emit_android_d1_runtime_with_form(source).expect("Should emit runtime with form validation");
    assert!(kotlin.contains("validateFormInputs"));
    assert!(kotlin.contains("isValidInputField"));
    assert!(kotlin.contains("getValidationError"));
    assert!(kotlin.contains("validationErrors: MutableMap<String, String?> = mutableMapOf()"));
    assert!(kotlin.contains("ErrorCode.VALIDATION"));
}

/// Test real-time input validation in UI layer.
#[test]
fn test_real_time_input_validation() {
    let source = r#"@ontology demo {
        @cell email {
            id: "email";
            text: "Enter email address";
            max-length: 50;
        }
    }"#;

    let kotlin = emit_android_input_preview(source).expect("Should emit input with real-time validation");
    assert!(kotlin.contains("addTextChangedListener(object : TextWatcher"));
    assert!(kotlin.contains("override fun afterTextChanged(s: android.text.Editable?)"));
    assert!(kotlin.contains("validateInput(s?.toString() ?: \"\")"));
    assert!(kotlin.contains("errorTextView"));
    assert!(kotlin.contains("errorTextView.visibility"));
}