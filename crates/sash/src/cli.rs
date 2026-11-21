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
        "inspect" | "check" | "build" | "verify" | "expectations" => {}
        _ => return usage(),
    }
    let mut positionals = Vec::new();
    let mut target = TARGET_MSVC_X64.to_string();
    let mut target_explicit = false;
    let mut json = false;
    let mut api_floor = crate::api_floor::ApiFloor::Win10;
    let mut floor_explicit = false;
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
            target_explicit = true;
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
        } else if text == "--api-floor" {
            let Some(value) = args.get(index + 1).and_then(|value| value.to_str()) else {
                return cli_error(2, "--api-floor needs a value");
            };
            match crate::api_floor::admit_floor(value) {
                Ok(floor) => {
                    floor_explicit = true;
                    api_floor = floor;
                }
                Err(reason) => {
                    return cli_error(2, &format!("api floor {value} is not admitted: {reason}"))
                }
            }
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
    let Some(admitted) = crate::windows_target::admit_triple(&target) else {
        return refuse(format!(
            "target {target} is not admitted; admitted targets are {}, {}, and {}",
            crate::windows_target::X86.triple,
            crate::windows_target::X64.triple,
            crate::windows_target::ARM64.triple,
        ));
    };
    if profile != PROFILE_PORTABLE {
        return refuse(format!(
            "profile {profile} is not admitted; the only profile is {PROFILE_PORTABLE}"
        ));
    }
    if api_floor.is_legacy() && admitted.machine == crate::windows_target::ARM64.machine {
        return refuse(
            "the win8 api floor admits i686 and x64 only. ARM64 Windows did not exist before \
             Windows 10, so a legacy-floor ARM64 image has no runtime to run on."
                .to_string(),
        );
    }
    let base = serde_json::json!({
        "schema_version": "1",
        "directive": DIRECTIVE_ID,
        "compiler": "sash",
        "operation": command,
        "target": admitted.triple,
        "profile": profile,
        "api_floor": api_floor.label(),
        "presentation_profile": PROFILE,
        "format": if json { "json" } else { "text" },
    });
    match command {
        "inspect" | "check" => {
            let Some(path) = one_path(&positionals) else {
                return cli_error(2, "expected one scene path");
            };
            scene_command(base, path, command == "inspect", api_floor)
        }
        "expectations" => {
            let Some(path) = one_path(&positionals) else {
                return cli_error(2, "expected one scene path");
            };
            expectations_command(base, path, api_floor)
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
            build_command(base, path, &out, admitted, api_floor)
        }
        "verify" => {
            let Some(path) = one_path(&positionals) else {
                return cli_error(2, "expected one executable path");
            };
            verify_command(
                base,
                path,
                evidence.as_deref(),
                target_explicit.then_some(admitted),
                floor_explicit.then_some(api_floor),
            )
        }
        _ => usage(),
    }
}

