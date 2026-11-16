//! Android Source Map Integration Testing
//! 
//! This test validates Android source mapping integration with enhanced input previews.

use goya_sdk::*;

#[test]
fn test_android_source_map_with_enhanced_inputs() {
    // Test Android source mapping with parser-enhanced input properties
    let source_with_enhanced_inputs = r"""@ontology demo {
        @input user_name {
            id: "user_name";
            text: "User Name";
            placeholder: "Enter your name...";
            max-length: 50;
            validation-rule: "name";
        }
        @input user_email {
            id: "user_email";
            text: "User Email";
            placeholder: "Enter your email...";
            max-length: 50;
            validation-rule: "email";
        }
        @input user_password {
            id: "user_password";
            text: "User Password";
            placeholder: "Enter your password...";
            max-length: 20;
            input-type: "password";
        }
    }""";
    
    // Test Android source map generation
    let android_source_map = goya_sdk::emit_android_source_map(source_with_enhanced_inputs)
        .expect("Should emit Android source map");
    
    // Test enhanced input preview generation
    let android_input_preview = goya_sdk::emit_android_input_preview(source_with_enhanced_inputs)
        .expect("Should emit Android input preview");
    
    // Test enhanced output generation
    let android_enhanced_output = goya_sdk::emit_android_enhanced_output(source_with_enhanced_inputs)
        .expect("Should emit Android enhanced output");
    
    // Verify source map contains enhanced input mappings
    assert!(android_source_map.contains("kotlin_mappings"));
    assert!(android_source_map.contains("user_name"));
    assert!(android_source_map.contains("user_email"));
    assert!(android_source_map.contains("user_password"));
    
    // Verify enhanced input preview contains all enhanced properties
    assert!(android_input_preview.contains("GoyaRootInputPreview"));
    assert!(android_input_preview.contains("user_name"));
    assert!(android_input_preview.contains("user_email"));
    assert!(android_input_preview.contains("user_password"));
    assert!(android_input_preview.contains("placeholder"));
    assert!(android_input_preview.contains("maxLength"));
    assert!(android_input_preview.contains("inputType"));
    
    // Verify enhanced output contains all enhancements
    assert!(android_enhanced_output.contains("GoyaEnhancedInput"));
    assert!(android_enhanced_output.contains("AndroidInputPreview"));
}

#[test]
fn test_android_source_map_input_properties() {
    // Test Android source mapping specifically for input properties
    let source_with_input_properties = r"""@ontology demo {
        @input text_input {
            id: "text_input";
            text: "Text Input";
            placeholder: "Enter text...";
            max-length: 100;
            input-type: "text";
        }
        @input email_input {
            id: "email_input";
            text: "Email Input";
            placeholder: "Enter email...";
            max-length: 50;
            validation-rule: "email";
        }
        @input number_input {
            id: "number_input";
            text: "Number Input";
            placeholder: "Enter number...";
            max-length: 10;
            input-type: "number";
        }
    }""";
    
    let android_source_map = goya_sdk::emit_android_source_map(source_with_input_properties)
        .expect("Should emit Android source map with input properties");
    
    // Verify source map captures all input properties
    assert!(android_source_map.contains("text_input"));
    assert!(android_source_map.contains("email_input"));
    assert!(android_source_map.contains("number_input"));
    
    // Verify source map structure
    assert!(android_source_map.contains("kotlin_mappings"));
    assert!(android_source_map.contains("kotlin_source_ranges"));
}

#[test]
fn test_android_source_map_multi_cell_support() {
    // Test Android source mapping with multi-cell scrolling support
    let source_with_scrolling = r"""@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
            height: 400;
        }
        @input item_name {
            id: "item_name";
            text: "Item Name";
            placeholder: "Enter item name...";
            max-length: 50;
        }
        @input item_description {
            id: "item_description";
            text: "Item Description";
            placeholder: "Enter description...";
            max-length: 200;
        }
        @input item_price {
            id: "item_price";
            text: "Item Price";
            placeholder: "Enter price...";
            max-length: 10;
            validation-rule: "price";
        }
    }""";
    
    let android_source_map = goya_sdk::emit_android_source_map(source_with_scrolling)
        .expect("Should emit Android source map with scrolling support");
    
    let android_preview = goya_sdk::emit_android_scrollable_preview(source_with_scrolling)
        .expect("Should emit Android scrollable preview");
    
    // Verify scrolling source map integration
    assert!(android_source_map.contains("viewport"));
    assert!(android_source_map.contains("item_name"));
    assert!(android_source_map.contains("item_description"));
    assert!(android_source_map.contains("item_price"));
    
    // Verify scrolling preview integration
    assert!(android_preview.contains("GoyaScrollablePreview"));
    assert!(android_preview.contains("ScrollView"));
    assert!(android_preview.contains("viewport"));
}

