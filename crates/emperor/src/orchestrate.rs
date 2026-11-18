//! Build, verify, and package one admitted counter.

use crate::elf;
use crate::lower::{self, Lowered};
use crate::stage::{Stages, Status};
use crate::target::{self, Class};
use crate::{Fault, COMPILER_VERSION, DIRECTIVE_ID, PROFILE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub struct Run {
    pub code: u8,
    pub json: Value,
    pub text: String,
}

pub enum CommandKind {
    Inspect,
    Check,
    Build,
    Verify,
    Test,
    Benchmark,
    Package,
    Qualify,
}

pub struct Invocation {
    pub kind: CommandKind,
    pub scene: Option<PathBuf>,
    pub directory: Option<PathBuf>,
    pub out: Option<PathBuf>,
    pub evidence: Option<PathBuf>,
    pub target: String,
    pub profile: String,
    pub release: bool,
    pub launch: bool,
    pub suite: Option<String>,
    pub package_format: Option<String>,
    /// `wayland` or `kumimanu`. The default is Wayland.
    pub window: String,
}

pub fn execute(invocation: Invocation) -> Run {
    match invocation.kind {
        CommandKind::Inspect | CommandKind::Check => admit_command(invocation),
        CommandKind::Build => build(invocation),
        CommandKind::Verify => verify(invocation),
        CommandKind::Test => test_scene(invocation),
        CommandKind::Benchmark => blocked_scene(
            invocation,
            "EMP-BENCH",
            "benchmark suites are not measured in compiler 0.0.1",
        ),
        CommandKind::Package => package(invocation),
        CommandKind::Qualify => qualify(invocation),
    }
}

fn admit_command(invocation: Invocation) -> Run {
    let scene = match read_required_scene(&invocation) {
        Ok(scene) => scene,
        Err(fault) => return fault_run(&invocation, fault),
    };
    let lowered = match lower::lower(&scene) {
        Ok(lowered) => lowered,
        Err(fault) => return admission_failure(&invocation, fault),
    };
    match gate_target(&invocation) {
        Ok(()) => {}
        Err(fault) => return target_blocked(&invocation, &lowered, fault),
    }
    let mut stages = Stages::new();
    push_admission(&mut stages, &lowered);
    stages.push(
        Status::NotApplicable,
        "no optimisation pass is enabled in compiler 0.0.1",
    );
    stages.fill(Status::Blocked, "check does not link, launch, or package");
    let text = format!(
        "admitted {}\n{} {}x{} dip\ncopy={} paste={}\nruntime-qualification: BLOCKED\n",
        lowered.ir.profile,
        lowered.ir.action_id,
        lowered.ir.width_dip,
        lowered.ir.height_dip,
        lowered.ir.capabilities.copy,
        lowered.ir.capabilities.paste
    );
    finish(&invocation, 0, stages, text, json!({}))
}

fn build(invocation: Invocation) -> Run {
    let out = match invocation.out.clone() {
        Some(path) => path,
        None => {
            return fault_run(
                &invocation,
                Fault::new(2, "EMP-USAGE", "build requires --out"),
            )
        }
    };
    if let Err(fault) = reject_traversal(&out) {
        return fault_run(&invocation, fault);
    }
    if let Some(evidence) = &invocation.evidence {
        if let Err(fault) = reject_traversal(evidence) {
            return fault_run(&invocation, fault);
        }
    }
    let scene = match read_required_scene(&invocation) {
        Ok(scene) => scene,
        Err(fault) => return fault_run(&invocation, fault),
    };
    let lowered = match lower::lower(&scene) {
        Ok(lowered) => lowered,
        Err(fault) => return admission_failure(&invocation, fault),
    };
    if let Err(fault) = gate_target(&invocation) {
        return target_blocked(&invocation, &lowered, fault);
    }
    if let Err(fault) = fs::create_dir_all(&out) {
        return fault_run(
            &invocation,
            Fault::new(
                2,
                "EMP-TOOLCHAIN",
                format!("could not create output: {fault}"),
            ),
        );
    }
    let json_text = lowered.json.clone();
    let embed = lower::embed_rust(&lowered.ir, &json_text);
    let desktop = desktop_entry();
    if let Err(fault) = validate_desktop(&desktop) {
        return fault_run(&invocation, fault);
    }
    if let Err(fault) = write_outputs(
        &out,
        &lowered,
        &json_text,
        &embed,
        &desktop,
        invocation.window.as_str(),
    ) {
        return fault_run(&invocation, fault);
    }
    let root = workspace_root();
    let embed_path = out.join("embed.rs");
    let target_dir = out.join("target");
    if let Err(fault) = cargo_build(
        &root,
        &embed_path,
        &target_dir,
        invocation.release,
        invocation.window.as_str(),
    ) {
        let mut stages = Stages::new();
        push_admission(&mut stages, &lowered);
        stages.push(
            Status::NotApplicable,
            "no optimisation pass is enabled in compiler 0.0.1",
        );
        stages.push(Status::Fail, fault.message.clone());
        stages.fill(Status::NotApplicable, "not started after the link failed");
        return finish(
            &invocation,
            fault.exit,
            stages,
            format!("{fault}\n"),
            json!({"out": out.display().to_string()}),
        );
    }
    let profile_dir = if invocation.release {
        "release"
    } else {
        "debug"
    };
    let produced = target_dir.join(profile_dir).join("emperor-app");
    let app = out.join("App");
    if let Err(error) = fs::copy(&produced, &app) {
        return fault_run(
            &invocation,
            Fault::new(
                2,
                "EMP-TOOLCHAIN",
                format!("could not copy {}: {error}", produced.display()),
            ),
        );
    }
    if let Err(fault) = mark_executable(&app) {
        return fault_run(&invocation, fault);
    }
    let (bytes, length, digest) = match hash_and_elf_prefix(&app) {
        Ok(read) => read,
        Err(fault) => return fault_run(&invocation, fault),
    };
    let machine = target::elf_machine(&invocation.target).unwrap_or(0);
    let elf_note = match elf::parse(&bytes).and_then(|summary| {
        elf::check(&summary, machine, invocation.release).map(|note| (summary, note))
    }) {
        Ok(pair) => pair,
        Err(message) => {
            let mut stages = Stages::new();
            push_linked(&mut stages, &lowered);
            stages.push(Status::Fail, message.clone());
            stages.fill(
                Status::NotApplicable,
                "not started after static verification failed",
            );
            return finish(
                &invocation,
                4,
                stages,
                format!("EMP-ELF: {message}\n"),
                json!({"artifact_sha256": digest}),
            );
        }
    };
    let oracle = match run_captured(&app, &["--oracle"], Duration::from_secs(20)) {
        Ok(captured) if captured.code == 0 => captured,
        Ok(captured) => {
            return runtime_failure(&invocation, &lowered, &digest, &captured, "oracle");
        }
        Err(fault) => return fault_run(&invocation, fault),
    };
    let window = invocation.window.as_str();
    let (smoke_status, smoke_text, exit, conformance) = if display_ready(window) {
        match run_captured(&app, &["--smoke"], Duration::from_secs(25)) {
            Ok(captured) if captured.code == 0 && smoke_matches(window, &captured.stdout) => {
                ("passed", captured.stdout, 0u8, Status::Pass)
            }
            Ok(captured) => {
                return runtime_failure(&invocation, &lowered, &digest, &captured, "smoke");
            }
            Err(fault) => return fault_run(&invocation, fault),
        }
    } else {
        ("blocked", String::new(), 3u8, Status::Blocked)
    };
    let conformance_detail = if conformance == Status::Pass {
        format!("oracle and {window} smoke passed\n{}", oracle.stdout.trim())
    } else {
        format!(
            "no {} display; oracle passed; window launch was not attempted",
            display_name(window)
        )
    };
    let mut stages = Stages::new();
    push_linked(&mut stages, &lowered);
    stages.push(Status::Pass, elf_note.1.clone());
    stages.push(conformance, conformance_detail);
    stages.push(
        Status::NotApplicable,
        "signing is not implemented; package was not requested",
    );
    let report = json!({
        "artifact_sha256": digest,
        "artifact_bytes": length,
        "elf_machine": elf_note.0.machine,
        "elf_type": elf_note.0.elf_type,
        "relro": elf_note.0.relro,
        "executable_stack": elf_note.0.executable_stack,
        "oracle_stdout": oracle.stdout,
        "smoke": smoke_status,
        "smoke_stdout": smoke_text,
        "target": invocation.target,
        "profile": invocation.profile,
        "release": invocation.release,
        "window": invocation.window,
    });
    let body = finish(
        &invocation,
        exit,
        stages,
        format!(
            "wrote {}\nsha256 {digest}\nsmoke {smoke_status}\n",
            app.display()
        ),
        report.clone(),
    );
    if let Err(fault) = write_reports(&out, &invocation, &digest, &body.json) {
        return fault_run(&invocation, fault);
    }
    if let Some(evidence) = &invocation.evidence {
        if let Err(error) = fs::write(evidence, body.json.to_string()) {
            return fault_run(
                &invocation,
                Fault::new(
                    2,
                    "EMP-TOOLCHAIN",
                    format!("could not write evidence: {error}"),
                ),
            );
        }
    }
    body
}

fn verify(invocation: Invocation) -> Run {
    let directory = match invocation.directory.clone() {
        Some(path) => path,
        None => {
            return fault_run(
                &invocation,
                Fault::new(2, "EMP-USAGE", "verify requires a build directory"),
            )
        }
    };
    if let Err(fault) = reject_traversal(&directory) {
        return fault_run(&invocation, fault);
    }
    let app = directory.join("App");
    let report_path = directory.join("build-report.json");
    let report_bytes = match fs::read(&report_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return fault_run(
                &invocation,
                Fault::new(
                    4,
                    "EMP-ELF",
                    format!("could not read {}: {error}", report_path.display()),
                ),
            )
        }
    };
    let report: Value = match serde_json::from_slice(&report_bytes) {
        Ok(value) => value,
        Err(error) => {
            return fault_run(
                &invocation,
                Fault::new(4, "EMP-ELF", format!("build report is not JSON: {error}")),
            )
        }
    };
    let recorded = report
        .get("artifact_sha256")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let (bytes, _length, digest) = match hash_and_elf_prefix(&app) {
        Ok(read) => read,
        Err(fault) => return fault_run(&invocation, fault),
    };
    if digest != recorded {
        return fault_run(
            &invocation,
            Fault::new(
                4,
                "EMP-ELF",
                format!("artifact sha256 {digest} does not match the build report"),
            ),
        );
    }
    let target = report
        .get("target")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let release = report
        .get("release")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let machine = match target::elf_machine(&target) {
        Some(machine) => machine,
        None => {
            return fault_run(
                &invocation,
                Fault::new(4, "EMP-ELF", "build report has no admitted ELF machine"),
            )
        }
    };
    if let Err(message) =
        elf::parse(&bytes).and_then(|summary| elf::check(&summary, machine, release))
    {
        return fault_run(&invocation, Fault::new(4, "EMP-ELF", message));
    }
    let oracle = match run_captured(&app, &["--oracle"], Duration::from_secs(20)) {
        Ok(captured) if captured.code == 0 => captured,
        Ok(captured) => {
            return fault_run(
                &invocation,
                Fault::new(
                    5,
                    "EMP-RUNTIME",
                    format!("oracle exited {}\n{}", captured.code, captured.stderr),
                ),
            )
        }
        Err(fault) => return fault_run(&invocation, fault),
    };
    let recorded_smoke = report.get("smoke").and_then(Value::as_str).unwrap_or("");
    let window = report
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("wayland");
    let (status, exit, detail) = if invocation.launch {
        if !display_ready(window) {
            (
                Status::Blocked,
                3,
                format!("launch was requested and {} is unset", display_env(window)),
            )
        } else {
            match run_captured(&app, &["--smoke"], Duration::from_secs(25)) {
                Ok(captured) if captured.code == 0 && smoke_matches(window, &captured.stdout) => {
                    (Status::Pass, 0, captured.stdout)
                }
                Ok(captured) => {
                    return fault_run(
                        &invocation,
                        Fault::new(
                            5,
                            "EMP-RUNTIME",
                            format!("smoke exited {}\n{}", captured.code, captured.stderr),
                        ),
                    )
                }
                Err(fault) => return fault_run(&invocation, fault),
            }
        }
    } else if recorded_smoke == "passed" {
        (
            Status::Pass,
            0,
            "build report records smoke passed and the ELF hash still matches".to_owned(),
        )
    } else {
        (
            Status::Blocked,
            3,
            format!(
                "oracle passed and the {} launch is not recorded. Headless output is not a windowed launch.",
                display_name(window)
            ),
        )
    };
    let mut stages = Stages::new();
    for (index, name) in crate::stage::NAMES.iter().enumerate() {
        if *name == "runtime-conformance" {
            stages.push(status, detail.clone());
        } else if name == &"static-verification" {
            stages.push(
                Status::Pass,
                format!("sha256 {digest} matches the build report"),
            );
        } else if index < 11 {
            stages.push(Status::NotApplicable, "verify does not repeat admission");
        } else {
            stages.push(Status::NotApplicable, "verify does not sign or package");
        }
    }
    let _ = oracle;
    finish(
        &invocation,
        exit,
        stages,
        format!("verified {digest}\n"),
        json!({"artifact_sha256": digest, "smoke": recorded_smoke, "window": window}),
    )
}

