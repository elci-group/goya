//! Command line for inspect, check, build, and verify.
//!
//! `check` does not link. `build` links and still reports runtime qualification
//! as blocked. `verify` never installs, signs, or launches the executable.

use crate::admit::{self, SceneTarget, PROFILE};
use crate::orchestrate;
use crate::pe;
use crate::report;
use crate::{Diagnostic, DIRECTIVE_ID, PROFILE_PORTABLE, TARGET_MSVC_X64};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const MAX_SCENE: usize = admit::MAX_SOURCE_BYTES;
const MAX_EXE: usize = 64 * 1024 * 1024;
const MAX_REPORT: usize = 1024 * 1024;

pub struct Run {
    pub code: ExitCode,
    pub json: serde_json::Value,
    pub text: String,
}

pub fn dispatch(args: &[std::ffi::OsString]) -> ExitCode {
    let run = execute(args);
    if run.json.get("format").and_then(|value| value.as_str()) == Some("json") {
        println!("{}", run.json);
    } else {
        println!("{}", run.text);
    }
    run.code
}

pub fn execute(args: &[std::ffi::OsString]) -> Run {
    let Some(command) = args.get(1).and_then(|arg| arg.to_str()) else {
        return usage();
    };
    match command {
        "inspect" | "check" | "build" | "verify" => {}
        _ => return usage(),
    }
    let mut positionals = Vec::new();
    let mut target = TARGET_MSVC_X64.to_string();
    let mut json = false;
    let mut profile = PROFILE_PORTABLE.to_string();
    let mut release = false;
    let mut out = None;
    let mut evidence = None;
    let mut index = 2;
    while index < args.len() {
        let arg = &args[index];
        let Some(text) = arg.to_str() else {
            return cli_error(2, "arguments must be UTF-8");
        };
        if text == "--format" {
            let Some(value) = args.get(index + 1).and_then(|value| value.to_str()) else {
                return cli_error(2, "--format needs json");
            };
            if value != "json" {
                return cli_error(2, "--format only accepts json");
            }
            json = true;
            index += 2;
        } else if text == "--target" {
            let Some(value) = args.get(index + 1).and_then(|value| value.to_str()) else {
                return cli_error(2, "--target needs a value");
            };
            target = value.to_string();
            index += 2;
        } else if text == "--profile" {
            let Some(value) = args.get(index + 1).and_then(|value| value.to_str()) else {
                return cli_error(2, "--profile needs a value");
            };
            profile = value.to_string();
            index += 2;
        } else if text == "--out" {
            let Some(value) = args.get(index + 1) else {
                return cli_error(2, "--out needs a path");
            };
            out = Some(PathBuf::from(value));
            index += 2;
        } else if text == "--evidence" {
            let Some(value) = args.get(index + 1) else {
                return cli_error(2, "--evidence needs a path");
            };
            evidence = Some(PathBuf::from(value));
            index += 2;
        } else if text == "--release" {
            release = true;
            index += 1;
        } else if text.starts_with('-') {
            return cli_error(2, "unknown option");
        } else {
            positionals.push(PathBuf::from(arg));
            index += 1;
        }
    }
    if target != TARGET_MSVC_X64 {
        return refuse(format!(
            "target {target} is not admitted; the only linked target is {TARGET_MSVC_X64}"
        ));
    }
    if profile != PROFILE_PORTABLE {
        return refuse(format!(
            "profile {profile} is not admitted; the only profile is {PROFILE_PORTABLE}"
        ));
    }
    let base = serde_json::json!({
        "schema_version": "1",
        "directive": DIRECTIVE_ID,
        "compiler": "sash",
        "operation": command,
        "target": target,
        "profile": profile,
        "presentation_profile": PROFILE,
        "format": if json { "json" } else { "text" },
    });
    match command {
        "inspect" | "check" => {
            let Some(path) = one_path(&positionals) else {
                return cli_error(2, "expected one scene path");
            };
            scene_command(base, path, command == "inspect")
        }
        "build" => {
            let Some(path) = one_path(&positionals) else {
                return cli_error(2, "expected one scene path");
            };
            if !release {
                return cli_error(2, "build requires --release");
            }
            let Some(out) = out else {
                return cli_error(2, "build requires --out");
            };
            build_command(base, path, &out)
        }
        "verify" => {
            let Some(path) = one_path(&positionals) else {
                return cli_error(2, "expected one executable path");
            };
            verify_command(base, path, evidence.as_deref())
        }
        _ => usage(),
    }
}

