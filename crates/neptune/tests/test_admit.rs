#[test]
fn test_admit_v2() {
    let source = include_str!("./native-button.gscene");
    println!("Source:\n{}", source);

    match goya_lang::handoff::admit_v2(source) {
        Ok(handoff) => {
            println!("Admitted successfully!");
            println!("Contract: {}", handoff.contract());
        }
        Err(e) => {
            println!("Error: {} at {}..{}", e, e.range.start, e.range.end);
            panic!("Admit failed: {}", e);
        }
    }
}