fn test_scene(invocation: Invocation) -> Run {
    let suite = invocation.suite.as_deref().unwrap_or("conformance");
    if suite == "desktop-sota" {
        return blocked_scene(
            invocation,
            "EMP-BENCH",
            "desktop-sota has no measurements in compiler 0.0.1",
        );
    }
    if suite != "conformance" && suite != "interaction" {
        return fault_run(
            &invocation,
            Fault::new(2, "EMP-USAGE", format!("unknown suite {suite}")),
        );
    }
    let scene = match read_required_scene(&invocation) {
        Ok(scene) => scene,
        Err(fault) => return fault_run(&invocation, fault),
    };
    let lowered = match lower::lower(&scene) {
        Ok(lowered) => lowered,
        Err(fault) => return admission_failure(&invocation, fault),
    };
    if suite == "interaction" {
        return interaction_suite(&invocation, &lowered);
    }
    match emperor_runtime::oracle(&lowered.json) {
        Ok(report) if report.count == 1 && report.revision == 1 && !report.paste_allowed => {
            let mut stages = Stages::new();
            push_admission(&mut stages, &lowered);
            stages.fill(
                Status::NotApplicable,
                "the conformance suite is the headless oracle, not a window launch",
            );
            finish(
                &invocation,
                0,
                stages,
                format!(
                    "count={} revision={} paste={}\n",
                    report.count, report.revision, report.paste_allowed
                ),
                json!({"count": report.count, "revision": report.revision}),
            )
        }
        Ok(report) => fault_run(
            &invocation,
            Fault::new(
                5,
                "EMP-RUNTIME",
                format!(
                    "oracle returned count {} revision {}",
                    report.count, report.revision
                ),
            ),
        ),
        Err(message) => fault_run(&invocation, Fault::new(5, "EMP-RUNTIME", message)),
    }
}