fn scene_command(mut base: serde_json::Value, path: &Path, inspect: bool) -> Run {
    let source = match read_scene(path) {
        Ok(source) => source,
        Err(diagnostic) => return finish(base, 1, diagnostic, "scene was not read"),
    };
    match admit::open(&source) {
        Ok(target) => {
            let (profile, semantic_id, width_dip, height_dip, source_digest, dispositions) =
                match &target {
                    SceneTarget::Static(window) => (
                        PROFILE,
                        window.semantic_id.as_str(),
                        window.width_dip,
                        window.height_dip,
                        window.source_digest,
                        &window.dispositions,
                    ),
                    SceneTarget::Interactive(app) => (
                        app.profile,
                        app.semantic_id.as_str(),
                        app.width_dip,
                        app.height_dip,
                        app.source_digest,
                        &app.dispositions,
                    ),
                };
            base["presentation_profile"] = serde_json::json!(profile);
            base["status"] = serde_json::json!("ADMITTED");
            base["runtime_qualification"] = serde_json::json!("BLOCKED");
            base["release_admission"] = serde_json::json!("BLOCKED");
            base["source_digest"] = serde_json::json!(crate::hex_encode(&source_digest));
            base["semantic_id"] = serde_json::json!(semantic_id);
            base["client_dip"] = serde_json::json!([width_dip, height_dip]);
            base["dispositions"] = serde_json::json!(dispositions.iter().map(|item| {
                serde_json::json!({"subject": item.subject, "disposition": item.disposition, "detail": item.detail})
            }).collect::<Vec<_>>());
            base["diagnostics"] = serde_json::json!([]);
            let text = format!(
                "admitted {profile}\n{semantic_id} {width_dip}x{height_dip} dip\nruntime-qualification: BLOCKED\n"
            );
            let _ = inspect;
            Run {
                code: ExitCode::SUCCESS,
                json: base,
                text,
            }
        }
        Err(diagnostic) => finish(base, 1, diagnostic, "scene was not admitted"),
    }
}

fn build_command(mut base: serde_json::Value, scene: &Path, out: &Path) -> Run {
    let output = match fresh_output(out) {
        Ok(path) => path,
        Err(diagnostic) => return finish(base, 2, diagnostic, "output path was rejected"),
    };
    let source = match read_scene(scene) {
        Ok(source) => source,
        Err(diagnostic) => return finish(base, 1, diagnostic, "scene was not read"),
    };
    match orchestrate::link(&source, &output) {
        Ok(linked) => {
            base["presentation_profile"] = serde_json::json!(linked.presentation_profile);
            base["status"] = serde_json::json!("LINKED");
            base["runtime_qualification"] = serde_json::json!("BLOCKED");
            base["release_admission"] = serde_json::json!("BLOCKED");
            base["output"] = serde_json::json!(linked.output.join("App.exe").display().to_string());
            base["exe_sha256"] = serde_json::json!(linked.exe_sha256);
            base["imports"] = serde_json::json!(linked.imports);
            base["diagnostics"] = serde_json::json!([]);
            let text = format!(
                "runtime-qualification: BLOCKED\nrelease-admission: BLOCKED\nlinked: {}\npe-structure: PASS\n",
                linked.output.join("App.exe").display()
            );
            Run {
                code: ExitCode::SUCCESS,
                json: base,
                text,
            }
        }
        Err(diagnostic) => {
            let code = if diagnostic.code == "SASH-TOOLCHAIN" {
                3
            } else {
                1
            };
            finish(base, code, diagnostic, "build did not promote App.exe")
        }
    }
}

