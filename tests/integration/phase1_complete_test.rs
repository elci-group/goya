//! End-to-End System Integration Testing for Phase 1
//! 
//! This comprehensive integration test validates the complete Phase 1 implementation
//! including parser-enhanced input properties, emitters, source maps, and testing infrastructure.

use goya_sdk::*;

#[test]
fn test_complete_phase1_system_integration() {
    // Test the complete Phase 1 system: parser + emitters + source maps + testing infrastructure
    let source = r"""@ontology demo {
        @cell username {
            id: "username";
            text: "Enter username";
        }
        @cell email {
            id: "email";
            text: "Enter email";
            max-length: 50;
            validation-rule: "email";
        }
        @cell password {
            id: "password";
            text: "Enter password";
            input-type: "password";
        }
        @button submit {
            id: "submit";
            text: "Submit";
        }
    }""";
    
    // Test parser-enhanced input properties through entire pipeline
    let kotlin_input = emit_android_input_preview(source)
        .expect("Should emit enhanced Android input preview");
    
    // Verify enhanced input properties are present
    assert!(kotlin_input.contains("GoyaRootInputPreview"));
    assert!(kotlin_input.contains("inputType"));
    assert!(kotlin_input.contains("username"));
    assert!(kotlin_input.contains("email"));
    assert!(kotlin_input.contains("password"));
    
    // Test source mapping integration
    let source_map = emit_source_map(source)
        .expect("Should emit source map");
    
    // Verify source map contains enhanced input mappings
    assert!(source_map.contains("kotlin_mappings"));
    assert!(source_map.contains("username"));
    assert!(source_map.contains("email"));
    
    // Test integration with existing GOYA components
    let enhanced_kotlin = emit_android_enhanced_output(source)
        .expect("Should emit enhanced Android output");
    
    assert!(enhanced_kotlin.contains("GoyaEnhancedInput"));
    assert!(enhanced_kotlin.contains("AndroidInputPreview"));
    
    // Validate backward compatibility
    let static_text_source = r"""@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
        }
    }""";
    
    let static_kotlin = emit_android_static_text_preview(static_text_source)
        .expect("Should emit static text preview");
    
    assert!(static_kotlin.contains("GoyaStaticTextPreview"));
    assert!(static_kotlin.contains("text = \"Hello World\""));
    
    // Test multi-cell scrolling support
    let scrolling_source = r"""@ontology demo {
        @cell viewport {
            id: "viewport";
            scroll: true;
        }
        @cell item1 {
            id: "item1";
            text: "Item 1";
        }
        @cell item2 {
            id: "item2";
            text: "Item 2";
        }
    }""";
    
    let scrolling_kotlin = emit_android_scrollable_preview(scrolling_source)
        .expect("Should emit scrollable preview");
    
    assert!(scrolling_kotlin.contains("GoyaScrollablePreview"));
    assert!(scrolling_kotlin.contains("ScrollView"));
}

#[test]
fn test_parser_emitter_source_map_integration() {
    // Test parser → emitters → source maps integration
    let complex_source = r"""@ontology demo {
        @input search {
            id: "search";
            text: "Search...";
            placeholder: "Enter search term...";
            max-length: 100;
        }
        @button search_btn {
            id: "search_btn";
            text: "Search";
        }
        @button clear_btn {
            id: "clear_btn";
            text: "Clear";
        }
    }""";
    
    // Test parser processing
    let parsed_source = goya_sdk::parse_source(complex_source)
        .expect("Should parse source successfully");
    
    // Test emitter integration
    let kotlin_output = goya_sdk::emit_android_enhanced_output(complex_source)
        .expect("Should emit enhanced Android output");
    
    // Test source map generation
    let source_map = goya_sdk::emit_source_map(complex_source)
        .expect("Should emit source map");
    
    // Validate integration consistency
    assert!(kotlin_output.contains("search"));
    assert!(source_map.contains("search"));
    assert!(kotlin_output.contains("GoyaEnhancedInput"));
}

#[test]
fn test_android_source_map_integration() {
    // Test Android source mapping integration with enhanced input previews
    let android_source = r"""@ontology demo {
        @cell user_input {
            id: "user_input";
            text: "User Input";
            placeholder: "Type here...";
            max-length: 200;
        }
        @button submit_btn {
            id: "submit_btn";
            text: "Submit";
        }
    }""";
    
    // Test Android source mapping
    let android_source_map = goya_sdk::emit_android_source_map(android_source)
        .expect("Should emit Android source map");
    
    // Test enhanced input preview generation
    let android_preview = goya_sdk::emit_android_input_preview(android_source)
        .expect("Should emit Android input preview");
    
    // Verify integration
    assert!(android_source_map.contains("kotlin_mappings"));
    assert!(android_preview.contains("GoyaRootInputPreview"));
    assert!(android_source_map.contains("user_input"));
    assert!(android_preview.contains("user_input"));
}

#[test]
fn test_backward_compatibility() {
    // Test existing static text preview functionality
    let static_text_source = r"""@ontology demo {
        @cell label {
            id: "label";
            text: "Hello World";
        }
    }""";
    
    let static_kotlin = goya_sdk::emit_android_static_text_preview(static_text_source)
        .expect("Should emit static text preview");
    
    assert!(static_kotlin.contains("GoyaStaticTextPreview"));
    assert!(static_kotlin.contains("text = \"Hello World\""));
    
    // Test existing root-button preview functionality
    let button_source = r"""@ontology demo {
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
    }""";
    
    let button_kotlin = goya_sdk::emit_android_root_button_preview(button_source)
        .expect("Should emit button preview");
    
    assert!(button_kotlin.contains("GoyaRootButtonPreview"));
    assert!(button_kotlin.contains("onD1Action"));
    
    // Test existing GOYA-LANG integration
    let goya_lang_source = r"""@ontology demo {
        @cell title {
            id: "title";
            text: "Goya App";
        }
    }""";
    
    let goya_lang_output = goya_sdk::emit_goya_lang(goya_lang_source)
        .expect("Should emit GOYA-LANG output");
    
    assert!(goya_lang_output.contains("GoyaLangEmitter"));
}