fn interaction_suite(invocation: &Invocation, lowered: &Lowered) -> Run {
    match emperor_runtime::exercise_interaction(&lowered.json) {
        Ok(report) if interaction_passed(&report) => {
            let mut stages = Stages::new();
            push_admission(&mut stages, lowered);
            stages.fill(
                Status::NotApplicable,
                "interaction suite is scripted dispatch, not a seat or compositor launch",
            );
            finish(
                invocation,
                0,
                stages,
                format!(
                    "count={} revision={} edit={} focus={}\n",
                    report.count, report.revision, report.edit, report.focus
                ),
                json!({
                    "count": report.count,
                    "revision": report.revision,
                    "edit": report.edit,
                    "focus": report.focus,
                    "paste_ignored": report.paste_ignored,
                    "copy_allowed": report.copy_allowed,
                    "paste_allowed": report.paste_allowed,
                    "add_hit": report.add_hit,
                    "rects_stable": report.rects_stable,
                    "space_activates": report.space_activates,
                }),
            )
        }
        Ok(report) => fault_run(
            invocation,
            Fault::new(5, "EMP-RUNTIME", interaction_fault(&report)),
        ),
        Err(message) => fault_run(invocation, Fault::new(5, "EMP-RUNTIME", message)),
    }
}

fn interaction_passed(report: &emperor_runtime::InteractionReport) -> bool {
    report.count == 3
        && report.revision == 2
        && report.edit == "2"
        && report.focus == "button"
        && report.paste_ignored
        && report.copy_allowed
        && !report.paste_allowed
        && report.add_hit
        && report.rects_stable
        && report.space_activates
}

