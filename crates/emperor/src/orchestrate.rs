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
use std::os::unix::fs::PermissionsExt;
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
    pub exercise: bool,
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
        CommandKind::Benchmark => benchmark(invocation),
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

/// Local qualification of one build directory.
///
/// Limitations, stated plainly:
///
/// - This is a LOCAL-RELEASE qualification. It executes everything the
///   current host can execute: the same static verification as `verify`
///   (build report read, sha256 match against `App`, ELF header and program
///   header checks), the headless oracle, and the windowed smoke when a
///   display is reachable.
/// - It does NOT claim the full linux-release matrix. That matrix is a
///   multi-machine run and has not been executed here; the final pipeline
///   stage is reported `BLOCKED` with that reason, never `PASS`.
/// - Windows parity is not executed and not claimed. Signing is not
///   implemented. Both fold into the same blocked final stage.
/// - Exit codes mirror `verify`: 4 on static/ELF failure, 5 on a runtime
///   failure (oracle or a run smoke), and 0 when static verification and the
///   oracle passed even if the smoke stayed `BLOCKED` for lack of a display.
///   A missing display is not a failure, matching `build`'s convention.
/// - `--launch` is honoured in a reduced form: requesting it without a
///   reachable display records an explicit blocked reason in the smoke stage
///   instead of silently skipping. Qualify always attempts the smoke when a
///   display exists; it does not wait for `--launch` to do so.
fn qualify(invocation: Invocation) -> Run {
    let directory = match invocation.directory.clone() {
        Some(path) => path,
        None => {
            return fault_run(
                &invocation,
                Fault::new(2, "EMP-USAGE", "qualify requires a build directory"),
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
    let window = report
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("wayland")
        .to_owned();
    // The smoke runs only when this host can actually open a window. Without
    // a display the stage is BLOCKED, not FAILED: nothing was attempted.
    let (smoke_status, smoke_exit, smoke_detail) = if display_ready(&window) {
        match run_captured(&app, &["--smoke"], Duration::from_secs(25)) {
            Ok(captured) if captured.code == 0 && smoke_matches(&window, &captured.stdout) => {
                (Status::Pass, 0u8, captured.stdout)
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
    } else if invocation.launch {
        (
            Status::Blocked,
            0u8,
            format!(
                "launch was requested and {} is unset; the smoke was not attempted",
                display_env(&window)
            ),
        )
    } else {
        (
            Status::Blocked,
            0u8,
            format!(
                "no {} display; the windowed smoke was not attempted",
                display_name(&window)
            ),
        )
    };
    let exercise = if invocation.exercise {
        match qualify_exercise(&app, &window) {
            Ok(evidence) => Some(evidence),
            Err(fault) => return fault_run(&invocation, fault),
        }
    } else {
        None
    };
    let exercise_status = exercise.as_ref().map(|(status, _)| *status);
    let mut stages = Stages::new();
    for (index, name) in crate::stage::NAMES.iter().enumerate() {
        if *name == "runtime-conformance" {
            if let Some((status, evidence)) = &exercise {
                stages.push(*status, evidence["wayland_script"]["detail"].as_str().unwrap_or_default());
            } else {
                stages.push(smoke_status, smoke_detail.clone());
            }
        } else if name == &"static-verification" {
            stages.push(
                Status::Pass,
                format!("sha256 {digest} matches the build report"),
            );
        } else if index < 11 {
            stages.push(Status::NotApplicable, "qualify does not repeat admission");
        } else {
            stages.push(
                Status::Blocked,
                "the multi-machine linux-release matrix, Windows parity, and signing were not executed on this host",
            );
        }
    }
    let exit = if exercise_status == Some(Status::Blocked) {
        3
    } else if smoke_status == Status::Pass {
        0
    } else {
        smoke_exit
    };
    let mut result = finish(
        &invocation,
        exit,
        stages,
        format!("qualified {digest}\n{}\n", oracle.stdout.trim()),
        json!({
            "artifact_sha256": digest,
            "oracle_stdout": oracle.stdout,
            "smoke": smoke_status.as_str().to_lowercase(),
            "smoke_detail": smoke_detail,
            "qualification_scope": "local-release",
            "window": window,
            "target": target,
            "release": release,
        }),
    );
    if let Some((status, evidence)) = exercise {
        result.json["interaction_exercise"] = evidence;
        result.text.push_str(&format!("Wayland scripted exercise {}\n", status.as_str()));
    }
    result
}

fn qualify_exercise(app: &Path, window: &str) -> Result<(Status, Value), Fault> {
    let synthetic = capture_exercise(app, false)?;
    let (status, stdout, detail) = if window != "wayland" {
        (Status::Blocked, None, "scripted Wayland exercise requires a Wayland App")
    } else if !display_ready(window) {
        (Status::Blocked, None, "WAYLAND_DISPLAY is unset; scripted Wayland frame was not attempted")
    } else {
        (Status::Pass, Some(capture_exercise(app, true)?), "scripted counter matched the headless oracle and committed one Wayland buffer")
    };
    Ok((status, json!({
        "headless_script": {"status": "pass", "stdout": synthetic},
        "wayland_script": {"status": status.as_str().to_lowercase(), "stdout": stdout, "detail": detail},
        "native_seat_input": {"status": "not-exercised"},
    })))
}

fn capture_exercise(app: &Path, wayland: bool) -> Result<String, Fault> {
    let (mode, window_report) = if wayland {
        ("--exercise", "presented=1 wayland=opened window=wayland")
    } else {
        ("--exercise-oracle", "presented=0 wayland=not-requested window=not-requested")
    };
    let captured = run_captured(app, &[mode], Duration::from_secs(25))?;
    let expected = format!("count=3 revision=2 {window_report} copy=true paste=false edit=2");
    if captured.code == 0 && captured.stdout.trim_end() == expected {
        Ok(captured.stdout)
    } else {
        Err(Fault::new(5, "EMP-RUNTIME", format!(
            "{mode} exited {}\nexpected {expected}\nstdout: {}\nstderr: {}",
            captured.code, captured.stdout, captured.stderr,
        )))
    }
}

/// Local latency benchmark of one build directory.
///
/// What this measures, honestly, on the current host only:
///
/// - Oracle latency: the wall time of `App --oracle`, a subprocess spawn plus
///   the headless oracle pass, timed with `std::time::Instant` around
///   `run_captured`. One warmup run precedes 5 timed runs; min, median, and
///   mean are reported. This is NOT the in-process oracle latency recorded in
///   `evidence/emperor/performance/`; it includes process startup and is a
///   different number. Both are honest; they are not interchangeable.
/// - ELF verification latency: the in-process cost of `hash_and_elf_prefix`
///   plus `elf::parse` plus `elf::check` on `App`, one warmup then 5 timed
///   runs. This is a static structural check, not a security review.
///
/// What this refuses to claim:
///
/// - Windowed frame rate. Frame-rate benchmarking needs a display and a
///   frame-presented counter; the smoke marker records a presented count but
///   there is no frame-timing channel in compiler 0.0.1, so no frame rate is
///   invented. When a display is reachable, the windowed smoke WALL TIME is
///   measured as a third benchmark (1 warmup + 5 runs) — that is a launch
///   latency, not a frame rate, and it is labeled as such.
/// - The multi-machine matrix. `benchmark_scope` is `local-host`; these
///   numbers describe this CPU, these cores, this rustc. Nothing here
///   generalises to other machines and no relative comparison is claimed.
///
/// Exit codes: 0 when every attempted measurement succeeded, 5 when `App`
/// fails to run (warmup or timed run), 2 on usage problems, 4 on the same
/// static failures as `verify` (missing report, hash mismatch, bad ELF).
fn benchmark(invocation: Invocation) -> Run {
    let directory = match invocation.directory.clone() {
        Some(path) => path,
        None => {
            return fault_run(
                &invocation,
                Fault::new(2, "EMP-USAGE", "benchmark requires a build directory"),
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
    let machine = match report
        .get("target")
        .and_then(Value::as_str)
        .and_then(target::elf_machine)
    {
        Some(machine) => machine,
        None => {
            return fault_run(
                &invocation,
                Fault::new(4, "EMP-ELF", "build report has no admitted ELF machine"),
            )
        }
    };
    let release = report
        .get("release")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let window = report
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("wayland")
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
    if let Err(message) =
        elf::parse(&bytes).and_then(|summary| elf::check(&summary, machine, release))
    {
        return fault_run(&invocation, Fault::new(4, "EMP-ELF", message));
    }
    // Oracle latency: one warmup, then 5 timed runs. A non-zero exit at any
    // point is a runtime failure (5), not a slow benchmark.
    if let Err(fault) = run_captured(&app, &["--oracle"], Duration::from_secs(20)) {
        return fault_run(&invocation, fault);
    }
    let mut oracle_runs = Vec::new();
    for _ in 0..BENCH_RUNS {
        let started = Instant::now();
        let captured = match run_captured(&app, &["--oracle"], Duration::from_secs(20)) {
            Ok(captured) => captured,
            Err(fault) => return fault_run(&invocation, fault),
        };
        if captured.code != 0 {
            return fault_run(
                &invocation,
                Fault::new(
                    5,
                    "EMP-RUNTIME",
                    format!("oracle exited {}\n{}", captured.code, captured.stderr),
                ),
            );
        }
        oracle_runs.push(started.elapsed().as_secs_f64());
    }
    // ELF verification latency: in-process, warm cache after the first pass.
    let verify_once = || -> Result<f64, Fault> {
        let started = Instant::now();
        let (bytes, _length, _digest) = hash_and_elf_prefix(&app)?;
        elf::parse(&bytes)
            .and_then(|summary| elf::check(&summary, machine, release))
            .map_err(|message| Fault::new(4, "EMP-ELF", message))
            .map(|_| started.elapsed().as_secs_f64())
    };
    if verify_once().is_err() {
        return fault_run(
            &invocation,
            Fault::new(4, "EMP-ELF", "artifact changed under the benchmark"),
        );
    }
    let mut elf_runs = Vec::new();
    for _ in 0..BENCH_RUNS {
        match verify_once() {
            Ok(seconds) => elf_runs.push(seconds),
            Err(fault) => return fault_run(&invocation, fault),
        }
    }
    // Windowed smoke wall time: measured only when a display is reachable.
    // This is launch latency; frame rate is not measured anywhere below.
    let (smoke_runs, smoke_status, smoke_detail) = if display_ready(&window) {
        match run_captured(&app, &["--smoke"], Duration::from_secs(25)) {
            Ok(captured) if captured.code == 0 && smoke_matches(&window, &captured.stdout) => {}
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
        let mut runs = Vec::new();
        for _ in 0..BENCH_RUNS {
            let started = Instant::now();
            match run_captured(&app, &["--smoke"], Duration::from_secs(25)) {
                Ok(captured) if captured.code == 0 && smoke_matches(&window, &captured.stdout) => {}
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
            runs.push(started.elapsed().as_secs_f64());
        }
        (
            Some(runs),
            Status::Pass,
            "windowed smoke latency measured".to_owned(),
        )
    } else {
        (
            None,
            Status::Blocked,
            format!(
                "no {} display; smoke wall time was not attempted; frame rate is not measurable without a display and a frame-presented counter",
                display_name(&window)
            ),
        )
    };
    // Frame rate: counted from presented frames of a `--live` pump on this
    // display. This is the software Wayland host's rate on the current
    // compositor only — it is not a compositor matrix and not a GPU path.
    let (frame_rate_runs, frame_rate_detail) = if display_ready(&window) {
        // Warmup: short pump so compositor and font caches are hot.
        let warmup = match run_captured(&app, &["--live", "--frames", "30"], Duration::from_secs(60))
        {
            Ok(captured) if captured.code == 0 && presented_frames(&captured.stdout) > 0 => None,
            Ok(captured) => Some(format!(
                "frame-rate warmup exited {} with presented={}",
                captured.code,
                presented_frames(&captured.stdout)
            )),
            Err(fault) => Some(fault.message),
        };
        let mut fps: Vec<f64> = Vec::new();
        let mut failure = warmup.unwrap_or_default();
        for _ in 0..FRAME_RATE_RUNS {
            if !failure.is_empty() {
                break;
            }
            let started = Instant::now();
            match run_captured(&app, &["--live", "--frames", "120"], Duration::from_secs(60)) {
                Ok(captured) if captured.code == 0 => {
                    let presented = presented_frames(&captured.stdout);
                    let elapsed = started.elapsed().as_secs_f64();
                    if presented > 0 && elapsed > 0.0 {
                        fps.push(f64::from(presented) / elapsed);
                    } else if presented == 0 {
                        failure = "presented=0: no frames were presented".to_owned();
                        break;
                    }
                }
                Ok(captured) => {
                    failure = format!("--live exited {}", captured.code);
                    break;
                }
                Err(fault) => {
                    failure = fault.message;
                    break;
                }
            }
        }
        if fps.len() == FRAME_RATE_RUNS {
            (Some(fps), "frame rate measured over --live pumps".to_owned())
        } else {
            (None, failure)
        }
    } else {
        (
            None,
            format!(
                "no {} display; frame rate was not attempted",
                display_name(&window)
            ),
        )
    };
    let oracle_stats = run_stats(&oracle_runs);
    let elf_stats = run_stats(&elf_runs);
    let mut conformance_detail = format!(
        "oracle subprocess wall time over {} runs: min {:.6}s median {:.6}s mean {:.6}s",
        oracle_runs.len(),
        oracle_stats.min,
        oracle_stats.median,
        oracle_stats.mean,
    );
    if let Some(runs) = &smoke_runs {
        let smoke_stats = run_stats(runs);
        conformance_detail.push_str(&format!(
            "; smoke launch wall time over {} runs: min {:.6}s median {:.6}s mean {:.6}s (launch latency, not frame rate)",
            runs.len(),
            smoke_stats.min,
            smoke_stats.median,
            smoke_stats.mean,
        ));
    }
    let mut stages = Stages::new();
    for (index, name) in crate::stage::NAMES.iter().enumerate() {
        if *name == "runtime-conformance" {
            stages.push(Status::Pass, conformance_detail.clone());
        } else if name == &"static-verification" {
            stages.push(
                Status::Pass,
                format!(
                    "sha256 {digest} matches the build report; ELF verification over {} runs: min {:.6}s median {:.6}s mean {:.6}s",
                    elf_runs.len(),
                    elf_stats.min,
                    elf_stats.median,
                    elf_stats.mean,
                ),
            );
        } else if index < 10 {
            stages.push(Status::NotApplicable, "benchmark does not repeat admission");
        } else {
            let mut detail = "the multi-machine matrix was not executed on this host".to_owned();
            if frame_rate_runs.is_some() {
                let stats = run_stats(frame_rate_runs.as_ref().expect("measured"));
                detail = format!(
                    "frame rate over {}-frame --live pumps: min {:.1} fps median {:.1} fps; {}",
                    120, stats.min, stats.median, detail
                );
            } else {
                detail = format!("{}; {}", frame_rate_detail, detail);
            }
            stages.push(Status::Blocked, detail);
        }
    }
    let _ = smoke_status;
    let smoke_json = match &smoke_runs {
        Some(runs) => {
            let stats = run_stats(runs);
            json!({
                "runs_seconds": runs,
                "min_seconds": stats.min,
                "median_seconds": stats.median,
                "mean_seconds": stats.mean,
                "note": "smoke launch wall time, not frame rate",
            })
        }
        None => json!({
            "status": "BLOCKED",
            "reason": smoke_detail,
        }),
    };
    finish(
        &invocation,
        0,
        stages,
        format!(
            "benchmarked {digest}\noracle median {:.6}s elf median {:.6}s\n",
            oracle_stats.median, elf_stats.median,
        ),
        json!({
            "artifact_sha256": digest,
            "benchmark_scope": "local-host",
            "runs_per_measurement": BENCH_RUNS,
            "oracle_latency": {
                "command": "App --oracle",
                "runs_seconds": oracle_runs,
                "min_seconds": oracle_stats.min,
                "median_seconds": oracle_stats.median,
                "mean_seconds": oracle_stats.mean,
            },
            "elf_verification_latency": {
                "in_process": true,
                "runs_seconds": elf_runs,
                "min_seconds": elf_stats.min,
                "median_seconds": elf_stats.median,
                "mean_seconds": elf_stats.mean,
            },
            "smoke_latency": smoke_json,
            "frame_rate": match &frame_rate_runs {
                Some(runs) => {
                    let stats = run_stats(runs);
                    json!({
                        "status": "MEASURED",
                        "command": "App --live --frames 120",
                        "runs_fps": runs,
                        "fps_min": stats.min,
                        "fps_median": stats.median,
                        "frames_per_run": 120,
                        "runs": frame_rate_runs.as_ref().map_or(0, Vec::len),
                    })
                }
                None => json!({
                    "status": "BLOCKED",
                    "reason": frame_rate_detail,
                }),
            },
            "context": machine_context(),
            "window": window,
            "target": report.get("target").cloned().unwrap_or(json!("")),
            "release": release,
        }),
    )
}

/// Fixed run count, matching the `scripts/perf-baseline.sh` convention:
/// 1 warmup (not recorded) followed by 5 timed runs.
const BENCH_RUNS: usize = 5;

/// Frame-rate pumps: 1 warmup pump of 30 frames, then 3 measured pumps of
/// 120 frames. Fewer runs than BENCH_RUNS because each pump presents 120
/// real frames and takes seconds of compositor time.
const FRAME_RATE_RUNS: usize = 3;

/// Extract the presented frame count from an emperor-app report line.
///
/// The report format is `count=… presented=N wayland=…`; parsing the first
/// `presented=` token keeps this independent of field order changes.
fn presented_frames(stdout: &str) -> u32 {
    stdout
        .split_whitespace()
        .find_map(|token| token.strip_prefix("presented="))
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
}

struct RunSummary {
    min: f64,
    median: f64,
    mean: f64,
}

fn run_stats(runs: &[f64]) -> RunSummary {
    let mut sorted = runs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    let median = if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    };
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    RunSummary {
        min: sorted[0],
        median,
        mean,
    }
}

/// Machine context for the benchmark report. The CPU model comes from the
/// first `model name` line of `/proc/cpuinfo`; a missing line (non-x86 or a
/// kernel that does not expose it) is recorded as "unknown" rather than
/// guessed. Cores use `available_parallelism`, the same concurrency the
/// measured work could actually use.
fn machine_context() -> Value {
    let cpu_model = fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, model)| model.trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    let cores = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(0);
    json!({
        "cpu_model": cpu_model,
        "cores": cores,
        "rustc": rustc_version(),
    })
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
    match format.as_str() {
        // The .deb format is produced for real on hosts that ship dpkg-deb;
        // the remaining bundle formats stay honest blocks with the precise
        // reason observed on this host rather than a blanket version note.
        "deb" => return package_deb(invocation, &format),
        "appimage" => {
            return blocked_dir(
                invocation,
                "EMP-PACKAGE",
                "appimage is blocked: no appimagetool on this host and downloads are outside the offline workspace policy",
            )
        }
        "flatpak" => return package_flatpak_blocked(invocation),
        "rpm" => {
            return blocked_dir(
                invocation,
                "EMP-PACKAGE",
                "rpm is blocked: no rpmbuild on this host",
            )
        }
        _ => {}
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

/// The Debian package name used for every emperor counter build. The build
/// directory name varies with the output flag, but the installed package
/// identity must be stable so that upgrades replace, not duplicate.
const DEB_PACKAGE_NAME: &str = "emperor-counter";

/// The Maintainer field Debian policy requires on every control file. It
/// points at the compiler project, not at any individual author.
const DEB_MAINTAINER: &str = "Emperor Compiler <emperor@goya.invalid>";

/// Map the recorded target triple to Debian's architecture vocabulary. Only
/// the two ELF machines the compiler admits have a Debian mapping; anything
/// else is refused rather than guessed at.
fn deb_architecture(target: &str) -> Option<&'static str> {
    match target::elf_machine(target) {
        Some(0x3E) => Some("amd64"),
        Some(0xB7) => Some("arm64"),
        _ => None,
    }
}

/// Package the build directory as a real Debian archive using dpkg-deb.
///
/// The payload mirrors what the linux-archive ships, laid out per Debian
/// policy: the linked ELF at /usr/local/bin/App (mode 755), the desktop
/// entry under /usr/local/share/applications, and a minimal copyright note
/// pointing at the workspace licenses. Failure of dpkg-deb — missing binary
/// or non-zero exit — is reported as an honest EMP-TOOLCHAIN fault, never
/// as a fabricated success.
fn package_deb(invocation: Invocation, format: &str) -> Run {
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
    let desktop_source = directory.join("counter.desktop");
    if !desktop_source.is_file() {
        return fault_run(
            &invocation,
            Fault::new(2, "EMP-DESKTOP", "package directory has no counter.desktop"),
        );
    }
    let architecture = match deb_architecture(&invocation.target) {
        Some(architecture) => architecture,
        None => {
            return fault_run(
                &invocation,
                Fault::new(
                    2,
                    "EMP-TOOLCHAIN",
                    format!(
                        "target {} has no Debian architecture mapping",
                        invocation.target
                    ),
                ),
            )
        }
    };
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
    let output = parent.join(format!("{}.deb", name.to_string_lossy()));
    if let Err(fault) = reject_traversal(&output) {
        return fault_run(&invocation, fault);
    }
    // Staging lives inside the build directory so every write stays inside
    // the traversal-checked tree; it is removed again once the archive is
    // built (or failed), leaving no residue in the deliverable directory.
    // Claim the staging path atomically instead of deleting a pre-existing
    // path: it may contain user data, or be a symlink into another tree.
    let staging = directory.join(".deb-staging");
    if let Err(error) = fs::create_dir(&staging) {
        return fault_run(
            &invocation,
            Fault::new(
                2,
                "EMP-PACKAGE",
                format!("could not claim staging directory {}: {error}", staging.display()),
            ),
        );
    }
    if let Err(fault) = stage_deb(&staging, &app, &desktop_source, architecture) {
        let _ = fs::remove_dir_all(&staging);
        return fault_run(&invocation, fault);
    }
    let built = Command::new("dpkg-deb")
        .arg("--build")
        .arg("--root-owner-group")
        .arg(&staging)
        .arg(&output)
        .output();
    let _ = fs::remove_dir_all(&staging);
    match built {
        Ok(built) if built.status.success() => {
            let size = match fs::metadata(&output) {
                Ok(metadata) if metadata.len() > 0 => metadata.len(),
                _ => {
                    return fault_run(
                        &invocation,
                        Fault::new(
                            2,
                            "EMP-TOOLCHAIN",
                            format!("dpkg-deb produced no file at {}", output.display()),
                        ),
                    )
                }
            };
            let stages = stages_with_last(Status::Pass, format!("wrote {}", output.display()));
            finish(
                &invocation,
                0,
                stages,
                format!("wrote {}\n", output.display()),
                json!({
                    "deb": output.display().to_string(),
                    "format": format,
                    "size_bytes": size,
                }),
            )
        }
        Ok(built) => fault_run(
            &invocation,
            Fault::new(
                2,
                "EMP-TOOLCHAIN",
                format!(
                    "dpkg-deb failed: {}",
                    String::from_utf8_lossy(&built.stderr)
                ),
            ),
        ),
        Err(error) => fault_run(
            &invocation,
            Fault::new(
                2,
                "EMP-TOOLCHAIN",
                format!("could not start dpkg-deb: {error}"),
            ),
        ),
    }
}

/// Populate the Debian control tree: metadata in DEBIAN/control, the ELF
/// payload, the desktop entry, and the copyright note. Any write or copy
/// failure is surfaced as an EMP-TOOLCHAIN fault so a half-built tree can
/// never reach dpkg-deb.
fn stage_deb(
    staging: &Path,
    app: &Path,
    desktop_source: &Path,
    architecture: &str,
) -> Result<(), Fault> {
    fs::create_dir_all(staging.join("DEBIAN")).map_err(io_fault)?;
    fs::create_dir_all(staging.join("usr/local/bin")).map_err(io_fault)?;
    fs::create_dir_all(staging.join("usr/local/share/applications")).map_err(io_fault)?;
    fs::create_dir_all(staging.join("usr/local/share/doc/emperor-counter")).map_err(io_fault)?;
    // Debian requires each Description continuation line to start with a
    // single space; the whole field is kept on one literal so the required
    // leading space survives formatting.
    let control = format!(
        "Package: {DEB_PACKAGE_NAME}\nVersion: {COMPILER_VERSION}\nArchitecture: {architecture}\nMaintainer: {DEB_MAINTAINER}\nSection: utils\nDescription: Counter application produced by the Emperor compiler\n Delivered as a real .deb by dpkg-deb from the workspace build tree.\n"
    );
    fs::write(staging.join("DEBIAN/control"), control).map_err(io_fault)?;
    fs::copy(app, staging.join("usr/local/bin/App")).map_err(io_fault)?;
    fs::set_permissions(
        staging.join("usr/local/bin/App"),
        std::fs::Permissions::from_mode(0o755),
    )
    .map_err(io_fault)?;
    fs::copy(
        desktop_source,
        staging.join("usr/local/share/applications/counter.desktop"),
    )
    .map_err(io_fault)?;
    let copyright = format!(
        "emperor-counter is built by the Emperor compiler (version {COMPILER_VERSION}).\n\
         The workspace licenses live at LICENSE.txt, LICENSE-MIT, and LICENSE-APACHE\n\
         in the goya workspace root; this package carries no vendored third-party\n\
         code beyond the runtime crates already recorded in sbom.json.\n"
    );
    fs::write(
        staging.join("usr/local/share/doc/emperor-counter/copyright"),
        copyright,
    )
    .map_err(io_fault)?;
    Ok(())
}

/// The flatpak block states what was actually observed on this host rather
/// than a generic version note. The host has runtimes installed (freedesktop,
/// GNOME, and KDE platforms appear in `flatpak list --runtime`), so the honest
/// blocker is the missing build tool: no flatpak-builder, and downloading it
/// would violate the offline workspace policy.
fn package_flatpak_blocked(invocation: Invocation) -> Run {
    let runtimes = Command::new("flatpak")
        .arg("list")
        .arg("--runtime")
        .arg("--columns=application")
        .output();
    let detail = match runtimes {
        Ok(output) if output.status.success() => {
            let list = String::from_utf8_lossy(&output.stdout);
            let count = list.lines().filter(|line| !line.trim().is_empty()).count();
            format!(
                "flatpak is blocked: no flatpak-builder on this host; `flatpak list --runtime` shows {count} runtimes installed, but downloading the builder is outside the offline workspace policy"
            )
        }
        Ok(output) => format!(
            "flatpak is blocked: `flatpak list --runtime` exited {} and no flatpak-builder is on this host",
            output.status
        ),
        Err(error) => format!(
            "flatpak is blocked: flatpak could not be run ({error}) and no flatpak-builder is on this host"
        ),
    };
    blocked_dir(invocation, "EMP-PACKAGE", &detail)
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
        // Carry the diagnostic code like fault_run does, so callers can
        // distinguish BLOCKED reasons in JSON without parsing text.
        json!({"diagnostic": code}),
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
        // Carry the diagnostic code like fault_run does, so callers can
        // distinguish BLOCKED reasons in JSON without parsing text.
        json!({"diagnostic": code}),
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
    // Bound the bytes actually read as well as the earlier metadata check.
    // The file can grow between metadata() and read_to_string(); taking at
    // most limit + 1 lets us detect that race without allocating an
    // unbounded buffer for compiler input.
    let limit = sash::admit::MAX_SOURCE_BYTES as u64;
    let file = fs::File::open(path).map_err(|error| {
        Fault::new(
            1,
            "EMP-PATH",
            format!("could not read {}: {error}", path.display()),
        )
    })?;
    let mut source = String::new();
    file.take(limit + 1)
        .read_to_string(&mut source)
        .map_err(|error| {
            Fault::new(
                1,
                "EMP-PATH",
                format!("could not read {}: {error}", path.display()),
            )
        })?;
    if source.len() as u64 > limit {
        return Err(Fault::new(
            1,
            "SASH-PARSE",
            "scene exceeds the admission bound",
        ));
    }
    Ok(source)
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
    let out_thread = std::thread::spawn(move || read_captured_output(stdout));
    let err_thread = std::thread::spawn(move || read_captured_output(stderr));
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

/// Keep child diagnostics bounded while continuing to drain the pipe so a
/// noisy child cannot exhaust compiler memory or block on a full pipe.
fn read_captured_output(mut reader: impl Read) -> String {
    const MAX_CAPTURE_BYTES: usize = 1024 * 1024;
    let mut bytes = Vec::with_capacity(MAX_CAPTURE_BYTES);
    let _ = reader
        .by_ref()
        .take((MAX_CAPTURE_BYTES + 1) as u64)
        .read_to_end(&mut bytes);
    let truncated = bytes.len() > MAX_CAPTURE_BYTES;
    bytes.truncate(MAX_CAPTURE_BYTES);
    if truncated {
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated {
        text.push_str("\n[child output truncated after 1048576 bytes]");
    }
    text
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
