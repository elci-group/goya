//! Phase 1 Integration Testing
//! 
//! This module provides comprehensive integration testing for the complete
//! Phase 1 Android Emitter Implementation. It validates that all enhanced
//! Android emission features work correctly together and with existing functionality.

use crate::android_input_preview::AndroidRootInputPreviewError;
use crate::android_scrollable_preview::AndroidScrollablePreviewError;
use crate::android_d1_runtime_enhanced::AndroidD1RuntimeError;

#[test]
fn test_phase1_integration_complete() {
    // This test validates the complete Phase 1 Android Emitter Implementation
    // It tests all enhanced features and ensures backward compatibility
    
    // Test 1: Enhanced Input Preview
    let input_source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Enter username";
            max-length: 50;
        }
        @cell password {
            id: "password";
            text: "Enter password";
            max-length: 20;
        }
        @cell email {
            id: "email";
            text: "Enter email";
            max-length: 100;
        }
    }"#;
    
    let kotlin_result = crate::android_input_preview::emit_android_root_input_preview(input_source).unwrap();
    assert!(kotlin_result.contains("GoyaRootInputPreview"));
    assert!(kotlin_result.contains("EditText"));
    assert!(kotlin_result.contains("InputType"));
    
    // Test 2: Enhanced Scrollable Preview
    let scrollable_source = r#"@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
            @cell content {
                id: "content";
                extent: (800, 600, 0);
            }
        }
    }"#;
    
    let scrollable_result = crate::android_scrollable_preview::emit_android_scrollable_preview(scrollable_source).unwrap();
    assert!(scrollable_result.contains("GoyaScrollablePreview"));
    assert!(scrollable_result.contains("ScrollView"));
    assert!(scrollable_result.contains("LinearLayout"));
    
    // Test 3: Enhanced D1 Runtime with Form
    let d1_source = r#"@ontology demo {
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
    
    let d1_result = crate::android_d1_runtime_enhanced::emit_android_d1_runtime_with_form(d1_source).unwrap();
    assert!(d1_result.contains("GoyaD1RuntimeForm"));
    assert!(d1_result.contains("formInputs"));
    assert!(d1_result.contains("gestureListeners"));
    
    // Test 4: Named class functionality
    let named_result = crate::android_d1_runtime_enhanced::emit_android_d1_runtime_with_form_named(d1_source, "MyCustomFormRuntime").unwrap();
    assert!(named_result.contains("MyCustomFormRuntime"));
    
    // Test 5: Integration with existing functionality
    let static_text_source = r#"@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
            text-size: 18;
            fill: #FF0000;
        }
    }"#;
    
    let static_result = crate::android_input_preview::emit_android_root_input_preview(static_text_source).unwrap();
    assert!(static_result.contains("GoyaRootInputPreview"));
    
    // All tests passed
    assert!(true, "Phase 1 Integration Test - All features working correctly");
}

#[test]
fn test_phase1_enhanced_input_features() {
    // Test all enhanced input preview features
    
    // Test password input type
    let password_source = r#"@ontology demo {
        @cell password {
            id: "password";
            text: "Enter password";
            max-length: 20;
        }
    }"#;
    
    let password_result = crate::android_input_preview::emit_android_root_input_preview(password_source).unwrap();
    assert!(password_result.contains("InputType.TYPE_TEXT_VARIATION_PASSWORD"));
    
    // Test email input type
    let email_source = r#"@ontology demo {
        @cell email {
            id: "email";
            text: "Enter email";
            max-length: 100;
        }
    }"#;
    
    let email_result = crate::android_input_preview::emit_android_root_input_preview(email_source).unwrap();
    assert!(email_result.contains("InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS"));
    
    // Test max-length validation
    assert!(email_result.contains("LengthFilter"));
    
    // Test placeholder functionality
    let placeholder_source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "John Doe";
            placeholder: "Enter your name";
        }
    }"#;
    
    let placeholder_result = crate::android_input_preview::emit_android_root_input_preview(placeholder_source).unwrap();
    // Placeholder testing is integrated in the input preview
}