fn interaction_fault(report: &emperor_runtime::InteractionReport) -> String {
    format!(
        "interaction count {} revision {} edit {} focus {} paste_ignored {} copy {} paste {} add_hit {} rects_stable {} space {}",
        report.count,
        report.revision,
        report.edit,
        report.focus,
        report.paste_ignored,
        report.copy_allowed,
        report.paste_allowed,
        report.add_hit,
        report.rects_stable,
        report.space_activates
    )
}

fn package(invocation: Invocation) -> Run {
    let format = match invocation.package_format.as_deref() {
        Some(format) => format.to_owned(),
        None => {
            return fault_run(
                &invocation,
                Fault::new(2, "EMP-USAGE", "package requires --format"),
            )
        }
    };
    if matches!(format.as_str(), "appimage" | "flatpak" | "deb" | "rpm") {
        return blocked_dir(
            invocation,
            "EMP-PACKAGE",
            &format!("{format} is not produced in compiler 0.0.1"),
        );
    }
    if format != "linux-archive" {
        return fault_run(
            &invocation,
            Fault::new(2, "EMP-USAGE", format!("unknown package format {format}")),
        );
    }
    let directory = match invocation.directory.clone() {
        Some(path) => path,
        None => {
            return fault_run(
                &invocation,
                Fault::new(2, "EMP-USAGE", "package requires a build directory"),
            )
        }
    };
    if let Err(fault) = reject_traversal(&directory) {
        return fault_run(&invocation, fault);
    }
    let app = directory.join("App");
    if !app.is_file() {
        return fault_run(
            &invocation,
            Fault::new(4, "EMP-ELF", "package directory has no App"),
        );
    }
    let Some(name) = directory.file_name().map(|name| name.to_owned()) else {
        return fault_run(
            &invocation,
            Fault::new(2, "EMP-USAGE", "package directory has no name"),
        );
    };
    let parent = match directory.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_owned(),
        _ => PathBuf::from("."),
    };
    let archive = parent.join(format!("{}.tar.gz", name.to_string_lossy()));
    if let Err(fault) = reject_traversal(&archive) {
        return fault_run(&invocation, fault);
    }
    let output = Command::new("tar")
        .arg("-C")
        .arg(&parent)
        // The build directory holds the cargo target cache used to link App.
        // The cache is a build artifact, not a deliverable: it is rebuildable
        // from embed.rs with the workspace toolchain, and including it made
        // archives exceed 150 MiB while the deliverables stay under 20 MiB.
        .arg(format!("--exclude={}/target", name.to_string_lossy()))
        .arg("-czf")
        .arg(&archive)
        .arg(&name)
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let stages = stages_with_last(Status::Pass, format!("wrote {}", archive.display()));
            finish(
                &invocation,
                0,
                stages,
                format!("wrote {}\n", archive.display()),
                json!({"archive": archive.display().to_string(), "format": format}),
            )
        }
        Ok(output) => fault_run(
            &invocation,
            Fault::new(
                2,
                "EMP-TOOLCHAIN",
                format!("tar failed: {}", String::from_utf8_lossy(&output.stderr)),
            ),
        ),
        Err(error) => fault_run(
            &invocation,
            Fault::new(2, "EMP-TOOLCHAIN", format!("could not start tar: {error}")),
        ),
    }
}

