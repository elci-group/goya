use goya_executable::{D1Snapshot, D1Value, InputSnapshot};
use maja_runtime::emit_checked_increment;
use std::{collections::BTreeMap, fs, process::Command};

#[test]
fn chrome_matches_canonical_d1_for_integer_and_revision_boundaries() {
    let mut script = String::from("<!doctype html><body><script>let results=[];\n");
    let mut expected = Vec::new();
    for (delta, cases) in [
        (
            1_i64,
            vec![
                (0_i64, 0_u64),
                (i64::MAX - 1, 41),
                (i64::MAX, 2),
                (5, u64::MAX),
            ],
        ),
        (-1, vec![(0, 0), (i64::MIN + 1, 9), (i64::MIN, 2)]),
    ] {
        let source = format!(
            r#"@ontology app {{ @cell root {{ id: "root" @button inc {{ id: "button.inc" }} }} @interaction add {{ id: "action.add"; trigger: button.inc.activate; class: D1; state {{ read count: int; write count: int; }} input {{ }} set count = state.count + {delta}; }} }}"#
        );
        let scene = goya_lang::parse_strict(&source).unwrap().scene;
        let generated = emit_checked_increment(&scene).unwrap();
        let canonical = goya_lang::compile_d1_interactions(&scene)
            .unwrap()
            .remove(0);
        script.push_str(&format!("{{ const action = {};\n", generated.javascript));
        for (value, revision) in cases {
            let snapshot = D1Snapshot::new(
                revision,
                BTreeMap::from([("count".into(), D1Value::Int(value))]),
            )
            .unwrap();
            let input = InputSnapshot::new(BTreeMap::new()).unwrap();
            let outcome = canonical.execute(&snapshot, &input);
            expected.push(match outcome {
                Ok(outcome) => {
                    let D1Value::Int(next) = outcome.next_state["count"] else {
                        panic!("wrong type")
                    };
                    format!("ok:{next}:{}", outcome.revision_after)
                }
                Err(error) => {
                    let message = error.to_string();
                    if message.contains("integer addition overflow") {
                        "error:arithmetic-overflow".to_owned()
                    } else if message.contains("revision counter exhausted") {
                        "error:revision-overflow".to_owned()
                    } else {
                        panic!("unexpected canonical error: {message}")
                    }
                }
            });
            script.push_str(&format!("{{ const r=action({value}n,{revision}n); results.push(r.ok ? `ok:${{r.value}}:${{r.revision}}` : `error:${{r.error}}`); }}\n"));
        }
        script.push_str("}\n");
    }
    script.push_str("document.body.textContent=results.join('|');</script></body>");
    let path = std::env::temp_dir().join(format!(
        "maja-browser-differential-{}.html",
        std::process::id()
    ));
    let created = Command::new("bank")
        .arg(&path)
        .status()
        .expect("bank available");
    assert!(created.success());
    fs::write(&path, script).unwrap();
    let result = Command::new("google-chrome")
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--dump-dom",
        ])
        .arg(format!("file://{}", path.display()))
        .output()
        .expect("Chrome available");
    let _ = fs::remove_file(path);
    assert!(
        result.status.success(),
        "Chrome failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    let expected_dom = format!("<body>{}</body>", expected.join("|"));
    assert!(
        output.contains(&expected_dom),
        "expected {expected_dom}, got {output}; stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
