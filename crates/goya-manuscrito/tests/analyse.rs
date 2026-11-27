//! Analysis-only inventory conformance: recognised file kinds, sorted
//! deterministic output, generated-directory and symlink skipping, dependency
//! classification, and explicit diagnostics.

mod common;

use common::{full_project, temp_project, write};
use goya_manuscrito::{analyse, REPORT_SCHEMA};

#[test]
fn inventory_is_complete_sorted_and_unclassified() {
    let root = full_project("inventory");
    let report = analyse(&root).expect("analysis succeeds");

    assert_eq!(report.schema, REPORT_SCHEMA);
    assert_eq!(report.mode, "analysis-only");
    assert!(!report.translated);
    assert_eq!(report.package_manifest.as_deref(), Some("package.json"));
    assert_eq!(report.package_name.as_deref(), Some("fixture-app"));

    let kinds: Vec<(&str, &str)> = report
        .source_files
        .iter()
        .map(|file| (file.path.as_str(), file.kind.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("legacy/util.cjs", "javascript"),
            ("scripts/seed.mjs", "javascript"),
            ("src/main.ts", "typescript"),
            ("src/panel.tsx", "typescript-jsx"),
            ("types/app.d.ts", "declaration"),
        ]
    );

    assert_eq!(
        report.tsconfig_files,
        vec!["tsconfig.build.json", "tsconfig.json"]
    );
    assert_eq!(report.package_manifests, vec!["package.json"]);
    assert_eq!(report.lockfiles, vec!["package-lock.json"]);

    let dependencies: Vec<(&str, &str, &str, &str)> = report
        .dependencies
        .iter()
        .map(|dep| {
            (
                dep.name.as_str(),
                dep.version_spec.as_str(),
                dep.declaration.as_str(),
                dep.classification.as_str(),
            )
        })
        .collect();
    assert_eq!(
        dependencies,
        vec![
            ("fsevents", "^2.3.2", "optional", "unclassified"),
            ("left-pad", "^1.3.0", "dependency", "unclassified"),
            ("react", ">=18", "peer", "unclassified"),
            ("typescript", "5.4.5", "development", "unclassified"),
        ]
    );

    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.starts_with("analysis-only: no AST/type analysis")));

    // Two runs must emit byte-identical reports.
    let second = analyse(&root).expect("second analysis succeeds");
    let first_json = serde_json::to_string(&report).expect("serialize first report");
    let second_json = serde_json::to_string(&second).expect("serialize second report");
    assert_eq!(first_json, second_json);
}

#[test]
#[cfg(unix)]
fn scan_skips_generated_directories_and_symlinks() {
    let root = temp_project("skip");
    write(&root, "real-src/keep.ts", "export const keep = 1;\n");
    write(&root, "node_modules/pkg/index.js", "module.exports = 1;\n");
    write(&root, "dist/out.js", "module.exports = 1;\n");
    write(&root, "build/out.js", "module.exports = 1;\n");
    write(&root, "target/out.js", "module.exports = 1;\n");
    write(&root, ".git/config", "[core]\n");
    std::os::unix::fs::symlink(root.join("real-src"), root.join("linked-src"))
        .expect("create directory symlink");
    std::os::unix::fs::symlink(root.join("real-src/keep.ts"), root.join("linked.ts"))
        .expect("create file symlink");

    let report = analyse(&root).expect("analysis succeeds");
    let paths: Vec<&str> = report
        .source_files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(paths, vec!["real-src/keep.ts"]);
}

#[test]
fn missing_manifest_lockfile_and_tsconfig_are_reported() {
    let root = temp_project("bare");
    write(&root, "src/only.ts", "export const only = 1;\n");

    let report = analyse(&root).expect("analysis succeeds");
    assert_eq!(report.package_manifest, None);
    assert_eq!(report.package_name, None);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.starts_with("package.json not found")));
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.starts_with("tsconfig.json not found")));
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.starts_with("no supported package-manager lockfile found")));
}

#[test]
fn missing_project_is_rejected() {
    let root = temp_project("absent");
    let missing = root.join("does-not-exist");
    let error = analyse(&missing).expect_err("missing project fails");
    assert!(error.contains("cannot resolve project"), "error: {error}");
}

#[test]
fn malformed_manifest_is_rejected() {
    let root = temp_project("broken-manifest");
    write(&root, "package.json", "{ this is not json\n");
    let error = analyse(&root).expect_err("malformed manifest fails");
    assert!(error.contains("invalid package.json"), "error: {error}");
}