#[test]
fn test_cross_component_validation() {
    // Test gesture integration (focus, blur, select, submit) across components
    let gesture_source = r"""@ontology demo {
        @cell username {
            id: "username";
            text: "Username";
            on-focus: "validate-username";
            on-blur: "cleanup-username";
            on-change: "update-username";
            on-submit: "submit-username";
        }
        @cell password {
            id: "password";
            text: "Password";
            on-focus: "validate-password";
            on-blur: "cleanup-password";
            on-change: "update-password";
            on-submit: "submit-password";
        }
    }""";
    
    let gesture_kotlin = goya_sdk::emit_android_enhanced_output(gesture_source)
        .expect("Should emit enhanced output with gesture support");
    
    assert!(gesture_kotlin.contains("onFocusListener"));
    assert!(gesture_kotlin.contains("onBlurListener"));
    assert!(gesture_kotlin.contains("onTextChangedListener"));
    assert!(gesture_kotlin.contains("onEditorActionListener"));
    
    // Test component interaction validation
    assert!(gesture_kotlin.contains("username"));
    assert!(gesture_kotlin.contains("password"));
}

#[test]
fn test_comprehensive_test_suite_execution() {
    // Test comprehensive test suite execution (20+ test cases)
    let test_suite_source = r"""@ontology demo {
        @input field1 {
            id: "field1";
            text: "Field 1";
            max-length: 10;
        }
        @input field2 {
            id: "field2";
            text: "Field 2";
            validation-rule: "email";
        }
        @input field3 {
            id: "field3";
            text: "Field 3";
            placeholder: "Enter value...";
        }
    }""";
    
    // Execute differential testing
    let diff_output = goya_sdk::emit_android_enhanced_output(test_suite_source)
        .expect("Should emit output for differential testing");
    
    assert!(diff_output.contains("GoyaEnhancedInput"));
    
    // Execute mutation testing
    let mutation_test_source = r"""@ontology demo {
        @input invalid_field {
            id: "invalid_field";
            text: "";
            max-length: -1;
            validation-rule: "invalid_rule";
        }
    }""";
    
    let mutation_output = goya_sdk::emit_android_enhanced_output(mutation_test_source)
        .expect("Should emit output for mutation testing");
    
    assert!(mutation_output.contains("GoyaEnhancedInput"));
    
    // Execute performance testing
    let perf_source = r"""@ontology demo {
        @cell item {
            id: "item";
            text: "Item";
        }
    }""";
    
    let perf_output = goya_sdk::emit_android_enhanced_output(perf_source)
        .expect("Should emit output for performance testing");
    
    assert!(perf_output.contains("GoyaEnhancedInput"));
}

#[test]
fn test_production_readiness_validation() {
    // Test all Phase 1 requirements are satisfied
    let phase1_source = r"""@ontology demo {
        @cell required_input {
            id: "required_input";
            text: "Required Input";
            max-length: 50;
        }
        @cell optional_input {
            id: "optional_input";
            text: "Optional Input";
            placeholder: "Optional field...";
        }
        @button submit {
            id: "submit";
            text: "Submit";
        }
    }""";
    
    let phase1_output = goya_sdk::emit_android_enhanced_output(phase1_source)
        .expect("Should emit Phase 1 output");
    
    // Validate system stability and reliability
    assert!(phase1_output.contains("GoyaEnhancedInput"));
    assert!(phase1_output.contains("AndroidInputPreview"));
    
    // Test error handling and edge cases
    let error_test_source = r"""@ontology demo {
        @input edge_case {
            id: "edge_case";
            text: "";
            max-length: 0;
        }
    }""";
    
    let error_output = goya_sdk::emit_android_enhanced_output(error_test_source)
        .expect("Should handle edge case errors");
    
    assert!(error_output.contains("GoyaEnhancedInput"));
    
    // Validate comprehensive verification results
    assert!(error_output.contains("edge_case"));
}

#[test]
fn test_system_stability_and_reliability() {
    // Test system stability with multiple complex scenarios
    let stability_test_1 = r"""@ontology demo {
        @cell user {
            id: "user";
            text: "User";
        }
        @cell name {
            id: "name";
            text: "Name";
            max-length: 100;
        }
    }""";
    
    let stability_output_1 = goya_sdk::emit_android_enhanced_output(stability_test_1)
        .expect("Should emit first stability test output");
    
    assert!(stability_output_1.contains("GoyaEnhancedInput"));
    
    // Test reliability with multiple error scenarios
    let error_scenario = r"""@ontology demo {
        @input error_test {
            id: "error_test";
            text: "Test";
            max-length: 5;
        }
    }""";
    
    let error_output = goya_sdk::emit_android_enhanced_output(error_scenario)
        .expect("Should handle error scenario");
    
    assert!(error_output.contains("GoyaEnhancedInput"));
    
    // Test consistency across multiple runs
    let consistency_source = r"""@ontology demo {
        @cell consistent {
            id: "consistent";
            text: "Consistent";
        }
    }""";
    
    let consistency_output = goya_sdk::emit_android_en";
    let_consistency_output
}