fn verify_command(mut base: serde_json::Value, exe: &Path, evidence: Option<&Path>) -> Run {
    let bytes = match read_limited(exe, MAX_EXE) {
        Ok(bytes) => bytes,
        Err(diagnostic) => return finish(base, 1, diagnostic, "executable was not read"),
    };
    let info = match pe::parse_pe(&bytes) {
        Ok(info) => info,
        Err(error) => {
            return finish(
                base,
                1,
                Diagnostic::new(
                    "SASH-VERIFY",
                    "qualify",
                    error,
                    "Supply the linked App.exe.",
                ),
                "verify failed",
            );
        }
    };
    let mut problems = pe::policy_errors(&info);
    if let Some(dir) = evidence {
        match evidence_problems(dir, &bytes) {
            Ok(more) => problems.extend(more),
            Err(diagnostic) => return finish(base, 1, diagnostic, "evidence was not accepted"),
        }
    }
    base["pe_structure"] = serde_json::json!(if problems.is_empty() { "PASS" } else { "FAIL" });
    base["runtime_qualification"] = serde_json::json!("BLOCKED");
    base["release_admission"] = serde_json::json!("BLOCKED");
    base["imports"] = serde_json::json!(info.imports);
    base["defenses"] = serde_json::json!(pe::format_flags(info.dll_characteristics));
    if problems.is_empty() {
        base["status"] = serde_json::json!("BLOCKED");
        base["diagnostics"] = serde_json::json!([]);
        Run {
            code: ExitCode::from(3),
            json: base,
            text:
                "pe-structure: PASS\nruntime-qualification: BLOCKED\nrelease-admission: BLOCKED\n"
                    .into(),
        }
    } else {
        finish(
            base,
            1,
            Diagnostic::new(
                "SASH-VERIFY",
                "qualify",
                problems.join("; "),
                "Do not treat this file as a qualified Windows build.",
            ),
            "verify failed",
        )
    }
}

fn evidence_problems(dir: &Path, exe: &[u8]) -> Result<Vec<String>, Diagnostic> {
    if !dir.is_dir() {
        return Err(Diagnostic::new(
            "SASH-VERIFY",
            "qualify",
            "evidence path is not a directory",
            "Pass the directory that contains build-report.json.",
        ));
    }
    let path = dir.join("build-report.json");
    let bytes = read_limited(&path, MAX_REPORT)?;
    let report: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
        Diagnostic::new(
            "SASH-VERIFY",
            "qualify",
            "build report is not JSON",
            "Use the report written by sash build.",
        )
    })?;
    let mut problems = Vec::new();
    match report::reported_exe_hash(&report) {
        Some(expected) if report::sha256_matches(exe, expected) => {}
        Some(_) => problems.push("executable hash does not match the build report".into()),
        None => problems.push("build report has no exe_sha256".into()),
    }
    if report::runtime_claim_is_unconfirmed(&report) {
        problems.push("this verifier does not accept a runtime or release pass claim".into());
    }
    Ok(problems)
}

fn finish(mut base: serde_json::Value, code: u8, diagnostic: Diagnostic, summary: &str) -> Run {
    base["status"] = serde_json::json!(if code == 3 { "BLOCKED" } else { "FAIL" });
    base["runtime_qualification"] = serde_json::json!("BLOCKED");
    base["release_admission"] = serde_json::json!("BLOCKED");
    base["diagnostics"] = serde_json::json!([diagnostic.json()]);
    let text = format!("{summary}\n{diagnostic}\n");
    Run {
        code: ExitCode::from(code),
        json: base,
        text,
    }
}

fn usage() -> Run {
    cli_error(
        2,
        "usage: sash inspect|check|build|verify <path> [--target x86_64-pc-windows-msvc] [--profile portable-native] [--release] [--out dir] [--evidence dir] [--format json]",
    )
}

fn cli_error(code: u8, message: &str) -> Run {
    finish(
        serde_json::json!({
            "schema_version": "1",
            "directive": DIRECTIVE_ID,
            "compiler": "sash",
            "format": "text",
        }),
        code,
        Diagnostic::new(
            "SASH-DEPLOY",
            "cli",
            message,
            "Use an admitted command and target.",
        ),
        "command was not admitted",
    )
}

fn refuse(message: String) -> Run {
    finish(
        serde_json::json!({
            "schema_version": "1",
            "directive": DIRECTIVE_ID,
            "compiler": "sash",
            "format": "text",
        }),
        1,
        Diagnostic::new(
            "SASH-DEPLOY",
            "cli",
            message,
            "Select the admitted target and profile.",
        ),
        "target or profile was rejected",
    )
}

fn one_path(paths: &[PathBuf]) -> Option<&Path> {
    if paths.len() == 1 {
        Some(&paths[0])
    } else {
        None
    }
}

