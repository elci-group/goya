//! Cross-Component Validation Testing
//! 
//! This test validates component interaction across the complete Phase 1 pipeline:
//! - Parser-enhanced input properties through entire pipeline
//! - Android source mapping integration with enhanced input previews
//! - Multi-cell scrolling support across all components
//! - Gesture integration (focus, blur, select, submit) across components

use goya_sdk::*;

#[test]
fn test_parser_emitter_source_map_pipeline() {
    // Test parser-enhanced input properties through complete pipeline
    let pipeline_source = r"""@ontology demo {
        @input complex_input {
            id: "complex_input";
            text: "Complex Input";
            placeholder: "Enter complex value...";
            max-length: 200;
            validation-rule: "complex";
            input-type: "text";
        }
    }""";
    
    // Test parser processing
    let parsed_result = goya_sdk::parse_source(pipeline_source)
        .expect("Parser should process complex input properties");
    
    // Test emitter integration
    let emitter_result = goya_sdk::emit_android_enhanced_output(pipeline_source)
        .expect("Emitter should process enhanced input properties");
    
    // Test source map generation
    let source_map_result = goya_sdk::emit_source_map(pipeline_source)
        .expect("Source map should generate mapping for complex input");
    
    // Test testing infrastructure
    let testing_result = goya_sdk::emit_android_input_preview(pipeline_source)
        .expect("Testing infrastructure should emit preview for complex input");
    
    // Verify pipeline integration
    assert!(emitter_result.contains("GoyaEnhancedInput"));
    assert!(source_map_result.contains("kotlin_mappings"));
    assert!(testing_result.contains("GoyaRootInputPreview"));
    assert!(emitter_result.contains("complex_input"));
    assert!(source_map_result.contains("complex_input"));
    assert!(testing_result.contains("complex_input"));
}

#[test]
fn test_android_source_map_enhanced_input_integration() {
    // Test Android source mapping integration with enhanced input previews
    let android_integration_source = r"""@ontology demo {
        @input android_input {
            id: "android_input";
            text: "Android Input";
            placeholder: "Android placeholder...";
            max-length: 100;
            input-type: "text";
        }
        @button android_button {
            id: "android_button";
            text: "Android Button";
        }
    }""";
    
    // Test Android source map generation
    let android_source_map = goya_sdk::emit_android_source_map(android_integration_source)
        .expect("Should emit Android source map with enhanced input");
    
    // Test enhanced input preview generation
    let android_input_preview = goya_sdk::emit_android_input_preview(android_integration_source)
        .expect("Should emit Android input preview with enhanced input");
    
    // Test enhanced output generation
    let android_enhanced_output = goya_sdk::emit_android_enhanced_output(android_integration_source)
        .expect("Should emit Android enhanced output with enhanced input");
    
    // Verify Android source map integration with enhanced input
    assert!(android_source_map.contains("kotlin_mappings"));
    assert!(android_source_map.contains("android_input"));
    
    // Verify enhanced input preview integration
    assert!(android_input_preview.contains("GoyaRootInputPreview"));
    assert!(android_input_preview.contains("android_input"));
    assert!(android_input_preview.contains("placeholder"));
    assert!(android_input_preview.contains("maxLength"));
    
    // Verify enhanced output integration
    assert!(android_enhanced_output.contains("GoyaEnhancedInput"));
    assert!(android_enhanced_output.contains("AndroidInputPreview"));
    assert!(android_enhanced_output.contains("android_input"));
}

#[test]
fn test_multi_cell_scrolling_across_components() {
    // Test multi-cell scrolling support across all components
    let scrolling_source = r"""@ontology demo {
        @cell main_viewport {
            id: "main_viewport";
            scroll: true;
            height: 500;
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
        @button add_item {
            id: "add_item";
            text: "Add Item";
        }
        @input search_filter {
            id: "search_filter";
            text: "Search Filter";
            placeholder: "Search...";
            max-length: 50;
        }
    }""";
    
    // Test scrolling preview generation
    let scrolling_preview = goya_sdk::emit_android_scrollable_preview(scrolling_source)
        .expect("Should emit scrolling preview with multi-cell support");
    
    // Test enhanced output generation
    let enhanced_output = goya_sdk::emit_android_enhanced_output(scrolling_source)
        .expect("Should emit enhanced output with multi-cell support");
    
    // Test source map generation
    let source_map = goya_sdk::emit_source_map(scrolling_source)
        .expect("Should emit source map with multi-cell support");
    
    // Verify multi-cell scrolling support
    assert!(scrolling_preview.contains("GoyaScrollablePreview"));
    assert!(scrolling_preview.contains("ScrollView"));
    assert!(scrolling_preview.contains("main_viewport"));
    assert!(scrolling_preview.contains("item_name"));
    assert!(scrolling_preview.contains("item_description"));
    assert!(scrolling_preview.contains("item_price"));
    assert!(scrolling_preview.contains("add_item"));
    assert!(scrolling_preview.contains("search_filter"));
    
    // Verify enhanced output multi-cell support
    assert!(enhanced_output.contains("GoyaEnhancedInput"));
    assert!(enhanced_output.contains("item_name"));
    assert!(enhanced_output.contains("item_description"));
    assert!(enhanced_output.contains("item_price"));
    assert!(enhanced_output.contains("add_item"));
    assert!(enhanced_output.contains("search_filter"));
    
    // Verify source map multi-cell support
    assert!(source_map.contains("kotlin_mappings"));
    assert!(source_map.contains("main_viewport"));
    assert!(source_map.contains("item_name"));
}

