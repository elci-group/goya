//! Parser to Emitter Integration Testing
//! 
//! This test validates the parser-enhanced input properties integration
//! through the complete pipeline: parser → emitters → source maps → testing.

use goya_sdk::*;

#[test]
fn test_parser_to_emitter_integration() {
    // Test parser-enhanced input properties through entire pipeline
    let source_with_enhanced_properties = r"""@ontology demo {
        @input enhanced_text {
            id: "enhanced_text";
            text: "Enhanced Text Input";
            placeholder: "Enter text here...";
            max-length: 100;
            input-type: "text";
        }
        @input enhanced_email {
            id: "enhanced_email";
            text: "Enhanced Email Input";
            placeholder: "Enter email...";
            max-length: 50;
            validation-rule: "email";
            input-type: "email";
        }
        @input enhanced_password {
            id: "enhanced_password";
            text: "Enhanced Password Input";
            placeholder: "Enter password...";
            max-length: 20;
            input-type: "password";
        }
    }""";
    
    // Test parser processing of enhanced input properties
    let parsed_result = goya_sdk::parse_source(source_with_enhanced_properties)
        .expect("Parser should process enhanced input properties");
    
    // Test emitter integration with parsed result
    let emitter_output = goya_sdk::emit_android_enhanced_output(source_with_enhanced_properties)
        .expect("Emitter should process enhanced input properties");
    
    // Verify enhanced input properties are correctly processed
    assert!(emitter_output.contains("enhanced_text"));
    assert!(emitter_output.contains("enhanced_email"));
    assert!(emitter_output.contains("enhanced_password"));
    
    // Verify placeholder processing
    assert!(emitter_output.contains("Enter text here..."));
    assert!(emitter_output.contains("Enter email..."));
    assert!(emitter_output.contains("Enter password..."));
    
    // Verify max-length processing
    assert!(emitter_output.contains("maxLength"));
    
    // Verify input-type processing
    assert!(emitter_output.contains("inputType"));
}

#[test]
fn test_parser_input_validation_integration() {
    // Test input validation integration
    let source_with_validation = r"""@ontology demo {
        @input validated_input {
            id: "validated_input";
            text: "Validated Input";
            placeholder: "Validation...";
            max-length: 50;
            validation-rule: "alphanumeric";
        }
    }""";
    
    let validation_output = goya_sdk::emit_android_enhanced_output(source_with_validation)
        .expect("Should emit validation-enhanced output");
    
    assert!(validation_output.contains("validated_input"));
    assert!(validation_output.contains("alphanumeric"));
    assert!(validation_output.contains("GoyaEnhancedInput"));
}

#[test]
fn test_parser_max_length_integration() {
    // Test max-length constraint integration
    let source_with_length_constraints = r"""@ontology demo {
        @input short_input {
            id: "short_input";
            text: "Short Input";
            max-length: 10;
        }
        @input medium_input {
            id: "medium_input";
            text: "Medium Input";
            max-length: 50;
        }
        @input long_input {
            id: "long_input";
            text: "Long Input";
            max-length: 100;
        }
    }""";
    
    let length_output = goya_sdk::emit_android_enhanced_output(source_with_length_constraints)
        .expect("Should emit length-constrained output");
    
    assert!(length_output.contains("short_input"));
    assert!(length_output.contains("medium_input"));
    assert!(length_output.contains("long_input"));
    assert!(length_output.contains("maxLength"));
}

#[test]
fn test_parser_placeholder_integration() {
    // Test placeholder integration
    let source_with_placeholders = r"""@ontology demo {
        @input name_field {
            id: "name_field";
            text: "Name";
            placeholder: "Enter your name...";
        }
        @input email_field {
            id: "email_field";
            text: "Email";
            placeholder: "Enter your email...";
        }
        @input phone_field {
            id: "phone_field";
            text: "Phone";
            placeholder: "Enter your phone...";
        }
    }""";
    
    let placeholder_output = goya_sdk::emit_android_enhanced_output(source_with_placeholders)
        .expect("Should emit placeholder-enhanced output");
    
    assert!(placeholder_output.contains("name_field"));
    assert!(placeholder_output.contains("email_field"));
    assert!(placeholder_output.contains("phone_field"));
    assert!(placeholder_output.contains("Enter your name..."));
    assert!(placeholder_output.contains("Enter your email..."));
    assert!(placeholder_output.contains("Enter your phone..."));
}