fn scene_command(
    mut base: serde_json::Value,
    path: &Path,
    inspect: bool,
    api_floor: crate::api_floor::ApiFloor,
) -> Run {
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
            base["api_floor"] = serde_json::json!(api_floor.label());
            base["status"] = serde_json::json!("ADMITTED");
            base["runtime_qualification"] = serde_json::json!("BLOCKED");
            base["release_admission"] = serde_json::json!("BLOCKED");
            base["source_digest"] = serde_json::json!(crate::hex_encode(&source_digest));
            base["semantic_id"] = serde_json::json!(semantic_id);
            base["client_dip"] = serde_json::json!([width_dip, height_dip]);
            base["dispositions"] = serde_json::json!(crate::orchestrate::legacy_dispositions(
                dispositions.clone(),
                api_floor
            )
            .iter()
            .map(|item| {
                serde_json::json!({"subject": item.subject, "disposition": item.disposition, "detail": item.detail})
            })
            .collect::<Vec<_>>());
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

/// Runtime-window expectations, projected from the lowered IR.
///
/// The Phase 0 launch harness consumes this JSON instead of hand-maintained
/// control tables, so a compiler change cannot desynchronize the harness from
/// what the emitted program actually does. The expectations cover what a
/// Windows observer can check without a launch: the window class and title,
/// the native control id, role, semantic id, authored text, and DIP rectangle
/// of every control, and the import closure the PE policy admits. This
/// command does not link and does not launch; runtime qualification stays
/// BLOCKED.
fn expectations_command(
    mut base: serde_json::Value,
    path: &Path,
    api_floor: crate::api_floor::ApiFloor,
) -> Run {
    let source = match read_scene(path) {
        Ok(source) => source,
        Err(diagnostic) => return finish(base, 1, diagnostic, "scene was not read"),
    };
    let document = match admit::open(&source) {
        Ok(SceneTarget::Static(window)) => {
            crate::ir_lower::lower_static_window(&window, api_floor)
        }
        Ok(SceneTarget::Interactive(app)) => {
            crate::ir_lower::lower_interactive_window(&app, api_floor)
        }
        Err(diagnostic) => return finish(base, 1, diagnostic, "scene was not admitted"),
    };
    let window_class = if document.profile.starts_with("sash.win32.static-window") {
        "SashStaticWindow"
    } else {
        "SashInteractiveWindow"
    };
    base["presentation_profile"] = serde_json::json!(document.profile);
    base["api_floor"] = serde_json::json!(api_floor.label());
    base["status"] = serde_json::json!("ADMITTED");
    base["runtime_qualification"] = serde_json::json!("BLOCKED");
    base["release_admission"] = serde_json::json!("BLOCKED");
    base["window"] = serde_json::json!({
        "title": document.window.scene_name,
        "class": window_class,
        "client_dip": [document.window.width_dip, document.window.height_dip],
    });
    base["controls"] = serde_json::json!(document
        .controls
        .iter()
        .map(|control| {
            let role = match control.role {
                sash_ir::ControlRole::Static => "static",
                sash_ir::ControlRole::Edit => "edit",
                sash_ir::ControlRole::Button => "button",
            };
            serde_json::json!({
                "index": control.index,
                "role": role,
                "control_id": control.control_id,
                "semantic_id": control.semantic_id,
                "text": control.text,
                "dip": [
                    control.x_dip,
                    control.y_dip,
                    control.width_dip,
                    control.height_dip,
                ],
            })
        })
        .collect::<Vec<_>>());
    base["imports"] = serde_json::json!({
        "expected": EXPECTED_IMPORTS,
        "must_not_import": MUST_NOT_IMPORT,
    });
    let text = format!(
        "admitted {profile}\n{class} '{title}' {width}x{height} dip, {count} controls\nruntime-qualification: BLOCKED\n",
        profile = document.profile,
        class = window_class,
        title = document.window.scene_name,
        width = document.window.width_dip,
        height = document.window.height_dip,
        count = document.controls.len(),
    );
    Run {
        code: ExitCode::SUCCESS,
        json: base,
        text,
    }
}

/// Import closure the PE policy admits for every generated image.
const EXPECTED_IMPORTS: &[&str] = &[
    "KERNEL32.dll",
    "USER32.dll",
    "GDI32.dll",
    "ntdll.dll",
    "api-ms-win-core-synch-l1-2-0.dll",
];

/// CRT redistributables and other dependencies the static CRT must exclude.
const MUST_NOT_IMPORT: &[&str] = &["MSVCRT", "VCRUNTIME", "ucrtbase", "concrt", "msvcp"];

fn build_command(
    mut base: serde_json::Value,
    scene: &Path,
    out: &Path,
    target: crate::windows_target::WindowsTarget,
    api_floor: crate::api_floor::ApiFloor,
) -> Run {
    let output = match fresh_output(out) {
        Ok(path) => path,
        Err(diagnostic) => return finish(base, 2, diagnostic, "output path was rejected"),
    };
    let source = match read_scene(scene) {
        Ok(source) => source,
        Err(diagnostic) => return finish(base, 1, diagnostic, "scene was not read"),
    };
    match orchestrate::link(&source, &output, target, api_floor) {
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

fn verify_command(
    mut base: serde_json::Value,
    exe: &Path,
    evidence: Option<&Path>,
    requested: Option<crate::windows_target::WindowsTarget>,
    requested_floor: Option<crate::api_floor::ApiFloor>,
) -> Run {
    // The command default is x64. Verify must not report that until the image
    // itself names a machine and a presentation profile.
    base["target"] = serde_json::Value::Null;
    base["presentation_profile"] = serde_json::Value::Null;
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
    if let Some(image) = crate::windows_target::by_machine(info.machine) {
        base["target"] = serde_json::json!(image.triple);
        if let Some(requested) = requested {
            if requested.machine != image.machine {
                problems.push(format!(
                    "image target is {}, not the requested {}",
                    image.triple, requested.triple
                ));
            }
        }
    }
    if let Some(profile) = pe::presentation_profile(&info.manifest) {
        base["presentation_profile"] = serde_json::json!(profile);
        let floor = if pe::manifest_is_legacy(&info.manifest) {
            crate::api_floor::ApiFloor::Win8
        } else {
            crate::api_floor::ApiFloor::Win10
        };
        base["api_floor"] = serde_json::json!(floor.label());
        // An explicit --api-floor that is not the image floor fails, the
        // same way an explicit --target that is not the PE machine fails.
        if let Some(requested_floor) = requested_floor {
            if requested_floor != floor {
                problems.push(format!(
                    "image api floor is {}, not the requested {}",
                    floor.label(),
                    requested_floor.label()
                ));
            }
        }
    } else if let Some(requested_floor) = requested_floor {
        problems.push(format!(
            "the manifest names no Sash presentation profile, so the requested {} floor cannot be confirmed",
            requested_floor.label()
        ));
    }
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
        let target = base["target"]
            .as_str()
            .unwrap_or("unrecognized")
            .to_string();
        let profile = base["presentation_profile"]
            .as_str()
            .unwrap_or("unrecognized")
            .to_string();
        Run {
            code: ExitCode::from(3),
            json: base,
            text: format!(
                "pe-structure: PASS\ntarget: {target}\npresentation-profile: {profile}\nruntime-qualification: BLOCKED\nrelease-admission: BLOCKED\n"
            ),
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
    if report::reproducibility_claim_is_unconfirmed(&report) {
        problems.push("this verifier does not accept a reproducibility pass claim".into());
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
        "usage: sash inspect|check|build|verify|expectations <path> [--target i686-pc-windows-msvc|x86_64-pc-windows-msvc|aarch64-pc-windows-msvc] [--profile portable-native] [--api-floor win10|win8] [--release] [--out dir] [--evidence dir] [--format json]",
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
    if is_reserved_windows_device_name(&name_text) {
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

/// Windows reserves device names even when they have an extension or are
/// followed by dots/spaces. Check the base name so the same output path is
/// refused on Linux cross-builds and native Windows builds.
fn is_reserved_windows_device_name(name: &str) -> bool {
    let normalized = name.trim_end_matches(['.', ' ']);
    let base = normalized.split('.').next().unwrap_or_default();
    let base = base.to_ascii_lowercase();
    matches!(
        base.as_str(),
        "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
    ) || ["com", "lpt"].iter().any(|prefix| {
        base.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
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
        assert_eq!(run.json["target"], TARGET_MSVC_X64);
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

    #[test]
    fn check_records_each_admitted_windows_triple() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        for triple in ["i686-pc-windows-msvc", "aarch64-pc-windows-msvc"] {
            let run = execute(&[
                "sash".into(),
                "check".into(),
                path.into(),
                "--target".into(),
                triple.into(),
                "--format".into(),
                "json".into(),
            ]);
            assert_eq!(run.code, ExitCode::SUCCESS);
            assert_eq!(run.json["status"], "ADMITTED");
            assert_eq!(run.json["runtime_qualification"], "BLOCKED");
            assert_eq!(run.json["target"], triple);
        }
    }

    #[test]
    fn thumb_target_is_rejected() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--target".into(),
            "thumbv7a-pc-windows-msvc".into(),
        ]);
        assert_eq!(run.code, ExitCode::from(1));
        assert_eq!(run.json["diagnostics"][0]["code"], "SASH-DEPLOY");
        let message = run.json["diagnostics"][0]["message"].as_str().unwrap();
        assert!(message.contains("i686-pc-windows-msvc"));
        assert!(message.contains("x86_64-pc-windows-msvc"));
        assert!(message.contains("aarch64-pc-windows-msvc"));
    }

    #[test]
    fn expectations_project_the_static_control_table() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "expectations".into(),
            path.into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        assert_eq!(run.json["status"], "ADMITTED");
        assert_eq!(run.json["runtime_qualification"], "BLOCKED");
        assert_eq!(run.json["api_floor"], "win10");
        assert_eq!(run.json["presentation_profile"], crate::admit::PROFILE);
        assert_eq!(run.json["window"]["title"], "hello");
        assert_eq!(run.json["window"]["class"], "SashStaticWindow");
        assert_eq!(run.json["window"]["client_dip"], serde_json::json!([320, 160]));
        let controls = run.json["controls"].as_array().unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0]["role"], "static");
        assert_eq!(controls[0]["control_id"], 1);
        assert_eq!(controls[0]["semantic_id"], "window.root");
        assert_eq!(controls[0]["text"], "Hello");
        assert_eq!(controls[0]["dip"], serde_json::json!([0, 0, 320, 160]));
        assert_eq!(
            run.json["imports"]["expected"],
            serde_json::json!(EXPECTED_IMPORTS)
        );
    }

    #[test]
    fn expectations_project_the_interactive_control_table() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/interactive-counter.gscene"
        );
        let run = execute(&[
            "sash".into(),
            "expectations".into(),
            path.into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        assert_eq!(
            run.json["presentation_profile"],
            crate::interactive::PROFILE_CLIPBOARD
        );
        assert_eq!(run.json["window"]["title"], "counter");
        assert_eq!(run.json["window"]["class"], "SashInteractiveWindow");
        let controls = run.json["controls"].as_array().unwrap();
        assert_eq!(controls.len(), 4);
        assert_eq!(controls[0]["role"], "static");
        assert_eq!(controls[0]["text"], "Count");
        assert_eq!(controls[1]["role"], "edit");
        assert_eq!(controls[1]["control_id"], 201);
        assert_eq!(controls[1]["semantic_id"], "window.amount");
        assert_eq!(controls[2]["role"], "button");
        assert_eq!(controls[2]["control_id"], 301);
        assert_eq!(controls[2]["semantic_id"], "window.add");
        assert_eq!(controls[3]["role"], "static");
        assert_eq!(controls[3]["semantic_id"], "state.count");
        assert_eq!(controls[3]["text"], "0");
    }

    #[test]
    fn expectations_reject_an_unadmitted_scene() {
        let path = std::env::temp_dir().join(format!(
            "sash-expectations-unknown-{}.gscene",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "@ontology hello { @cell root { id: \"window.root\"; text: \"Hi\"; colur: #fff; extent: (80, 40, 0); } }\n",
        )
        .unwrap();
        let run = execute(&[
            "sash".into(),
            "expectations".into(),
            path.as_os_str().to_os_string(),
            "--format".into(),
            "json".into(),
        ]);
        let _ = std::fs::remove_file(&path);
        assert_eq!(run.code, ExitCode::from(1));
        assert_eq!(run.json["diagnostics"][0]["code"], "GY-E1001");
    }

    #[test]
    fn expectations_project_the_legacy_floor() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "expectations".into(),
            path.into(),
            "--api-floor".into(),
            "win8".into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        assert_eq!(run.json["api_floor"], "win8");
        assert_eq!(run.json["presentation_profile"], crate::admit::PROFILE_LEGACY);
        // The control table is floor-independent: same meaning, other adapter.
        let controls = run.json["controls"].as_array().unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0]["text"], "Hello");
    }

    #[test]
    fn unknown_api_floor_is_refused() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--api-floor".into(),
            "win7".into(),
        ]);
        assert_eq!(run.code, ExitCode::from(2));
        let message = run.json["diagnostics"][0]["message"].as_str().unwrap();
        assert!(message.contains("tier-3"), "{message}");
        assert!(message.contains("does not download a compiler"));
    }

    #[test]
    fn legacy_floor_refuses_arm64() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--target".into(),
            "aarch64-pc-windows-msvc".into(),
            "--api-floor".into(),
            "win8".into(),
        ]);
        assert_eq!(run.code, ExitCode::from(1));
        let message = run.json["diagnostics"][0]["message"].as_str().unwrap();
        assert!(message.contains("i686 and x64 only"), "{message}");
        // The same floor on x64 admits.
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--target".into(),
            "x86_64-pc-windows-msvc".into(),
            "--api-floor".into(),
            "win8".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        assert_eq!(run.json["api_floor"], "win8");
    }

    #[test]
    fn check_reports_honest_legacy_dispositions() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/static-window.gscene");
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--api-floor".into(),
            "win8".into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        let dispositions = run.json["dispositions"].as_array().unwrap();
        let background = dispositions
            .iter()
            .find(|item| item["subject"] == "background")
            .expect("background disposition");
        let detail = background["detail"].as_str().unwrap();
        assert!(!detail.contains("recreated when that DPI changes"), "{detail}");
        assert!(detail.contains("system DPI aware"), "{detail}");
        assert!(dispositions
            .iter()
            .any(|item| item["subject"] == "dpi"
                && item["detail"].as_str().unwrap().contains("GetDeviceCaps")));

        // The win10 check keeps the per-monitor claim.
        let run = execute(&[
            "sash".into(),
            "check".into(),
            path.into(),
            "--format".into(),
            "json".into(),
        ]);
        assert_eq!(run.code, ExitCode::SUCCESS);
        let dispositions = run.json["dispositions"].as_array().unwrap();
        assert!(!dispositions.iter().any(|item| item["subject"] == "dpi"));
        let background = dispositions
            .iter()
            .find(|item| item["subject"] == "background")
            .expect("background disposition");
        assert!(background["detail"]
            .as_str()
            .unwrap()
            .contains("recreated when that DPI changes"));
    }

    #[test]
    fn verify_reports_the_image_machine_and_profile() {
        let dir = std::env::temp_dir();
        let i686 = dir.join(format!("sash-verify-i686-{}.exe", std::process::id()));
        let bytes = pe::synthetic_gui_image(0x014C, 0x010B, 0x0040 | 0x0100 | 0x4000);
        std::fs::write(&i686, &bytes).unwrap();
        let run = execute(&[
            "sash".into(),
            "verify".into(),
            i686.as_os_str().to_os_string(),
            "--format".into(),
            "json".into(),
        ]);
        let _ = std::fs::remove_file(&i686);
        assert_eq!(run.code, ExitCode::from(3));
        assert_eq!(run.json["target"], "i686-pc-windows-msvc");
        assert_eq!(run.json["presentation_profile"], PROFILE);
        assert_eq!(run.json["pe_structure"], "PASS");
        assert_eq!(run.json["runtime_qualification"], "BLOCKED");
    }

    #[test]
    fn verify_asserts_a_requested_api_floor() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("sash-verify-floor-{}.exe", std::process::id()));
        // A win10 image verified against the win8 floor fails on the floor,
        // not on the PE structure.
        let bytes = pe::synthetic_gui_image(0x8664, 0x020B, 0x0020 | 0x0040 | 0x0100 | 0x4000);
        std::fs::write(&path, &bytes).unwrap();
        let run = execute(&[
            "sash".into(),
            "verify".into(),
            path.as_os_str().to_os_string(),
            "--api-floor".into(),
            "win8".into(),
            "--format".into(),
            "json".into(),
        ]);
        let _ = std::fs::remove_file(&path);
        assert_eq!(run.code, ExitCode::from(1));
        assert_eq!(run.json["api_floor"], "win10");
        let message = run.json["diagnostics"][0]["message"].as_str().unwrap();
        assert!(
            message.contains("image api floor is win10, not the requested win8"),
            "{message}"
        );

        // The same image with no floor flag stays BLOCKED-PASS.
        std::fs::write(&path, &bytes).unwrap();
        let run = execute(&[
            "sash".into(),
            "verify".into(),
            path.as_os_str().to_os_string(),
            "--format".into(),
            "json".into(),
        ]);
        let _ = std::fs::remove_file(&path);
        assert_eq!(run.code, ExitCode::from(3));
    }

    #[test]
    fn verify_accepts_the_matching_legacy_floor() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("sash-verify-legacy-floor-{}.exe", std::process::id()));
        let manifest = concat!(
            "level=\"asInvoker\" uiAccess=\"false\" ",
            "<dpiAware>true</dpiAware> ",
            "processorArchitecture=\"amd64\" ",
            "name=\"ELCI.Goya.Sash.StaticWindow.Legacy\" version=\"0.0.1.0\""
        );
        let bytes = pe::synthetic_gui_image_with_manifest(
            0x8664,
            0x020B,
            0x0020 | 0x0040 | 0x0100 | 0x4000,
            &manifest,
        );
        std::fs::write(&path, &bytes).unwrap();
        let run = execute(&[
            "sash".into(),
            "verify".into(),
            path.as_os_str().to_os_string(),
            "--api-floor".into(),
            "win8".into(),
            "--format".into(),
            "json".into(),
        ]);
        let _ = std::fs::remove_file(&path);
        assert_eq!(run.code, ExitCode::from(3));
        assert_eq!(run.json["api_floor"], "win8");
        assert_eq!(
            run.json["presentation_profile"],
            crate::admit::PROFILE_LEGACY
        );
        assert_eq!(run.json["pe_structure"], "PASS");
    }

    #[test]
    fn verify_rejects_a_requested_target_that_is_not_the_image() {
        let dir = std::env::temp_dir();
        let arm64 = dir.join(format!("sash-verify-arm64-{}.exe", std::process::id()));
        let bytes = pe::synthetic_gui_image(0xAA64, 0x020B, 0x0020 | 0x0040 | 0x0100 | 0x4000);
        std::fs::write(&arm64, &bytes).unwrap();
        let run = execute(&[
            "sash".into(),
            "verify".into(),
            arm64.as_os_str().to_os_string(),
            "--target".into(),
            "i686-pc-windows-msvc".into(),
            "--format".into(),
            "json".into(),
        ]);
        let _ = std::fs::remove_file(&arm64);
        assert_eq!(run.code, ExitCode::from(1));
        assert_eq!(run.json["target"], "aarch64-pc-windows-msvc");
        assert_eq!(run.json["presentation_profile"], PROFILE);
        let message = run.json["diagnostics"][0]["message"].as_str().unwrap();
        assert!(message.contains("image target is aarch64-pc-windows-msvc"));
        assert!(message.contains("not the requested i686-pc-windows-msvc"));
    }
}
