#[test]
fn test_native_button_parsing() {
    let source = "@ontology counter {\n  @state { count: int = 0; }\n  @button increment { id: \"button.increment\" text: \"Increment\" }\n  @interaction increment {\n    id: \"action.increment\";\n    trigger: button.increment.activate;\n    class: D1;\n    state { read count: int; write count: int; }\n    input { }\n    set count = state.count + 1;\n  }\n}";
    println!("Source:\n{}", source);

    match goya_lang::parse_strict_gssh3(&source) {
        Ok(parsed) => {
            println!("Parsed successfully!");
            println!("Entries: {}", parsed.entries.len());
            for entry in &parsed.entries {
                println!(
                    "  kind={}, name={}, path={:?}",
                    entry.kind, entry.name, entry.scene_path
                );
            }
        }
        Err(e) => {
            println!("Error: {} at {}..{}", e.name, e.range.start, e.range.end);
            panic!("Parse failed: {}", e);
        }
    }
}