fn blocked_scene(invocation: Invocation, code: &'static str, detail: &str) -> Run {
    if let Some(path) = &invocation.scene {
        if let Err(fault) = reject_traversal(path) {
            return fault_run(&invocation, fault);
        }
        if !path.is_file() {
            return fault_run(
                &invocation,
                Fault::new(
                    2,
                    "EMP-USAGE",
                    format!("scene is missing: {}", path.display()),
                ),
            );
        }
    }
    let suite = invocation.suite.as_deref().unwrap_or("conformance");
    if invocation.suite.is_some() && suite != "conformance" && suite != "desktop-sota" {
        return fault_run(
            &invocation,
            Fault::new(2, "EMP-USAGE", format!("unknown suite {suite}")),
        );
    }
    let mut stages = Stages::new();
    stages.push(Status::Blocked, format!("{code}: {detail}"));
    stages.fill(Status::Blocked, detail);
    finish(
        &invocation,
        3,
        stages,
        format!("{code}: {detail}\n"),
        json!({}),
    )
}

fn blocked_dir(invocation: Invocation, code: &'static str, detail: &str) -> Run {
    if let Some(path) = &invocation.directory {
        if let Err(fault) = reject_traversal(path) {
            return fault_run(&invocation, fault);
        }
    }
    let mut stages = Stages::new();
    stages.push(Status::Blocked, format!("{code}: {detail}"));
    stages.fill(Status::Blocked, detail);
    finish(
        &invocation,
        3,
        stages,
        format!("{code}: {detail}\n"),
        json!({}),
    )
}

fn admission_failure(invocation: &Invocation, fault: Fault) -> Run {
    let mut stages = Stages::new();
    let parse = fault.code.contains("PARSE");
    if parse {
        stages.push(Status::Fail, format!("{}: {}", fault.code, fault.message));
        stages.push(Status::NotApplicable, "not started");
    } else {
        stages.push(Status::Pass, "the scene was read");
        stages.push(Status::Fail, format!("{}: {}", fault.code, fault.message));
    }
    stages.fill(Status::NotApplicable, "not started");
    finish(
        invocation,
        fault.exit,
        stages,
        format!("{}: {}\n", fault.code, fault.message),
        json!({"diagnostic": fault.code}),
    )
}

fn target_blocked(invocation: &Invocation, lowered: &Lowered, fault: Fault) -> Run {
    let mut stages = Stages::new();
    push_through_effects(&mut stages, lowered);
    stages.push(Status::Blocked, fault.message.clone());
    stages.fill(
        Status::Blocked,
        "no ELF is produced for this target or profile",
    );
    finish(
        invocation,
        fault.exit,
        stages,
        format!("{}: {}\n", fault.code, fault.message),
        json!({"diagnostic": fault.code}),
    )
}

fn runtime_failure(
    invocation: &Invocation,
    lowered: &Lowered,
    digest: &str,
    captured: &Captured,
    which: &str,
) -> Run {
    let mut stages = Stages::new();
    push_linked(stages_mut(&mut stages), lowered);
    stages.push(
        Status::Pass,
        "static ELF checks passed before the runtime failed",
    );
    stages.push(
        Status::Fail,
        format!("{which} exited {}\n{}", captured.code, captured.stderr),
    );
    stages.push(Status::NotApplicable, "not started");
    finish(
        invocation,
        5,
        stages,
        format!("EMP-RUNTIME: {which} failed\n"),
        json!({"artifact_sha256": digest}),
    )
}

fn stages_mut(stages: &mut Stages) -> &mut Stages {
    stages
}

fn push_through_effects(stages: &mut Stages, lowered: &Lowered) {
    stages.push(
        Status::Pass,
        format!("admitted by {} and Sash", lowered.handoff_contract),
    );
    stages.push(
        Status::Pass,
        format!("interactive profile {}", lowered.sash_profile),
    );
    stages.push(
        Status::Pass,
        format!("canonical handoff {}", lowered.handoff_contract),
    );
    stages.push(
        Status::Pass,
        format!(
            "copy {} paste {} cut {} filesystem none",
            lowered.ir.capabilities.copy,
            lowered.ir.capabilities.paste,
            lowered.ir.capabilities.cut
        ),
    );
}

fn push_admission(stages: &mut Stages, lowered: &Lowered) {
    push_through_effects(stages, lowered);
    stages.push(Status::Pass, format!("host triple {}", host_label()));
    stages.push(
        Status::Pass,
        format!(
            "lowered {} bytes of {}",
            lowered.json.len(),
            lowered.ir.schema
        ),
    );
}

fn push_linked(stages: &mut Stages, lowered: &Lowered) {
    push_admission(stages, lowered);
    stages.push(
        Status::NotApplicable,
        "no optimisation pass is enabled in compiler 0.0.1",
    );
    stages.push(
        Status::Pass,
        "linked emperor-app through the workspace toolchain",
    );
    stages.push(Status::Pass, "rustc produced the ELF");
    stages.push(Status::Pass, "copied the ELF to App");
}

