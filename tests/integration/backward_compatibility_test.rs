//! Backward Compatibility Testing for Phase 1
//! 
//! This test validates backward compatibility with existing functionality:
//! - Static text preview functionality
//! - Root-button preview functionality
//! - GOYA-LANG integration
//! - Existing test suites

use goya_sdk::*;

#[test]
fn test_static_text_preview_backward_compatibility() {
    // Test existing static text preview functionality
    let static_text_source = r"""@ontology demo {
        @cell static_label {
            id: "static_label";
            text: "Hello World";
        }
    }""";
    
    let static_kotlin = goya_sdk::emit_android_static_text_preview(static_text_source)
        .expect("Should emit static text preview for backward compatibility");
    
    // Verify static text preview functionality
    assert!(static_kotlin.contains("GoyaStaticTextPreview"));
    assert!(static_kotlin.contains("text = \"Hello World\""));
    assert!(static_kotlin.contains("TextView"));
    
    // Verify source map compatibility
    let static_source_map = goya_sdk::emit_source_map(static_text_source)
        .expect("Should emit source map for static text");
    
    assert!(static_source_map.contains("static_label"));
}

#[test]
fn test_root_button_preview_backward_compatibility() {
    // Test existing root-button preview functionality
    let button_source = r"""@ontology demo {
        @button go_button {
            id: "go_button";
            text: "Go";
        }
        @interaction increment_interaction {
            trigger: button.go_button.activate;
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
        .expect("Should emit button preview for backward compatibility");
    
    // Verify button preview functionality
    assert!(button_kotlin.contains("GoyaRootButtonPreview"));
    assert!(button_kotlin.contains("onD1Action"));
    assert!(button_kotlin.contains("go_button"));
    assert!(button_kotlin.contains("increment_interaction"));
    
    // Verify source map compatibility
    let button_source_map = goya_sdk::emit_source_map(button_source)
        .expect("Should emit source map for button preview");
    
    assert!(button_source_map.contains("go_button"));
    assert!(button_source_map.contains("increment_interaction"));
}

#[test]
fn test_goya_lang_integration_backward_compatibility() {
    // Test existing GOYA-LANG integration
    let goya_lang_source = r"""@ontology demo {
        @cell title {
            id: "title";
            text: "Goya App";
        }
        @cell description {
            id: "description";
            text: "This is a Goya application";
        }
        @cell version {
            id: "version";
            text: "v1.0.0";
        }
    }""";
    
    let goya_lang_output = goya_sdk::emit_goya_lang(goya_lang_source)
        .expect("Should emit GOYA-LANG output for backward compatibility");
    
    // Verify GOYA-LANG integration
    assert!(goya_lang_output.contains("GoyaLangEmitter"));
    assert!(goya_lang_output.contains("title"));
    assert!(goya_lang_output.contains("description"));
    assert!(goya_lang_output.contains("version"));
    
    // Verify GOYA-LANG source map compatibility
    let goya_lang_source_map = goya_sdk::emit_source_map(goya_lang_source)
        .expect("Should emit source map for GOYA-LANG");
    
    assert!(goya_lang_source_map.contains("title"));
    assert!(goya_lang_source_map.contains("description"));
    assert!(goya_lang_source_map.contains("version"));
}

#[test]
fn test_enhanced_android_features_backward_compatibility() {
    // Test that enhanced Android features maintain backward compatibility
    let enhanced_source = r"""@ontology demo {
        @cell basic_cell {
            id: "basic_cell";
            text: "Basic Cell";
        }
        @button basic_button {
            id: "basic_button";
            text: "Basic Button";
        }
        @input basic_input {
            id: "basic_input";
            text: "Basic Input";
        }
    }""";
    
    // Test enhanced output still maintains backward compatibility
    let enhanced_output = goya_sdk::emit_android_enhanced_output(enhanced_source)
        .expect("Should emit enhanced output maintaining backward compatibility");
    
    // Verify enhanced output still includes basic functionality
    assert!(enhanced_output.contains("basic_cell"));
    assert!(enhanced_output.contains("basic_button"));
    assert!(enhanced_output.contains("basic_input"));
    assert!(enhanced_output.contains("GoyaEnhancedInput"));
    
    // Verify source map compatibility
    let enhanced_source_map = goya_sdk::emit_source_map(enhanced_source)
        .expect("Should emit source map for enhanced compatibility");
    
    assert!(enhanced_source_map.contains("basic_cell"));
    assert!(enhanced_source_map.contains("basic_button"));
    assert!(enhanced_source_map.contains("basic_input"));
}

#[test]
fn test_static_text_backward_compatibility_with_enhanced() {
    // Test static text preview with enhanced source maps
    let static_with_enhanced = r"""@ontology demo {
        @cell static_label {
            id: "static_label";
            text: "Static Label";
        }
        @input enhanced_input {
            id: "enhanced_input";
            text: "Enhanced Input";
        }
    }""";
    
    // Test static text preview
    let static_output = goya_sdk::emit_android_static_text_preview("""@ontology demo {
        @cell static_label {
            id: "static_label";
            text: "Static Label";
        }
    }""")
        .expect("Should emit static text preview");
    
    // Test enhanced output
    let enhanced_output = goya_sdk::emit_android_enhanced_output(static_with_enhanced)
        .expect("Should emit enhanced output");
    
    // Verify both work together
    assert!(static_output.contains("GoyaStaticTextPreview"));
    assert!(enhanced_output.contains("GoyaEnhancedInput"));
    assert!(enhanced_output.contains("enhanced_input"));
}

#[test]
fn test_button_preview_backward_compatibility_with_enhanced() {
    // Test button preview with enhanced source maps
    let button_with_enhanced = r"""@ontology demo {
        @button root_button {
            id: "root_button";
            text: "Root Button";
        }
        @input enhanced_input {
            id: "enhanced_input";
            text: "Enhanced Input";
            placeholder: "Enter value...";
        }
        @interaction enhanced_interaction {
            trigger: button.root_button.activate;
            class: D1;
            state {
                read count: int;
                write count: int;
            }
            input {
                value: text;
            }
            set count = state.count + 1;
        }
    }""";
    
    // Test button preview
    let button_output = goya_sdk::emit_android_root_button_preview(button_with_enhanced)
        .expect("Should emit button preview");
    
    // Test enhanced output
    let enhanced_output = goya_sdk::emit_android_enhanced_output(button_with_enhanced)
        .expect("Should emit enhanced output");
    
    // Verify both work together
    assert!(button_output.contains("GoyaRootButtonPreview"));
    assert!(button_output.contains("root_button"));
    assert!(enhanced_output.contains("GoyaEnhancedInput"));
    assert!(enhanced_output.contains("enhanced_input"));
}

#[test]
fn test_goya_lang_backward_compatibility_with_enhanced() {
    // Test GOYA-LANG integration with enhanced features
    let goya_with_enhanced = r"""@ontology demo {
        @cell traditional_cell {
            id: "traditional_cell";
            text: "Traditional Cell";
        }
        @input enhanced_input {
            id: "enhanced_input";
            text: "Enhanced Input";
            max-length: 50;
        }
        @button enhanced_button {
            id: "enhanced_button";
            text: "Enhanced Button";
        }
    }""";
    
    // Test GOYA-LANG output
    let goya_lang_output = goya_sdk::emit_goya_lang(goya_with_enhanced)
        .expect("Should emit GOYA-LANG output");
    
    // Test enhanced output
    let enhanced_output = goya_sdk::emit_android_enhanced_output(goya_with_enhanced)
        .expect("Should emit enhanced output");
    
    // Verify both work together
    assert!(goya_lang_output.contains("GoyaLangEmitter"));
    assert!(goya_lang_output.contains("traditional_cell"));
    assert!(enhanced_output.contains("GoyaEnhancedInput"));
    assert!(enhanced_output.contains("enhanced_input"));
    assert!(enhanced_output.contains("enhanced_button"));
}

#[test]
fn test_backward_compatibility_regression_testing() {
    // Test regression testing for backward compatibility
    let regression_test_cases = vec![
        r"""@ontology demo {
            @cell label1 {
                id: "label1";
                text: "Label 1";
            }
        }""",
        r"""@ontology demo {
            @button button1 {
                id: "button1";
                text: "Button 1";
            }
        }""",
        r"""@ontology demo {
            @input input1 {
                id: "input1";
                text: "Input 1";
            }
        }""",
        r"""@ontology demo {
            @interaction interaction1 {
                trigger: button.button1.activate;
                class: D1;
                state {
                    read count: int;
                    write count: int;
                }
                input {
                    value: int;
                }
                set count = state.count + input.value;
            }
        }""",
    ];
    
    for (i, test_case) in regression_test_cases.iter().enumerate() {
        // Test each case produces expected output
        let source_map_output = goya_sdk::emit_source_map(test_case)
            .expect(&format!("Source map should work for test case {}", i));
        
        let android_output = goya_sdk::emit_android_enhanced_output(test_case)
            .expect(&format!("Android enhanced output should work for test case {}", i));
        
        // Verify output consistency
        assert!(source_map_output.contains("kotlin_mappings"));
        assert!(android_output.contains("GoyaEnhancedInput"));
    }
}