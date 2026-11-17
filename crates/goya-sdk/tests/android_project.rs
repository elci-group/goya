use std::{
    collections::BTreeMap,
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
            "goya-android-project-{}-{}",
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
    Command::new(env!("CARGO_BIN_EXE_aragon-android-project"))
}

fn files(root: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &std::path::Path, at: &std::path::Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(at).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), out);
            } else {
                out.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn emits_a_deterministic_minimal_android_project() {
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    fs::write(
        &source,
        r#"@ontology demo { @cell label { id: "label"; text: "Hello Android" } }"#,
    )
    .unwrap();
    let first = scratch.path("first");
    let second = scratch.path("second");
    for output in [&first, &second] {
        let result = cli().arg(&source).arg(output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let first_files = files(&first);
    assert_eq!(first_files, files(&second));
    assert_eq!(first_files.len(), 12);
    let preview = String::from_utf8(
        first_files["app/src/main/java/dev/goya/preview/GoyaStaticTextPreview.kt"].clone(),
    )
    .unwrap();
    assert!(preview.contains("class GoyaStaticTextPreview(context: Context)"));
    assert!(preview.contains("Hello Android"));
    let activity = String::from_utf8(
        first_files["app/src/main/java/dev/goya/preview/MainActivity.kt"].clone(),
    )
    .unwrap();
    assert!(activity.contains("setContentView(GoyaStaticTextPreview(this))"));
}

#[test]
fn rejects_unsupported_scene_without_creating_project() {
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    let output = scratch.path("project");
    fs::write(
        &source,
        r#"@ontology demo { @button action { id: "action"; text: "No" } }"#,
    )
    .unwrap();
    let result = cli().arg(&source).arg(&output).output().unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("scene rejected:"));
    assert!(!output.exists());
}

#[test]
fn refuses_to_replace_an_existing_project_directory() {
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    let output = scratch.path("project");
    fs::write(
        &source,
        r#"@ontology demo { @cell label { id: "label"; text: "Hello" } }"#,
    )
    .unwrap();
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep"), "unchanged").unwrap();
    let result = cli().arg(&source).arg(&output).output().unwrap();
    assert!(!result.status.success());
    assert_eq!(
        fs::read_to_string(output.join("keep")).unwrap(),
        "unchanged"
    );
}

#[test]
fn emits_a_root_button_project_that_dispatches_through_the_d1_runtime() {
    let scratch = Scratch::new();
    let source = scratch.path("button.gscene");
    fs::write(
        &source,
        r#"@ontology demo { @button increment { id: "button.increment"; text: "Increment" } @interaction increment { id: "action.increment"; trigger: button.increment.activate; class: D1; state { read count: int; write count: int; } input { } set count = state.count + 1; } }"#,
    )
    .unwrap();
    let output = scratch.path("button-project");
    let result = cli()
        .arg(&source)
        .arg(&output)
        .arg("--root-button")
        .arg("--initial")
        .arg("count=int:0")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let generated = files(&output);
    assert!(generated.contains_key("app/src/main/java/dev/goya/preview/GoyaD1Runtime.kt"));
    let activity =
        String::from_utf8(generated["app/src/main/java/dev/goya/preview/MainActivity.kt"].clone())
            .unwrap();
    assert!(
        activity.contains("GoyaD1Runtime(mapOf(\"count\" to GoyaD1Runtime.Value.IntValue(0L)))")
    );
    assert!(activity.contains("d1.dispatch(actionId, emptyMap())"));
    let button = String::from_utf8(
        generated["app/src/main/java/dev/goya/preview/GoyaRootButtonPreview.kt"].clone(),
    )
    .unwrap();
    assert!(button.contains("onD1Action(\"action.increment\")"));
}

#[test]
fn root_button_project_requires_exact_explicit_initial_state_before_output() {
    let scratch = Scratch::new();
    let source = scratch.path("button.gscene");
    fs::write(
        &source,
        r#"@ontology demo { @button increment { id: "button.increment"; text: "Increment" } @interaction increment { id: "action.increment"; trigger: button.increment.activate; class: D1; state { write count: int; } input { } set count = 1; } }"#,
    )
    .unwrap();
    for initial in ["other=int:0", "count=text:zero"] {
        let output = scratch.path(initial.replace(':', "-").as_str());
        let result = cli()
            .arg(&source)
            .arg(&output)
            .arg("--root-button")
            .arg("--initial")
            .arg(initial)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("initial state rejected:"));
        assert!(!output.exists());
    }
}

#[test]
fn root_button_project_emits_signed_minimum_as_long_min_value() {
    let scratch = Scratch::new();
    let source = scratch.path("button.gscene");
    fs::write(
        &source,
        r#"@ontology demo { @button increment { id: "button.increment"; text: "Increment" } @interaction increment { id: "action.increment"; trigger: button.increment.activate; class: D1; state { write count: int; } input { } set count = 1; } }"#,
    )
    .unwrap();
    let output = scratch.path("button-project");
    let result = cli()
        .arg(&source)
        .arg(&output)
        .arg("--root-button")
        .arg("--initial")
        .arg("count=int:-9223372036854775808")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let activity =
        fs::read_to_string(output.join("app/src/main/java/dev/goya/preview/MainActivity.kt"))
            .unwrap();
    assert!(activity.contains("GoyaD1Runtime.Value.IntValue(Long.MIN_VALUE)"));
}

#[test]
fn emits_the_locked_gradle_wrapper_and_dependency_checksums() {
    use sha2::{Digest, Sha256};
    let scratch = Scratch::new();
    let source = scratch.path("input.gscene");
    fs::write(
        &source,
        r#"@ontology demo { @cell label { id: "label"; text: "Locked" } }"#,
    )
    .unwrap();
    let output = scratch.path("project");
    assert!(cli().arg(&source).arg(&output).status().unwrap().success());
    let generated = files(&output);
    let hex = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    // Gradle 8.9's published wrapper jar checksum
    // (https://services.gradle.org/distributions/gradle-8.9-wrapper.jar.sha256).
    assert_eq!(
        hex(&generated["gradle/wrapper/gradle-wrapper.jar"]),
        "498495120a03b9a6ab5d155f5de3c8f0d986a449153702fb80fc80e134484f17"
    );
    let properties =
        String::from_utf8(generated["gradle/wrapper/gradle-wrapper.properties"].clone()).unwrap();
    assert!(properties.contains("gradle-8.9-bin.zip"));
    assert!(properties.contains(
        "distributionSha256Sum=d725d707bfabd4dfdc958c624003b3c80accc03f7037b5122c4b1d0ef15cecab"
    ));
    let metadata =
        String::from_utf8(generated["gradle/verification-metadata.xml"].clone()).unwrap();
    assert!(metadata.contains("<verify-metadata>true</verify-metadata>"));
    assert!(metadata.contains(r#"name="gradle" version="8.7.3""#));
    let gradle_properties = String::from_utf8(generated["gradle.properties"].clone()).unwrap();
    assert!(gradle_properties.contains("android.builder.sdkDownload=false"));
    let app = String::from_utf8(generated["app/build.gradle.kts"].clone()).unwrap();
    assert!(app.contains(r#"buildToolsVersion = "34.0.0""#));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(output.join("gradlew"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "gradlew must be executable");
    }
}