fn stages_with_last(status: Status, detail: String) -> Stages {
    let mut stages = Stages::new();
    while stages.items().len() < 12 {
        stages.push(Status::NotApplicable, "archive packaging does not re-link");
    }
    stages.push(status, detail);
    stages
}

fn finish(invocation: &Invocation, code: u8, stages: Stages, text: String, extra: Value) -> Run {
    let mut body = json!({
        "schema_version": "1",
        "directive": DIRECTIVE_ID,
        "compiler": "emperor",
        "compiler_version": COMPILER_VERSION,
        "operation": operation_name(invocation),
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": code,
        "stages": stages.json(),
    });
    if let (Some(map), Some(extra_map)) = (body.as_object_mut(), extra.as_object()) {
        for (key, value) in extra_map {
            map.insert(key.clone(), value.clone());
        }
    }
    Run {
        code,
        json: body,
        text,
    }
}

fn fault_run(invocation: &Invocation, fault: Fault) -> Run {
    let mut stages = Stages::new();
    let status = if fault.exit == 3 {
        Status::Blocked
    } else {
        Status::Fail
    };
    stages.push(status, format!("{}: {}", fault.code, fault.message));
    stages.fill(Status::NotApplicable, "not started");
    finish(
        invocation,
        fault.exit,
        stages,
        format!("{}: {}\n", fault.code, fault.message),
        json!({"diagnostic": fault.code}),
    )
}

fn operation_name(invocation: &Invocation) -> &'static str {
    match invocation.kind {
        CommandKind::Inspect => "inspect",
        CommandKind::Check => "check",
        CommandKind::Build => "build",
        CommandKind::Verify => "verify",
        CommandKind::Test => "test",
        CommandKind::Benchmark => "benchmark",
        CommandKind::Package => "package",
        CommandKind::Qualify => "qualify",
    }
}

fn gate_target(invocation: &Invocation) -> Result<(), Fault> {
    match target::profile_class(&invocation.profile) {
        Class::Host => {}
        Class::Blocked => {
            return Err(Fault::new(
                3,
                "EMP-PROFILE",
                format!(
                    "profile {} is named and is not implemented; balanced is the only profile",
                    invocation.profile
                ),
            ));
        }
        Class::Cross | Class::Unknown => {
            return Err(Fault::new(
                2,
                "EMP-USAGE",
                format!(
                    "profile {} is not a known Emperor profile",
                    invocation.profile
                ),
            ));
        }
    }
    match target::classify(&invocation.target) {
        Class::Host => Ok(()),
        Class::Cross | Class::Blocked => Err(Fault::new(
            3,
            "EMP-TARGET",
            target::blocked_reason(&invocation.target),
        )),
        Class::Unknown => Err(Fault::new(
            1,
            "EMP-TARGET",
            format!("target {} is not admitted", invocation.target),
        )),
    }
}

fn read_required_scene(invocation: &Invocation) -> Result<String, Fault> {
    let Some(path) = &invocation.scene else {
        return Err(Fault::new(2, "EMP-USAGE", "a scene path is required"));
    };
    reject_traversal(path)?;
    let metadata = fs::metadata(path).map_err(|_| {
        Fault::new(
            2,
            "EMP-USAGE",
            format!("scene is missing: {}", path.display()),
        )
    })?;
    if metadata.len() > sash::admit::MAX_SOURCE_BYTES as u64 {
        return Err(Fault::new(
            1,
            "SASH-PARSE",
            "scene exceeds the admission bound",
        ));
    }
    fs::read_to_string(path).map_err(|error| {
        Fault::new(
            1,
            "EMP-PATH",
            format!("could not read {}: {error}", path.display()),
        )
    })
}

fn reject_traversal(path: &Path) -> Result<(), Fault> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        Err(Fault::new(1, "EMP-PATH", "path must not contain .."))
    } else {
        Ok(())
    }
}

fn write_outputs(
    out: &Path,
    lowered: &Lowered,
    json_text: &str,
    embed: &str,
    desktop: &str,
    window: &str,
) -> Result<(), Fault> {
    fs::write(out.join("nir.json"), json_text).map_err(io_fault)?;
    fs::write(out.join("embed.rs"), embed).map_err(io_fault)?;
    fs::write(out.join("counter.desktop"), desktop).map_err(io_fault)?;
    let capability = json!({
        "schema": "GY-EMP-LINUX/1",
        "profile": PROFILE,
        "source_digest_sha256": lowered.ir.source_digest_sha256,
        "capabilities": {
            "copy": lowered.ir.capabilities.copy,
            "paste": lowered.ir.capabilities.paste,
            "cut": lowered.ir.capabilities.cut,
            "filesystem": "none",
            "network": false,
            "subprocess": false,
            "web_content": false,
        },
        "sandbox": "not enforced",
        "window_backend": window,
    });
    fs::write(out.join("capability-manifest.json"), capability.to_string()).map_err(io_fault)?;
    let mut packages = vec![
        json!({"name": "emperor-runtime"}),
        json!({"name": "goya-parity"}),
        json!({"name": "goya-wayland"}),
        json!({"name": "sash"}),
        json!({"name": "goya-lang"}),
    ];
    if window == "kumimanu" {
        packages.push(json!({"name": "goya-kumimanu"}));
    }
    let sbom = json!({
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "emperor-app",
        "comment": "Direct Emperor runtime edges only. Not a transitive SBOM. No vulnerability review was performed.",
        "packages": packages,
    });
    fs::write(out.join("sbom.json"), sbom.to_string()).map_err(io_fault)?;
    Ok(())
}

