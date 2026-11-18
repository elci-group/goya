use std::ffi::OsString;
use std::path::PathBuf;

fn run(args: &[&str]) -> emperor::orchestrate::Run {
    let mut full = vec![OsString::from("emperor")];
    full.extend(args.iter().map(OsString::from));
    emperor::cli::execute(&full)
}

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../sash/fixtures")
        .join(name);
    let path = path.canonicalize().expect("fixture");
    path.display().to_string()
}

#[test]
fn check_counter_exits_zero_and_names_the_profile() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&[
        "check",
        &scene,
        "--target",
        "x86_64-unknown-linux-gnu",
        "--format",
        "json",
    ]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.json["compiler"], "emperor");
    assert_eq!(run.json["directive"], "GY-EMP-001");
    let stages = run.json["stages"].as_array().expect("stages");
    assert_eq!(stages.len(), 13);
    assert_eq!(stages[5]["status"], "PASS");
    assert_eq!(stages[6]["status"], "NOT_APPLICABLE");
    assert_eq!(stages[11]["status"], "BLOCKED");
    assert!(stages.iter().all(|stage| stage["status"] != "blocked"));
}

#[test]
fn static_scene_is_a_source_error() {
    let scene = fixture("static-window.gscene");
    let run = run(&["check", &scene, "--target", "x86_64-unknown-linux-gnu"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert_eq!(run.json["diagnostic"], "SASH-CAP");
}

#[test]
fn musl_target_is_blocked() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["check", &scene, "--target", "x86_64-unknown-linux-musl"]);
    assert_eq!(run.code, 3, "{}", run.text);
    let stages = run.json["stages"].as_array().expect("stages");
    assert!(stages
        .iter()
        .all(|stage| stage["status"] != "PASS" || stage["id"].as_u64().unwrap() < 5));
    assert_eq!(stages[4]["status"], "BLOCKED");
}

#[test]
fn unknown_target_is_rejected() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["check", &scene, "--target", "wasm32-unknown-unknown"]);
    assert_eq!(run.code, 1, "{}", run.text);
}

#[test]
fn parent_directory_output_is_rejected() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["build", &scene, "--out", "dist/../escape"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert_eq!(run.json["diagnostic"], "EMP-PATH");
}

#[test]
fn benchmark_without_a_build_directory_is_a_static_failure() {
    let scene = fixture("interactive-counter.gscene");
    // benchmark is a directory command: a scene path is treated as a build
    // directory, which has no build-report.json, so this is EMP-ELF, not a
    // silently skipped benchmark.
    let run = run(&["benchmark", &scene]);
    assert_eq!(run.code, 4, "{}", run.text);
    assert_eq!(run.json["diagnostic"], "EMP-ELF");
    let rendered = run.json.to_string();
    assert!(!rendered.contains("150 ms"));
}

#[test]
fn benchmark_requires_a_build_directory() {
    let missing = run(&["benchmark"]);
    assert_eq!(missing.code, 2, "{}", missing.text);
    assert_eq!(missing.json["diagnostic"], "EMP-USAGE");
}