#[test]
fn test_android_source_map_gesture_integration() {
    // Test Android source mapping with gesture integration
    let source_with_gestures = r"""@ontology demo {
        @input searchable_input {
            id: "searchable_input";
            text: "Search Input";
            placeholder: "Search...";
            max-length: 50;
            on-focus: "show-suggestions";
            on-blur: "hide-suggestions";
            on-change: "filter-results";
            on-submit: "submit-search";
        }
        @input editable_input {
            id: "editable_input";
            text: "Editable Input";
            placeholder: "Edit...";
            max-length: 100;
            on-focus: "enable-edit-mode";
            on-blur: "disable-edit-mode";
            on-change: "save-changes";
            on-submit: "commit-changes";
        }
    }""";
    
    let android_source_map = goya_sdk::emit_android_source_map(source_with_gestures)
        .expect("Should emit Android source map with gesture integration");
    
    let android_enhanced_output = goya_sdk::emit_android_enhanced_output(source_with_gestures)
        .expect("Should emit Android enhanced output with gesture integration");
    
    // Verify gesture integration in source map
    assert!(android_source_map.contains("searchable_input"));
    assert!(android_source_map.contains("editable_input"));
    assert!(android_source_map.contains("onFocusListener"));
    assert!(android_source_map.contains("onBlurListener"));
    assert!(android_source_map.contains("onTextChangedListener"));
    assert!(android_source_map.contains("onEditorActionListener"));
}

#[test]
fn test_android_source_map_backward_compatibility() {
    // Test Android source mapping backward compatibility
    let static_text_source = r"""@ontology demo {
        @cell static_label {
            id: "static_label";
            text: "Static Label";
        }
    }""";
    
    let android_source_map = goya_sdk::emit_android_source_map(static_text_source)
        .expect("Should emit Android source map for static text");
    
    let static_preview = goya_sdk::emit_android_static_text_preview(static_text_source)
        .expect("Should emit Android static text preview");
    
    // Verify backward compatibility in source map
    assert!(android_source_map.contains("static_label"));
    assert!(static_preview.contains("GoyaStaticTextPreview"));
    assert!(static_preview.contains("text = \"Static Label\""));
}

#[test]
fn test_android_source_map_validation_integration() {
    // Test Android source mapping validation integration
    let source_with_validation = r"""@ontology demo {
        @input validation_input {
            id: "validation_input";
            text: "Validation Input";
            placeholder: "Enter value...";
            max-length: 30;
            validation-rule: "alphanumeric";
        }
        @input empty_validation_input {
            id: "empty_validation_input";
            text: "";
            placeholder: "Enter value...";
            max-length: 0;
            validation-rule: "required";
        }
    }""";
    
    let android_source_map = goya_sdk::emit_android_source_map(source_with_validation)
        .expect("Should emit Android source map with validation");
    
    let android_enhanced_output = goya_sdk::emit_android_enhanced_output(source_with_validation)
        .expect("Should emit Android enhanced output with validation");
    
    // Verify validation integration in source map
    assert!(android_source_map.contains("validation_input"));
    assert!(android_source_map.contains("empty_validation_input"));
    assert!(android_source_map.contains("validationRule"));
    assert!(android_enhanced_output.contains("validationRule"));
}

#[test]
fn test_android_source_map_complete_integration() {
    // Test complete Android source mapping integration
    let complete_android_source = r"""@ontology demo {
        @input user_input {
            id: "user_input";
            text: "User Input";
            placeholder: "Type here...";
            max-length: 100;
            validation-rule: "alphanumeric";
            input-type: "text";
        }
        @cell scrollable_view {
            id: "scrollable_view";
            scroll: true;
        }
        @button submit_button {
            id: "submit_button";
            text: "Submit";
        }
    }""";
    
    // Test Android source map
    let android_source_map = goya_sdk::emit_android_source_map(complete_android_source)
        .expect("Should emit complete Android source map");
    
    // Test enhanced input preview
    let android_input_preview = goya_sdk::emit_android_input_preview(complete_android_source)
        .expect("Should emit complete Android input preview");
    
    // Test enhanced output
    let android_enhanced_output = goya_sdk::emit_android_enhanced_output(complete_android_source)
        .expect("Should emit complete Android enhanced output");
    
    // Test scrollable preview
    let scrollable_preview = goya_sdk::emit_android_scrollable_preview(complete_android_source)
        .expect("Should emit complete scrollable preview");
    
    // Verify complete integration
    assert!(android_source_map.contains("kotlin_mappings"));
    assert!(android_input_preview.contains("GoyaRootInputPreview"));
    assert!(android_enhanced_output.contains("GoyaEnhancedInput"));
    assert!(scrollable_preview.contains("GoyaScrollablePreview"));
    assert!(android_source_map.contains("user_input"));
}