#[test]
fn test_parser_input_type_integration() {
    // Test input-type integration
    let source_with_input_types = r"""@ontology demo {
        @input text_field {
            id: "text_field";
            text: "Text Field";
            input-type: "text";
        }
        @input email_field {
            id: "email_field";
            text: "Email Field";
            input-type: "email";
        }
        @input password_field {
            id: "password_field";
            text: "Password Field";
            input-type: "password";
        }
        @input number_field {
            id: "number_field";
            text: "Number Field";
            input-type: "number";
        }
    }""";
    
    let input_type_output = goya_sdk::emit_android_enhanced_output(source_with_input_types)
        .expect("Should emit input-type-enhanced output");
    
    assert!(input_type_output.contains("text_field"));
    assert!(input_type_output.contains("email_field"));
    assert!(input_type_output.contains("password_field"));
    assert!(input_type_output.contains("number_field"));
    assert!(input_type_output.contains("inputType"));
}

#[test]
fn test_parser_validation_rule_integration() {
    // Test validation-rule integration
    let source_with_validation_rules = r"""@ontology demo {
        @input email_input {
            id: "email_input";
            text: "Email Input";
            validation-rule: "email";
        }
        @input phone_input {
            id: "phone_input";
            text: "Phone Input";
            validation-rule: "phone";
        }
        @input alphanumeric_input {
            id: "alphanumeric_input";
            text: "Alphanumeric Input";
            validation-rule: "alphanumeric";
        }
        @input url_input {
            id: "url_input";
            text: "URL Input";
            validation-rule: "url";
        }
    }""";
    
    let validation_rule_output = goya_sdk::emit_android_enhanced_output(source_with_validation_rules)
        .expect("Should emit validation-rule-enhanced output");
    
    assert!(validation_rule_output.contains("email_input"));
    assert!(validation_rule_output.contains("phone_input"));
    assert!(validation_rule_output.contains("alphanumeric_input"));
    assert!(validation_rule_output.contains("url_input"));
    assert!(validation_rule_output.contains("validationRule"));
}

#[test]
fn test_parser_complete_pipeline_integration() {
    // Test complete parser → emitter → source map → testing integration
    let complete_pipeline_source = r"""@ontology demo {
        @input complete_field {
            id: "complete_field";
            text: "Complete Field";
            placeholder: "Complete placeholder...";
            max-length: 100;
            input-type: "text";
            validation-rule: "alphanumeric";
        }
        @button complete_button {
            id: "complete_button";
            text: "Complete Button";
        }
    }""";
    
    // Test parser
    let parsed_result = goya_sdk::parse_source(complete_pipeline_source)
        .expect("Parser should process complete source");
    
    // Test emitter
    let emitter_result = goya_sdk::emit_android_enhanced_output(complete_pipeline_source)
        .expect("Emitter should process enhanced output");
    
    // Test source map
    let source_map_result = goya_sdk::emit_source_map(complete_pipeline_source)
        .expect("Source map should emit mapping");
    
    // Test testing infrastructure
    let testing_result = goya_sdk::emit_android_input_preview(complete_pipeline_source)
        .expect("Testing infrastructure should emit preview");
    
    // Verify integration
    assert!(emitter_result.contains("GoyaEnhancedInput"));
    assert!(source_map_result.contains("kotlin_mappings"));
    assert!(testing_result.contains("GoyaRootInputPreview"));
    assert!(testing_result.contains("complete_field"));
}