/// A real release build plus the measured runs (1 warmup + 5 per benchmark)
/// takes roughly a minute on this host; that cost is accepted so the numbers
/// below are genuine measurements of a genuine artifact, not fixtures.
#[test]
fn benchmark_measures_oracle_and_elf_latency_with_machine_context() {
    let scene = fixture("interactive-counter.gscene");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("benchmark-counter");
    let out_str = out.display().to_string();
    let build = run(&[
        "build",
        &scene,
        "--out",
        &out_str,
        "--release",
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(build.code, 0, "{}", build.text);

    let bench = run(&[
        "benchmark",
        &out_str,
        "--target",
        "x86_64-unknown-linux-gnu",
        "--format",
        "json",
    ]);
    assert_eq!(bench.code, 0, "{}", bench.text);
    assert_eq!(bench.json["benchmark_scope"], "local-host");
    assert_eq!(bench.json["operation"], "benchmark");

    let oracle = &bench.json["oracle_latency"];
    let oracle_runs = oracle["runs_seconds"].as_array().expect("oracle runs");
    assert_eq!(oracle_runs.len(), 5);
    assert!(oracle["min_seconds"].as_f64().expect("min") > 0.0);
    assert!(oracle["median_seconds"].as_f64().expect("median") > 0.0);
    assert!(oracle["mean_seconds"].as_f64().expect("mean") > 0.0);
    let elf = &bench.json["elf_verification_latency"];
    let elf_runs = elf["runs_seconds"].as_array().expect("elf runs");
    assert_eq!(elf_runs.len(), 5);
    assert_eq!(elf["in_process"], true);

    let context = &bench.json["context"];
    let cpu = context["cpu_model"].as_str().unwrap_or("");
    assert!(!cpu.is_empty(), "cpu model context: {}", context);
    assert_ne!(context["cores"].as_u64(), Some(0));
    let rustc = context["rustc"].as_str().unwrap_or("");
    assert!(rustc.starts_with("rustc "), "rustc context: {rustc}");

    // Frame rate is never claimed, with or without a display.
    assert_eq!(bench.json["frame_rate"]["status"], "BLOCKED");
    let smoke = &bench.json["smoke_latency"];
    if smoke.get("runs_seconds").is_some() {
        assert_eq!(
            smoke["runs_seconds"].as_array().expect("smoke runs").len(),
            5
        );
    } else {
        assert_eq!(smoke["status"], "BLOCKED");
    }

    let stages = bench.json["stages"].as_array().expect("stages");
    assert_eq!(stages.len(), 13);
    assert_eq!(stages[10]["name"], "static-verification");
    assert_eq!(stages[10]["status"], "PASS");
    assert_eq!(stages[11]["name"], "runtime-conformance");
    assert_eq!(stages[11]["status"], "PASS");
    assert_eq!(stages[12]["status"], "BLOCKED");
    for stage in stages.iter().take(10) {
        assert_eq!(stage["status"], "NOT_APPLICABLE", "{}", stage["name"]);
    }
}

#[test]
fn interaction_suite_dispatches_the_admitted_counter() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&["test", &scene, "--suite", "interaction"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.json["count"].as_i64(), Some(3));
    assert_eq!(run.json["revision"].as_u64(), Some(2));
    assert_eq!(run.json["edit"], "2");
    assert_eq!(run.json["focus"], "button");
    assert_eq!(run.json["paste_ignored"], true);
    assert_eq!(run.json["copy_allowed"], true);
    assert_eq!(run.json["paste_allowed"], false);
    assert_eq!(run.json["add_hit"], true);
    assert_eq!(run.json["rects_stable"], true);
    assert_eq!(run.json["space_activates"], true);
    let stages = run.json["stages"].as_array().expect("stages");
    assert_eq!(stages.len(), 13);
    for stage in stages.iter().take(6) {
        assert_eq!(stage["status"], "PASS");
    }
    for stage in stages.iter().skip(6) {
        assert_eq!(stage["status"], "NOT_APPLICABLE");
        assert_eq!(
            stage["detail"],
            "interaction suite is scripted dispatch, not a seat or compositor launch"
        );
    }
    assert_eq!(stages[11]["name"], "runtime-conformance");
    assert_ne!(stages[11]["status"], "PASS");
    assert!(!run.json.to_string().contains("150 ms"));
    assert!(!run.text.contains("wayland"));
}

#[test]
fn format_json_before_the_scene_keeps_one_positional() {
    let scene = fixture("interactive-counter.gscene");
    let run = run(&[
        "check",
        "--format",
        "json",
        &scene,
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.json["output"], "json");
    assert_eq!(run.json["compiler"], "emperor");
    assert_ne!(run.json["diagnostic"], "EMP-USAGE");
}

#[test]
fn format_json_does_not_swallow_the_package_format() {
    let ahead = run(&[
        "package",
        "--format",
        "json",
        "--format",
        "linux-archive",
        "missing-demo-a-package-dir",
    ]);
    assert_eq!(ahead.code, 4, "{}", ahead.text);
    assert_eq!(ahead.json["diagnostic"], "EMP-ELF");
    let behind = run(&[
        "package",
        "--format",
        "linux-archive",
        "--format",
        "json",
        "missing-demo-a-package-dir",
    ]);
    assert_eq!(behind.code, 4, "{}", behind.text);
    assert_eq!(behind.json["diagnostic"], "EMP-ELF");
    let swallowed = run(&["package", "--format", "json", "missing-demo-a-package-dir"]);
    assert_eq!(swallowed.code, 2, "{}", swallowed.text);
    assert_eq!(swallowed.json["diagnostic"], "EMP-USAGE");
    assert!(
        swallowed.text.contains("package requires --format"),
        "{}",
        swallowed.text
    );
}

/// The .deb path is exercised for real: a genuine build supplies the ELF and
/// desktop entry, dpkg-deb turns them into an archive, and dpkg-deb -I is run
/// back over the result so the control fields are read by dpkg itself rather
/// than trusted from our own JSON.
#[test]
fn package_deb_builds_a_real_debian_archive() {
    let scene = fixture("interactive-counter.gscene");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("deb-counter");
    let out_str = out.display().to_string();
    let build = run(&[
        "build",
        &scene,
        "--out",
        &out_str,
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(build.code, 0, "{}", build.text);

    let packaged = run(&["package", "--format", "deb", &out_str]);
    assert_eq!(packaged.code, 0, "{}", packaged.text);
    let deb = packaged.json["deb"].as_str().expect("deb path");
    assert!(deb.ends_with("deb-counter.deb"), "{}", deb);
    let size = packaged.json["size_bytes"].as_u64().expect("size_bytes");
    assert!(size > 0, "size_bytes: {size}");
    let metadata = std::fs::metadata(deb).expect("deb metadata");
    assert_eq!(metadata.len(), size);
    assert!(metadata.len() > 0);

    let inspect = std::process::Command::new("dpkg-deb")
        .arg("-I")
        .arg(deb)
        .output()
        .expect("dpkg-deb -I");
    assert!(inspect.status.success(), "{}", String::from_utf8_lossy(&inspect.stderr));
    let control = String::from_utf8_lossy(&inspect.stdout);
    assert!(control.contains("Package: emperor-counter"), "{}", control);
    assert!(
        control.contains(&format!("Version: {}", emperor::COMPILER_VERSION)),
        "{}",
        control
    );
    assert!(control.contains("Architecture: amd64"), "{}", control);
    assert!(
        control.contains("Maintainer: Emperor Compiler"),
        "{}",
        control
    );
    assert!(control.contains("Section: utils"), "{}", control);
    assert!(control.contains("Description:"), "{}", control);
}

#[test]
fn appimage_flatpak_and_rpm_stay_blocked() {
    for format in ["appimage", "flatpak", "rpm"] {
        let blocked = run(&["package", "--format", format, "missing-demo-a-package-dir"]);
        assert_eq!(blocked.code, 3, "{}: {}", format, blocked.text);
        assert_eq!(blocked.json["diagnostic"], "EMP-PACKAGE", "{}", format);
    }
    let appimage = run(&["package", "--format", "appimage", "missing-demo-a-package-dir"]);
    assert!(
        appimage.text.contains("no appimagetool on this host"),
        "{}",
        appimage.text
    );
    let flatpak = run(&["package", "--format", "flatpak", "missing-demo-a-package-dir"]);
    assert!(
        flatpak.text.contains("no flatpak-builder on this host"),
        "{}",
        flatpak.text
    );
    let rpm = run(&["package", "--format", "rpm", "missing-demo-a-package-dir"]);
    assert!(rpm.text.contains("no rpmbuild on this host"), "{}", rpm.text);
}

#[test]
fn window_backend_accepts_wayland_and_kumimanu_and_rejects_electron() {
    let scene = fixture("interactive-counter.gscene");
    let wayland = run(&[
        "check",
        &scene,
        "--window",
        "wayland",
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(wayland.code, 0, "{}", wayland.text);
    let kumimanu = run(&[
        "check",
        &scene,
        "--window",
        "kumimanu",
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(kumimanu.code, 0, "{}", kumimanu.text);
    assert_ne!(kumimanu.json["diagnostic"], "EMP-USAGE");
    let electron = run(&["check", &scene, "--window", "electron"]);
    assert_eq!(electron.code, 2, "{}", electron.text);
    assert_eq!(electron.json["diagnostic"], "EMP-USAGE");
    assert!(electron.text.contains("unknown window backend"));
    let duplicate = run(&[
        "check", &scene, "--window", "wayland", "--window", "kumimanu",
    ]);
    assert_eq!(duplicate.code, 2, "{}", duplicate.text);
    assert_eq!(duplicate.json["diagnostic"], "EMP-USAGE");
}

#[test]
fn desktop_sota_stays_blocked_and_unknown_suites_exit_2() {
    let scene = fixture("interactive-counter.gscene");
    let blocked = run(&["test", &scene, "--suite", "desktop-sota"]);
    assert_eq!(blocked.code, 3, "{}", blocked.text);
    assert!(!blocked.json.to_string().contains("150 ms"));
    let unknown = run(&["test", &scene, "--suite", "seat"]);
    assert_eq!(unknown.code, 2, "{}", unknown.text);
    let conformance = run(&["test", &scene, "--suite", "conformance"]);
    assert_eq!(conformance.code, 0, "{}", conformance.text);
    assert_eq!(conformance.json["count"].as_i64(), Some(1));
    assert_eq!(conformance.json["revision"].as_u64(), Some(1));
}

/// A real release build takes roughly thirty seconds on this host; that cost
/// is accepted so the qualification below runs against a genuine artifact
/// with a genuine build report, not a fixture.
#[test]
fn qualify_runs_static_checks_and_oracle_and_blocks_the_rest() {
    let scene = fixture("interactive-counter.gscene");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("qualify-counter");
    let out_str = out.display().to_string();
    let build = run(&[
        "build",
        &scene,
        "--out",
        &out_str,
        "--release",
        "--target",
        "x86_64-unknown-linux-gnu",
    ]);
    assert_eq!(build.code, 0, "{}", build.text);

    let qualify = run(&[
        "qualify",
        &out_str,
        "--target",
        "x86_64-unknown-linux-gnu",
        "--format",
        "json",
    ]);
    assert_eq!(qualify.code, 0, "{}", qualify.text);
    assert_eq!(qualify.json["qualification_scope"], "local-release");
    let digest = qualify.json["artifact_sha256"].as_str().expect("sha256");
    assert_eq!(digest.len(), 64);
    assert_eq!(digest, build.json["artifact_sha256"]);
    assert!(qualify.json["oracle_stdout"]
        .as_str()
        .unwrap_or("")
        .contains("count=1"));
    let smoke = qualify.json["smoke"].as_str().unwrap_or("");
    assert!(smoke == "pass" || smoke == "blocked", "smoke: {smoke}");

    let stages = qualify.json["stages"].as_array().expect("stages");
    assert_eq!(stages.len(), 13);
    assert_eq!(stages[10]["name"], "static-verification");
    assert_eq!(stages[10]["status"], "PASS");
    assert_eq!(stages[12]["name"], "packaging-signing-release");
    // The multi-machine matrix, Windows parity, and signing never run here.
    assert_eq!(stages[12]["status"], "BLOCKED");
    for stage in stages.iter().take(10) {
        assert_eq!(stage["status"], "NOT_APPLICABLE", "{}", stage["name"]);
    }
    // runtime-conformance is PASS only when a display was actually reachable;
    // headless environments leave it BLOCKED, which still exits 0.
    let conformance = stages[11]["status"].as_str().unwrap_or("");
    assert!(
        conformance == "PASS" || conformance == "BLOCKED",
        "{}",
        conformance
    );
    if smoke == "blocked" {
        assert_eq!(conformance, "BLOCKED");
    } else {
        assert_eq!(conformance, "PASS");
    }
}
