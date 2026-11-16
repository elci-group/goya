#!/usr/bin/env rustc
//! Test source map Android Kotlin support
use goya_sdk::artifacts::VerifiedPreview;
use goya_sdk::source_map::map_kotlin_location;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn main() -> Result<(), String> {
    // Create a test verified preview with Kotlin mappings
    let mut artifacts = BTreeMap::new();
    
    // Source text
    let source = "class TextView { val text = \"Hello\" }";
    let kotlin = "package com.example;\nclass GoyaTextPreview { }";
    
    // Create a simple source map JSON with Kotlin mappings
    let source_map_json = json!({
        "schema_version": "2",
        "mappings": [
            {
                "semantic_id": {"module": "app/main.gscene", "declared_id": "root"},
                "source": {"start_byte": 0, "end_byte": source.len()},
                "swift": {"start_byte": 0, "end_byte": 5}
            }
        ],
        "kotlin_mappings": [
            {
                "semantic_id": {"module": "app/main.gscene", "declared_id": "root"},
                "source": {"start_byte": 0, "end_byte": source.len()},
                "kotlin": {"start_byte": 0, "end_byte": kotlin.len()}
            }
        ]
    });
    
    artifacts.insert("source-map.json".to_string(), source_map_json.to_string().as_bytes().to_vec());
    artifacts.insert("capabilities.json".to_string(), b"{}".to_vec());
    artifacts.insert("AndroidInputPreview.kt".to_string(), b"// Kotlin preview".to_vec());
    artifacts.insert("AndroidD1Runtime.kt".to_string(), b"// D1 runtime".to_vec());
    
    // Create a fake verified preview (simplified for testing)
    let preview = VerifiedPreview {
        artifacts,
        hashes: BTreeMap::new(),
        input_sha256: format!("{:x}", sha2::Sha256::digest(source.as_bytes())),
    };
    
    // Test mapping Kotlin location
    let result = map_kotlin_location(&preview, source, kotlin, 1, 1)
        .map_err(|e| format!("Kotlin location mapping failed: {}", e))?;
    
    println!("Kotlin location mapping result: {}\n", serde_json::to_string_pretty(&result).unwrap());
    
    // Verify result structure
    assert_eq!(result.get("status").and_then(|s| s.as_str()), Some("MAPPED"));
    assert!(result.get("kotlin_location").is_some());
    assert!(result.get("mappings").is_some());
    
    println!("✓ Kotlin location mapping test passed");
    Ok(())
}