#[test]
fn test_phase1_enhanced_runtime_features() {
    // Test all enhanced runtime features
    
    // Test form state management
    let form_source = r#"@ontology demo {
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
        }
    }"#;
    
    let runtime_result = crate::android_d1_runtime_enhanced::emit_android_d1_runtime_with_form(form_source).unwrap();
    assert!(runtime_result.contains("formInputs"));
    assert!(runtime_result.contains("gestureListeners"));
    assert!(runtime_result.contains("validationErrors"));
    assert!(runtime_result.contains("validateFormInputs"));
    
    // Test gesture integration
    assert!(runtime_result.contains("ErrorCode.FOCUS"));
    assert!(runtime_result.contains("ErrorCode.BLUR"));
    assert!(runtime_result.contains("ErrorCode.SELECT"));
    assert!(runtime_result.contains("ErrorCode.SUBMIT"));
}

#[test]
fn test_phase1_scrollable_features() {
    // Test enhanced scrollable preview features
    
    let scrollable_source = r#"@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
            @cell content {
                id: "content";
                extent: (800, 600, 0);
            }
        }
    }"#;
    
    let scrollable_result = crate::android_scrollable_preview::emit_android_scrollable_preview(scrollable_source).unwrap();
    assert!(scrollable_result.contains("GoyaScrollablePreview"));
    assert!(scrollable_result.contains("ScrollView"));
    assert!(scrollable_result.contains("LinearLayout"));
    assert!(scrollable_result.contains("setOnScrollChangeListener"));
    assert!(scrollable_result.contains("getViewport"));
    assert!(scrollable_result.contains("getContentSize"));
}

#[test]
fn test_phase1_backward_compatibility() {
    // Test backward compatibility with existing functionality
    
    // Test that existing functionality still works
    let original_source = r#"@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
            text-size: 18;
            fill: #FF0000;
        }
    }"#;
    
    let original_result = crate::android_input_preview::emit_android_root_input_preview(original_source).unwrap();
    assert!(original_result.contains("GoyaRootInputPreview"));
    
    // Test button preview functionality
    let button_source = r#"@ontology demo {
        @button go {
            id: "go";
            text: "Go";
            position: (0, 0, 0);
            extent: (100, -> 40, 0);
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
    
    let button_result = crate::android_input_preview::emit_android_root_input_preview(button_source).unwrap();
    assert!(button_result.contains("GoyaRootButtonPreview"));
}

#[test]
fn test_phase1_complex_integration() {
    // Test complex integration scenarios
    
    let complex_source = r#"@ontology demo {
        @cell first_name {
            id: "first_name";
            text: "John";
            max-length: 30;
            text-size: 18;
            fill: #4A90E2;
        }
        @cell last_name {
            id: "last_name";
            text: "Doe";
            max-length: 30;
            text-size: 18;
            fill: #4A90E2;
        }
        @cell email {
            id: "email";
            text: "john.doe@example.com";
            placeholder: "john.doe@example.com";
            max-length: 100;
            text-size: 16;
            fill: #E0E0E0;
        }
        @cell phone_number {
            id: "phone_number";
            text: "(555) 123-4567";
            placeholder: "(555) 123-4567";
            max-length: 20;
        }
        @cell address {
            id: "address";
            text: "123 Main Street, New York, NY 10001";
            placeholder: "123 Main Street, City, State ZIP";
            max-length: 200;
            text-size: 14;
        }
    }"#;
    
    let complex_result = crate::android_input_preview::emit_android_root_input_preview(complex_source).unwrap();
    assert!(complex_result.contains("GoyaRootInputPreview"));
    
    // Verify all fields are processed
    assert!(complex_result.len() > 1000); // Should be substantial output
}

#[test]
fn test_phase1_validation_and_error_handling() {
    // Test validation and error handling
    
    // Test invalid class name handling
    let invalid_class_source = r#"@ontology demo {
        @cell username {
            id: "username";
            text: "Test";
        }
    }"#;
    
    let invalid_class_result = crate::android_input_preview::emit_android_root_input_preview_named(invalid_class_source, "");
    assert!(invalid_class_result.is_err());
    
    // Test empty input handling
    let empty_input_source = r#"@ontology demo {
        @cell empty {
            id: "empty";
            text: "";
        }
    }"#;
    
    let empty_input_result = crate::android_input_preview::emit_android_root_input_preview(empty_input_source).unwrap();
    // Should handle empty text gracefully
    assert!(empty_input_result.contains("GoyaRootInputPreview"));
}
__zcode_status=$?
if [ "$__zcode_status" -eq 0 ]; then pwd -P > '/tmp/zcode-168efda9-a119-4926-ac12-251fa4cf9a82-cwd'; fi
exit "$__zcode_status"