fn write_reports(
    out: &Path,
    invocation: &Invocation,
    digest: &str,
    report: &Value,
) -> Result<(), Fault> {
    fs::write(out.join("build-report.json"), report.to_string()).map_err(io_fault)?;
    let provenance = json!({
        "directive": DIRECTIVE_ID,
        "compiler_version": COMPILER_VERSION,
        "source_revision": source_revision(),
        "target": invocation.target,
        "profile": invocation.profile,
        "release": invocation.release,
        "window": invocation.window,
        "artifact_sha256": digest,
        "rustc": rustc_version(),
        "signing": "not performed",
    });
    fs::write(out.join("provenance.json"), provenance.to_string()).map_err(io_fault)?;
    Ok(())
}

fn io_fault(error: std::io::Error) -> Fault {
    Fault::new(2, "EMP-TOOLCHAIN", error.to_string())
}

fn desktop_entry() -> String {
    "[Desktop Entry]\nType=Application\nName=Count\nExec=App\nTerminal=false\nCategories=Utility;\n"
        .to_owned()
}

fn validate_desktop(text: &str) -> Result<(), Fault> {
    for key in ["Type=", "Name=", "Exec=", "Terminal=", "Categories="] {
        if !text.lines().any(|line| line.starts_with(key)) {
            return Err(Fault::new(
                2,
                "EMP-DESKTOP",
                format!("desktop entry is missing {key}"),
            ));
        }
    }
    Ok(())
}

fn cargo_build(
    root: &Path,
    embed: &Path,
    target_dir: &Path,
    release: bool,
    window: &str,
) -> Result<(), Fault> {
    let mut command = Command::new("cargo");
    command
        .arg("build")
        .arg("-p")
        .arg("emperor-runtime")
        .arg("--bin")
        .arg("emperor-app")
        .arg("--offline")
        .arg("--target-dir")
        .arg(target_dir);
    if window == "kumimanu" {
        command.arg("--features").arg("kumimanu");
    }
    if release {
        command.arg("--release");
    } else {
        // Dev-profile builds do not need debug info: the ELF qualification
        // checks (elf::parse) read only the ELF header and program headers,
        // and the debug info is what pushes these binaries past 64 MiB.
        // Skipping it shrinks both link time and the artifact. Release builds
        // keep the workspace profile's `debug = true` untouched.
        command.env("CARGO_PROFILE_DEV_DEBUG", "0");
    }
    command
        .current_dir(root)
        .env("EMPEROR_EMBED", embed)
        .env("EMPEROR_WINDOW_BACKEND", window)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().map_err(|error| {
        Fault::new(
            2,
            "EMP-TOOLCHAIN",
            format!("could not start cargo: {error}"),
        )
    })?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(Fault::new(
            2,
            "EMP-TOOLCHAIN",
            format!("cargo build failed: {}", tail(&stderr)),
        ))
    }
}

/// Hash the whole file and keep the ELF header plus program headers.
///
/// Debug builds of this workspace exceed 64 MiB because they include
/// debug info. The structural check does not need the rest of the file.
fn hash_and_elf_prefix(path: &Path) -> Result<(Vec<u8>, u64, String), Fault> {
    let mut file = fs::File::open(path).map_err(|error| {
        Fault::new(
            4,
            "EMP-ELF",
            format!("could not read {}: {error}", path.display()),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut prefix = Vec::new();
    let mut total = 0u64;
    let mut chunk = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut chunk).map_err(|error| {
            Fault::new(
                4,
                "EMP-ELF",
                format!("could not read {}: {error}", path.display()),
            )
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&chunk[..read]);
        total = total.saturating_add(read as u64);
        let limit = if prefix.len() >= 64 {
            elf_prefix_limit(&prefix)
        } else {
            64
        };
        if prefix.len() < limit {
            let take = (limit - prefix.len()).min(read);
            prefix.extend_from_slice(&chunk[..take]);
            if prefix.len() >= 64 {
                let grown = elf_prefix_limit(&prefix);
                if prefix.len() < grown {
                    let extra = (grown - prefix.len()).min(read - take);
                    prefix.extend_from_slice(&chunk[take..take + extra]);
                }
            }
        }
    }
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((prefix, total, digest))
}

fn elf_prefix_limit(header: &[u8]) -> usize {
    const CAP: usize = 1024 * 1024;
    if header.len() < 64 || header[0..4] != [0x7f, b'E', b'L', b'F'] {
        return 64;
    }
    let mut phoff_bytes = [0u8; 8];
    let mut phentsize_bytes = [0u8; 2];
    let mut phnum_bytes = [0u8; 2];
    phoff_bytes.copy_from_slice(&header[32..40]);
    phentsize_bytes.copy_from_slice(&header[54..56]);
    phnum_bytes.copy_from_slice(&header[56..58]);
    let phoff = u64::from_le_bytes(phoff_bytes);
    let phentsize = u16::from_le_bytes(phentsize_bytes);
    let phnum = u16::from_le_bytes(phnum_bytes);
    let end = phoff
        .saturating_add(u64::from(phentsize).saturating_mul(u64::from(phnum)))
        .max(64)
        .min(CAP as u64);
    end as usize
}

fn mark_executable(path: &Path) -> Result<(), Fault> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).map_err(io_fault)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(io_fault)
}