fn read_scene(path: &Path) -> Result<String, Diagnostic> {
    if path_is_hostile(path) {
        return Err(Diagnostic::new(
            "SASH-PARSE",
            "ingest",
            "scene path is not a plain relative or absolute file path",
            "Pass a normal file path without .. components.",
        ));
    }
    let bytes = read_limited(path, MAX_SCENE)?;
    String::from_utf8(bytes).map_err(|_| {
        Diagnostic::new(
            "SASH-PARSE",
            "ingest",
            "scene is not UTF-8",
            "Save the scene as UTF-8 without a UTF-16 BOM.",
        )
    })
}

fn read_limited(path: &Path, max: usize) -> Result<Vec<u8>, Diagnostic> {
    let mut file = File::open(path).map_err(|_| {
        Diagnostic::new(
            "SASH-PARSE",
            "ingest",
            "path could not be opened",
            "Pass an existing readable file.",
        )
    })?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            Diagnostic::new(
                "SASH-PARSE",
                "ingest",
                "path could not be read",
                "Retry with a readable file inside the size bound.",
            )
        })?;
    if bytes.len() > max {
        return Err(Diagnostic::new(
            "SASH-PARSE",
            "ingest",
            format!("file exceeds the {max} byte bound"),
            "Reduce the input or admit a larger bound in a new profile.",
        ));
    }
    Ok(bytes)
}

fn fresh_output(path: &Path) -> Result<PathBuf, Diagnostic> {
    if path_is_hostile(path) || path.file_name().is_none() {
        return Err(Diagnostic::new(
            "SASH-DEPLOY",
            "cli",
            "output path is not a new directory name",
            "Choose a path whose parent exists and whose final component is a normal name.",
        ));
    }
    let name = path.file_name().unwrap();
    let name_text = name.to_string_lossy();
    const RESERVED: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "lpt1", "lpt2"];
    if RESERVED.contains(&name_text.to_ascii_lowercase().as_str()) {
        return Err(Diagnostic::new(
            "SASH-DEPLOY",
            "cli",
            "output name is a reserved Windows device name",
            "Choose another directory name.",
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize().map_err(|_| {
        Diagnostic::new(
            "SASH-DEPLOY",
            "cli",
            "output parent does not exist",
            "Create the parent directory first. Sash does not create parent paths.",
        )
    })?;
    let full = parent.join(name);
    if full.exists() {
        return Err(Diagnostic::new(
            "SASH-DEPLOY",
            "cli",
            "output path already exists",
            "Choose a new directory. Sash does not replace an existing App.exe.",
        ));
    }
    Ok(full)
}

fn path_is_hostile(path: &Path) -> bool {
    path.as_os_str().is_empty()
        || path.components().any(|component| {
            matches!(component, Component::ParentDir)
                || component.as_os_str().to_string_lossy().contains('\0')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_fixture_admits() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        assert_eq!(run.json["status"], "ADMITTED");
        assert_eq!(run.json["runtime_qualification"], "BLOCKED");
        assert_eq!(run.json["client_dip"], serde_json::json!([320, 160]));
        assert_eq!(run.json["presentation_profile"], PROFILE);
    }

    #[test]
    fn interactive_counter_checks_as_its_own_profile() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/interactive-counter.gscene"
        );
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        assert_eq!(
            run.json["presentation_profile"],
            crate::interactive::PROFILE_CLIPBOARD
        );
        assert_eq!(run.json["runtime_qualification"], "BLOCKED");
        assert_eq!(run.json["semantic_id"], "window.root");
    }

    #[test]
    fn unknown_property_fails_check() {
        let path = std::env::temp_dir().join(format!("sash-unknown-{}.gscene", std::process::id()));
        std::fs::write(
            &path,
            "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; colur: #fff; extent: (80, 40, 0); } }\n",
        )
        .unwrap();
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.as_os_str().to_os_string(),
        ]);
        let _ = std::fs::remove_file(&path);
        assert_eq!(run.code, ExitCode::from(1));
        assert_eq!(run.json["diagnostics"][0]["code"], "GY-E1001");
    }

    #[test]
    fn single_exe_profile_is_rejected() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "build".into(),
            path.into(),
            "--profile".into(),
            "single-exe".into(),
            "--release".into(),
            "--out".into(),
            "unused-out".into(),
        ]);
        assert_eq!(run.code, ExitCode::from(1));
        assert_eq!(run.json["diagnostics"][0]["code"], "SASH-DEPLOY");
    }
}
