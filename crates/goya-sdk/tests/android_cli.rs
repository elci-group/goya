use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "goya-android-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aragon-androidc"))
}

#[test]
fn compiles_the_input_scene_to_the_requested_kotlin_file() {
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    let output = scratch.path("Preview.kt");
    fs::write(
        &source,
        r#"@ontology demo { @cell label { id: "label"; text: "Hello $user \ café 🦀" } }"#,
    )
    .unwrap();

    let result = cli()
        .arg(&source)
        .arg(&output)
        .arg("WelcomePreview")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let kotlin = fs::read_to_string(output).unwrap();
    assert!(kotlin.contains("class WelcomePreview(context: Context)"));
    assert!(kotlin.contains(r#"Hello \$user \\ caf\u00e9 \ud83e\udd80"#));
}

#[test]
fn rejects_unsupported_scene_without_touching_existing_output() {
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    let output = scratch.path("Preview.kt");
    fs::write(
        &source,
        r#"@ontology demo { @button action { id: "action"; text: "No" } }"#,
    )
    .unwrap();
    let result = cli().arg(&source).arg(&output).output().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("scene rejected:"));
    assert!(!output.exists(), "rejected input must not create output");
}

#[test]
fn refuses_to_replace_an_existing_output() {
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    let output = scratch.path("Preview.kt");
    fs::write(
        &source,
        r#"@ontology demo { @cell label { id: "label"; text: "Hello" } }"#,
    )
    .unwrap();
    fs::write(&output, "keep this output").unwrap();

    let result = cli().arg(&source).arg(&output).output().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must not already exist"));
    assert_eq!(fs::read_to_string(output).unwrap(), "keep this output");
}

#[test]
fn refuses_when_output_aliases_the_source_file() {
    let scratch = Scratch::new();
    let source_and_output = scratch.path("input.gscene");
    let original = r#"@ontology demo { @cell label { id: "label"; text: "Hello" } }"#;
    fs::write(&source_and_output, original).unwrap();

    let result = cli()
        .arg(&source_and_output)
        .arg(&source_and_output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(fs::read_to_string(source_and_output).unwrap(), original);
}

#[test]
fn reports_usage_errors_with_exit_code_two() {
    let result = cli().output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&result.stderr).contains("usage: aragon-androidc"));
}