#[test]
fn test_gesture_integration_across_components() {
    // Test gesture integration (focus, blur, select, submit) across components
    let gesture_source = r"""@ontology demo {
        @input searchable_field {
            id: "searchable_field";
            text: "Search Field";
            placeholder: "Search...";
            max-length: 100;
            on-focus: "show-search-suggestions";
            on-blur: "hide-search-suggestions";
            on-change: "filter-search-results";
            on-submit: "submit-search-query";
        }
        @input editable_field {
            id: "editable_field";
            text: "Editable Field";
            placeholder: "Edit text...";
            max-length: 200;
            on-focus: "enable-edit-mode";
            on-blur: "save-and-disable-edit-mode";
            on-change: "auto-save-draft";
            on-submit: "commit-edits";
        }
        @input selectable_field {
            id: "selectable_field";
            text: "Selectable Field";
            placeholder: "Select option...";
            max-length: 50;
            on-focus: "show-selection-options";
            on-blur: "hide-selection-options";
            on-change: "update-selection";
            on-submit: "apply-selection";
        }
    }""";
    
    // Test gesture-enhanced output generation
    let gesture_output = goya_sdk::emit_android_enhanced_output(gesture_source)
        .expect("Should emit output with gesture integration");
    
    // Test gesture input preview generation
    let gesture_preview = goya_sdk::emit_android_input_preview(gesture_source)
        .expect("Should emit gesture-enhanced input preview");
    
    // Verify gesture integration across components
    assert!(gesture_output.contains("onFocusListener"));
    assert!(gesture_output.contains("onBlurListener"));
    assert!(gesture_output.contains("onTextChangedListener"));
    assert!(gesture_output.contains("onEditorActionListener"));
    
    assert!(gesture_output.contains("searchable_field"));
    assert!(gesture_output.contains("editable_field"));
    assert!(gesture_output.contains("selectable_field"));
    
    assert!(gesture_preview.contains("GoyaRootInputPreview"));
    assert!(gesture_preview.contains("searchable_field"));
    assert!(gesture_preview.contains("editable_field"));
    assert!(gesture_preview.contains("selectable_field"));
    
    assert!(gesture_output.contains("show-search-suggestions"));
    assert!(gesture_output.contains("hide-search-suggestions"));
    assert!(gesture_output.contains("filter-search-results"));
    assert!(gesture_output.contains("submit-search-query"));
    assert!(gesture_output.contains("enable-edit-mode"));
    assert!(gesture_output.contains("save-and-disable-edit-mode"));
}

#[test]
fn test_cross_component_validation_comprehensive() {
    // Test comprehensive cross-component validation
    let comprehensive_source = r"""@ontology demo {
        @input form_field1 {
            id: "form_field1";
            text: "Form Field  tha inputs through entire pipeline.";}
    """;
    
    // Test complete pipeline
    let pipeline_output = goya_sdk::emit_android_enhanced_output(comprehensive_source)
        .expect("Should emit pipeline output");
    
    let pipeline_source_map = goya_sdk::emit_source_map(comprehensive_source)
        .expect("Should emit pipeline source map");
    
    let pipeline_preview = goya_sdk::emit_android_input_preview(comprehensive_source)
        .expect("Should emit pipeline preview");
    
    // Verify cross-component validation
    assert!(pipeline_output.contains("GoyaEnhancedInput"));
    assert!(pipeline_source_map.contains("kotlin_mappings"));
    assert!(pipeline_preview.contains("GoyaRootInputPreview"));
    
    // Test multi-component scenario
    let multi_component_source = r"""@ontology demo {
        @input field1 {
            id: "field1";
            text: "Field 1";
            max-length: 50;
            placeholder: "Placeholder 1";
            validation-rule: "rule1";
        }
        @input field2 {
            id: "field2";
            text: "Field 2";
            max-length: 100;
            placeholder: "Placeholder 2";
            validation-rule: "rule2";
        }
        @button submit {
            id: "submit";
            text: "Submit";
        }
    }""";
    
    let multi_output = goya_sdk::emit_android_enhanced_output(multi_component_source)
        .expect("Should emit multi-component output");
    
    let multi_source_map = goya_sdk::emit_source_map(multi_component_source)
        .expect("Should emit multi-component source map");
    
    let multi_preview = goya_sdk::emit_android_input_preview(multi_component_source)
        .expect("Should emit multi-component preview");
    
    // Verify multi-component validation
    assert!(multi_output.contains("field1"));
    assert!(multi_output.contains("field2"));
    assert!(multi_output.contains("submit"));
    assert!(multi_source_map.contains("kotlin_mappings"));
    assert!(multi_preview.contains("GoyaRootInputPreview"));
}