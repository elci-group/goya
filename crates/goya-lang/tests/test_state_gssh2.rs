#[test]
fn test_state_in_gssh2() {
    let source = r#"@ontology counter {
  @state { count: int = 0 session; }
  @button increment { id: "button.increment" text: "Increment" }
  @interaction increment {
    id: "action.increment";
    trigger: button.increment.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 1;
  }
}
"#;
    let parsed = goya_lang::parse_strict_gssh2(source).unwrap();
    println!("Parsed successfully!");
    println!("Entries: {}", parsed.entries.len());
    for entry in &parsed.entries {
        println!("  kind={}, name={}, path={:?}", entry.kind, entry.name, entry.scene_path);
    }
}