struct Captured {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run_captured(bin: &Path, args: &[&str], timeout: Duration) -> Result<Captured, Fault> {
    let mut child = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            Fault::new(
                5,
                "EMP-RUNTIME",
                format!("could not start {}: {error}", bin.display()),
            )
        })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Fault::new(5, "EMP-RUNTIME", "the application has no stdout pipe"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| Fault::new(5, "EMP-RUNTIME", "the application has no stderr pipe"))?;
    let out_thread = std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = stdout.read_to_string(&mut buffer);
        buffer
    });
    let err_thread = std::thread::spawn(move || {
        let mut buffer = String::new();
        let _ = stderr.read_to_string(&mut buffer);
        buffer
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                return Err(Fault::new(
                    5,
                    "EMP-RUNTIME",
                    format!("{} exceeded {} seconds", bin.display(), timeout.as_secs()),
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(error) => {
                return Err(Fault::new(5, "EMP-RUNTIME", error.to_string()));
            }
        }
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = err_thread.join().unwrap_or_default();
    Ok(Captured {
        code: status.code().unwrap_or(1),
        stdout,
        stderr,
    })
}

fn wayland_display() -> bool {
    std::env::var("WAYLAND_DISPLAY")
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

fn display_ready(window: &str) -> bool {
    if window == "kumimanu" {
        std::env::var("DISPLAY")
            .map(|value| !value.is_empty())
            .unwrap_or(false)
    } else {
        wayland_display()
    }
}

fn display_name(window: &str) -> &'static str {
    if window == "kumimanu" {
        "X11"
    } else {
        "Wayland"
    }
}

fn display_env(window: &str) -> &'static str {
    if window == "kumimanu" {
        "DISPLAY"
    } else {
        "WAYLAND_DISPLAY"
    }
}

/// Wayland smoke still requires `wayland=opened`. Kumimanu requires its own
/// marker and a presented frame. A missing display is not a pass.
fn smoke_matches(window: &str, stdout: &str) -> bool {
    if window == "kumimanu" {
        stdout.contains("window=kumimanu") && !stdout.contains("presented=0 ")
    } else {
        stdout.contains("wayland=opened")
    }
}

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    dir
}

fn source_revision() -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(workspace_root())
        .output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

fn rustc_version() -> String {
    let output = Command::new("rustc").arg("--version").output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => "unknown".to_owned(),
    }
}

/// Label for the stage line naming the host triple. A `None` host means we are
/// not on an admitted Linux target at all (e.g. macOS or Windows); printing
/// "none" would suggest the check merely failed to run, while "not-linux"
/// tells the user the platform itself is outside Emperor's supported set.
fn host_label() -> String {
    target::host().unwrap_or("not-linux").to_owned()
}

fn tail(text: &str) -> &str {
    const LIMIT: usize = 4096;
    if text.len() <= LIMIT {
        return text;
    }
    let start = text.len() - LIMIT;
    match text.char_indices().find(|(index, _)| *index >= start) {
        Some((index, _)) => &text[index..],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    // display_ready itself is environment-dependent (DISPLAY vs
    // WAYLAND_DISPLAY), so only the pure mappings are asserted here.
    use super::{display_env, display_name, host_label, smoke_matches};

    #[test]
    fn display_helpers_map_kumimanu_to_x11_display() {
        assert_eq!(display_name("kumimanu"), "X11");
        assert_eq!(display_name("wayland"), "Wayland");
        assert_eq!(display_env("kumimanu"), "DISPLAY");
        assert_eq!(display_env("wayland"), "WAYLAND_DISPLAY");
    }

    #[test]
    fn smoke_matches_requires_each_backend_marker() {
        assert!(smoke_matches("wayland", "opened wayland=opened\n"));
        assert!(!smoke_matches("wayland", "window=kumimanu presented=1\n"));
        assert!(smoke_matches(
            "kumimanu",
            "window=kumimanu presented=3 frames\n"
        ));
        assert!(!smoke_matches("kumimanu", "window=wayland presented=3\n"));
        assert!(!smoke_matches(
            "kumimanu",
            "window=kumimanu presented=0 frames\n"
        ));
        assert!(!smoke_matches("kumimanu", "no display\n"));
    }

    #[test]
    fn host_label_never_reports_a_bare_none() {
        // "none" reads as a failed lookup; a missing host triple must be
        // labeled as an unsupported platform instead.
        assert_ne!(host_label(), "none");
    }
}
