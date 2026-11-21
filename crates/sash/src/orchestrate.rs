//! Link an admitted scene with the pinned host's Windows cross tools.
//!
//! The output directory is created only after the PE policy checks pass.
//! A failed build does not leave an App.exe behind.

use crate::admit::{self, SceneTarget, ViewportDip};
use crate::emit::Generated;
use crate::interactive;
use crate::pe;
use crate::report::{self, BuildFacts, SceneEvidence};
use crate::windows_target::WindowsTarget;
use crate::{Diagnostic, PROFILE_PORTABLE};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, UNIX_EPOCH};

const MIN_FREE_BYTES: u64 = 1_200_000_000;

pub struct Linked {
    pub output: PathBuf,
    pub exe_sha256: String,
    pub imports: Vec<String>,
    pub presentation_profile: &'static str,
}

pub fn link(
    source: &str,
    output: &Path,
    target: crate::windows_target::WindowsTarget,
    floor: crate::api_floor::ApiFloor,
) -> Result<Linked, Diagnostic> {
    link_with_viewport(source, output, target, floor, None)
}

pub fn link_with_viewport(
    source: &str,
    output: &Path,
    target: WindowsTarget,
    floor: crate::api_floor::ApiFloor,
    viewport: Option<ViewportDip>,
) -> Result<Linked, Diagnostic> {
    if !free_enough(output) {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "the build host does not have enough free disk for a Windows link",
            "Free space and retry. Sash does not delete unrelated build caches.",
        ));
    }
    let mut prepared = prepare(source, floor, viewport)?;
    prepared.generated.manifest_xml = crate::windows_target::stamp_processor_architecture(
        &prepared.generated.manifest_xml,
        target,
    )
    .map_err(|message| {
        Diagnostic::new(
            "SASH-LINK",
            "emit",
            message,
            "Keep one processorArchitecture=\"amd64\" attribute in the manifest template.",
        )
    })?;
    let tools = probe_tools(target)?;
    let cache = pinned_windows_cache(target)?;
    // The directory name is a function of the generated crate and the triple.
    // A unique directory changes the PDB, and `/Brepro` then changes the
    // CodeView GUID embedded in App.exe.
    let key = build_directory_key(
        target.triple,
        &[
            &prepared.source_digest,
            prepared.generated.main_rs.as_bytes(),
            prepared.generated.manifest_xml.as_bytes(),
            prepared.generated.cargo_toml.as_bytes(),
            prepared.generated.build_rs.as_bytes(),
            prepared.generated.resource_script.as_bytes(),
            crate::windows_target::cargo_config(target).as_bytes(),
        ],
    );
    let temp = TempDir::new(&key)?;
    write_project(&temp.path, &prepared.generated, target)?;
    let log = run_link(&temp.path, &cache, target)?;
    let release = temp.path.join("target").join(target.triple).join("release");
    let exe = release.join("app.exe");
    let pdb = release.join("app.pdb");
    if !exe.is_file() {
        return Err(link_error(&log, "the linker did not produce app.exe"));
    }
    if !pdb.is_file() {
        return Err(link_error(
            &log,
            "the linker did not produce private symbols",
        ));
    }
    let exe_bytes = fs::read(&exe).map_err(|error| io_diag("build", &error))?;
    let pdb_bytes = fs::read(&pdb).map_err(|error| io_diag("build", &error))?;
    let info = pe::parse_pe(&exe_bytes).map_err(|error| {
        Diagnostic::new(
            "SASH-VERIFY",
            "qualify",
            error,
            "Inspect the linker output. A file that is not a valid PE is not promoted.",
        )
    })?;
    let problems = pe::policy_errors(&info);
    if !problems.is_empty() {
        return Err(Diagnostic::new(
            "SASH-VERIFY",
            "qualify",
            problems.join("; "),
            "Fix the toolchain flags or the admitted import closure before promoting App.exe.",
        ));
    }
    let facts = BuildFacts {
        target_triple: target.triple.to_string(),
        exe_sha256: crate::sha256_hex(&exe_bytes),
        pdb_sha256: crate::sha256_hex(&pdb_bytes),
        manifest_sha256: crate::sha256_hex(prepared.generated.manifest_xml.as_bytes()),
        generated_sha256: crate::sha256_hex(prepared.generated.main_rs.as_bytes()),
        source_digest: crate::hex_encode(&prepared.source_digest),
        pe: info.clone(),
        rustc: tools.rustc,
        cargo_xwin: tools.cargo_xwin,
        llvm_rc: tools.llvm_rc,
    };
    promote(
        output,
        &exe_bytes,
        &pdb_bytes,
        &prepared.generated,
        &prepared.evidence,
        &facts,
    )?;
    Ok(Linked {
        output: output.to_path_buf(),
        exe_sha256: facts.exe_sha256,
        imports: info.imports,
        presentation_profile: prepared.evidence.presentation_profile,
    })
}

struct Prepared {
    generated: Generated,
    evidence: SceneEvidence,
    source_digest: [u8; 32],
}

/// Amend the evidence dispositions for the win8 API floor.
///
/// The admitted dispositions describe the win10 adapter: the static window's
/// background disposition claims the message font is recreated when the DPI
/// changes, which is false for a system-DPI-aware process that never
/// receives a per-monitor change message. The legacy evidence must describe
/// the adapter the image actually carries, so the static background
/// disposition is rewritten and every legacy image gains an explicit `dpi`
/// disposition. The win10 dispositions pass through untouched.
pub(crate) fn legacy_dispositions(
    mut dispositions: Vec<crate::admit::Disposition>,
    floor: crate::api_floor::ApiFloor,
) -> Vec<crate::admit::Disposition> {
    if !floor.is_legacy() {
        return dispositions;
    }
    const WIN10_BACKGROUND_CLAIM: &str =
        "recreated when that DPI changes";
    const LEGACY_BACKGROUND_CLAIM: &str =
        "laid out at the process DPI, which is system DPI aware and read once; Windows scales the rendered window on other monitors and no per-monitor message exists to handle";
    for disposition in dispositions.iter_mut() {
        if disposition.subject == "background" {
            disposition.detail = disposition
                .detail
                .replace(WIN10_BACKGROUND_CLAIM, LEGACY_BACKGROUND_CLAIM);
        }
    }
    dispositions.push(crate::admit::Disposition {
        subject: "dpi".into(),
        disposition: "equivalently-transformed",
        detail: "The win8 adapter is system DPI aware. SetProcessDPIAware is requested before window creation and the process DPI is read once from GetDeviceCaps(LOGPIXELSY). The window is laid out at that DPI; moving it to a monitor with a different DPI lets Windows scale the rendered window instead of re-laying it out. PerMonitorV2 is not requested. This has not been executed on Windows.".into(),
    });
    dispositions
}

fn prepare(
    source: &str,
    floor: crate::api_floor::ApiFloor,
    viewport: Option<ViewportDip>,
) -> Result<Prepared, Diagnostic> {
    match admit::open_with_viewport(source, viewport)? {
        SceneTarget::Static(window) => {
            let evidence = SceneEvidence {
                presentation_profile: if floor.is_legacy() {
                    admit::PROFILE_LEGACY
                } else {
                    admit::PROFILE
                },
                semantic_id: window.semantic_id.clone(),
                dispositions: legacy_dispositions(window.dispositions.clone(), floor),
                unit_name: "static-window",
                contains: &[
                    "top-level HWND",
                    "one STATIC control per admitted text cell",
                ],
                not_generated: &[
                    "banner communications",
                    "taskbar behaviour",
                    "startup behaviour",
                    "actions",
                    "retention",
                ],
            };
            let source_digest = window.source_digest;
            Ok(Prepared {
                generated: crate::emit::generate(&window, floor),
                evidence,
                source_digest,
            })
        }
        SceneTarget::Interactive(app) => {
            let clipboard = app.profile == interactive::PROFILE_CLIPBOARD;
            let presentation_profile = match (floor, clipboard) {
                (crate::api_floor::ApiFloor::Win8, true) => interactive::PROFILE_CLIPBOARD_LEGACY,
                (crate::api_floor::ApiFloor::Win8, false) => interactive::PROFILE_LEGACY,
                (_, true) => interactive::PROFILE_CLIPBOARD,
                (_, false) => interactive::PROFILE,
            };
            let evidence = SceneEvidence {
                presentation_profile,
                semantic_id: app.semantic_id.clone(),
                dispositions: legacy_dispositions(app.dispositions.clone(), floor),
                unit_name: "interactive-window",
                contains: if clipboard {
                    &[
                        "top-level HWND",
                        "STATIC heading and result",
                        "one EDIT",
                        "one BUTTON",
                        "one D1 integer transition",
                        "authored copy and paste policy on the edit",
                    ]
                } else {
                    &[
                        "top-level HWND",
                        "STATIC heading and result",
                        "one EDIT",
                        "one BUTTON",
                        "one D1 integer transition",
                    ]
                },
                not_generated: if clipboard {
                    &[
                        "banner communications",
                        "taskbar behaviour",
                        "startup behaviour",
                        "retention",
                        "selection policy",
                        "undo policy",
                    ]
                } else {
                    &[
                        "banner communications",
                        "taskbar behaviour",
                        "startup behaviour",
                        "retention",
                        "gesture policy",
                    ]
                },
            };
            let source_digest = app.source_digest;
            Ok(Prepared {
                generated: interactive::generate(&app, floor),
                evidence,
                source_digest,
            })
        }
    }
}

struct Tools {
    rustc: String,
    cargo_xwin: String,
    llvm_rc: String,
}

fn probe_tools(target: WindowsTarget) -> Result<Tools, Diagnostic> {
    let rustc = version("rustc", &["--version"])?;
    let sysroot = version("rustc", &["--print", "sysroot"])?;
    let target_lib = Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(target.triple);
    if !target_lib.is_dir() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            format!(
                "the Rust standard library for {} is not installed",
                target.triple
            ),
            "Install that target with rustup before asking Sash to link. Sash does not download a target.",
        ));
    }
    let cargo_xwin = version("cargo", &["xwin", "--version"])?;
    let llvm_rc = llvm_rc_identity()?;
    // The linker shim is a Python program. It rewrites rustc's random
    // symbols.o path before lld records that path in the PDB.
    let _python = version("python3", &["--version"])?;
    Ok(Tools {
        rustc: rustc.trim().to_string(),
        cargo_xwin: cargo_xwin.trim().to_string(),
        llvm_rc: llvm_rc.trim().to_string(),
    })
}

fn llvm_rc_identity() -> Result<String, Diagnostic> {
    let output = Command::new("llvm-rc").arg("/?").output().map_err(|_| {
        Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "llvm-rc is not available on this build host",
            "Install the LLVM resource compiler. Sash does not download it.",
        )
    })?;
    let text = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !text.contains("LLVM Resource Converter") {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "llvm-rc did not identify itself as the LLVM resource converter",
            "Install llvm-rc on the build host.",
        ));
    }
    let clang = Command::new("clang").arg("--version").output();
    let clang_line = clang
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(|line| line.trim().to_string())
        })
        .filter(|line| !line.is_empty());
    Ok(match clang_line {
        Some(line) => format!("llvm-rc: LLVM Resource Converter; host {line}"),
        None => "llvm-rc: LLVM Resource Converter (the tool has no version flag)".into(),
    })
}

fn version(program: &str, args: &[&str]) -> Result<String, Diagnostic> {
    let output = Command::new(program).args(args).output().map_err(|_| {
        Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            format!("{program} is not available on this build host"),
            "Install the named tool. Sash does not download a compiler of its own.",
        )
    })?;
    if !output.status.success() && program != "llvm-rc" {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            format!("{program} failed while reporting its version"),
            "Repair the toolchain and retry.",
        ));
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stderr).into_owned();
    }
    Ok(text)
}

fn write_project(
    dir: &Path,
    generated: &Generated,
    target: WindowsTarget,
) -> Result<(), Diagnostic> {
    fs::create_dir(dir.join("src")).map_err(|error| io_diag("emit", &error))?;
    fs::create_dir(dir.join(".cargo")).map_err(|error| io_diag("emit", &error))?;
    write_new(&dir.join("Cargo.toml"), generated.cargo_toml.as_bytes())?;
    let config = crate::windows_target::cargo_config(target);
    write_new(&dir.join(".cargo/config.toml"), config.as_bytes())?;
    write_new(&dir.join("build.rs"), generated.build_rs.as_bytes())?;
    write_new(&dir.join("app.manifest"), generated.manifest_xml.as_bytes())?;
    write_new(&dir.join("app.rc"), generated.resource_script.as_bytes())?;
    write_new(&dir.join("src/main.rs"), generated.main_rs.as_bytes())?;
    write_linker_shim(dir)?;
    Ok(())
}

/// `lld-link` shim. rustc writes `symbols.o` in a random directory. That path
/// is the only PDB difference left after the build directory itself is stable,
/// and `/Brepro` then copies it into the CodeView GUID. The shim gives lld one
/// stable path. It is not a Windows launch.
fn write_linker_shim(dir: &Path) -> Result<(), Diagnostic> {
    let shim_dir = dir.join("shim");
    fs::create_dir(&shim_dir).map_err(|error| io_diag("emit", &error))?;
    let path = shim_dir.join("lld-link");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(&path)
        .map_err(|error| io_diag("emit", &error))?;
    file.write_all(LINKER_SHIM.as_bytes())
        .map_err(|error| io_diag("emit", &error))?;
    file.set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .map_err(|error| io_diag("emit", &error))?;
    Ok(())
}

const LINKER_SHIM: &str = r#"#!/usr/bin/env python3
import os, shutil, sys

real = os.environ["SASH_LLD_LINK"]
deps = os.environ["SASH_DEPS_DIR"]
stable = os.path.join(deps, "symbols.o")

def rewrite(arg):
    norm = arg.replace("\\", "/")
    if norm.endswith("/symbols.o") and "/rustc" in norm:
        os.makedirs(deps, exist_ok=True)
        shutil.copyfile(arg, stable)
        return stable
    if arg.startswith("@") and os.path.isfile(arg[1:]):
        return "@" + rewrite_response(arg[1:])
    return arg

def rewrite_response(path):
    data = open(path, "rb").read()
    utf16 = data.startswith(b"\xff\xfe")
    text = data[2:].decode("utf-16-le") if utf16 else data.decode()
    lines = []
    for line in text.splitlines():
        quoted = len(line) >= 2 and line[0] == '"' and line[-1] == '"'
        inner = line[1:-1].replace('\\"', '"') if quoted else line
        new = rewrite(inner)
        if quoted:
            lines.append('"' + new.replace('"', '\\"') + '"')
        else:
            lines.append(new)
    out = "\n".join(lines) + "\n"
    dest = os.path.join(deps, "linker-arguments")
    os.makedirs(deps, exist_ok=True)
    blob = b"\xff\xfe" + out.encode("utf-16-le") if utf16 else out.encode()
    with open(dest, "wb") as handle:
        handle.write(blob)
    return dest

args = [rewrite(arg) for arg in sys.argv[1:]]
os.execv(real, [real, *args])
"#;

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_diag("emit", &error))?;
    file.write_all(bytes)
        .map_err(|error| io_diag("emit", &error))?;
    // Source mtimes are not part of the scene. A fixed stamp keeps them out
    // of any debug record that stores file times.
    file.set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .map_err(|error| io_diag("emit", &error))?;
    Ok(())
}

fn pinned_windows_cache(target: WindowsTarget) -> Result<PathBuf, Diagnostic> {
    let Some(home) = std::env::var_os("HOME") else {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "HOME is not set, so the pinned Windows sysroot cannot be located",
            "Set HOME to the account that holds the local windows-msvc-sysroot.",
        ));
    };
    let cache = PathBuf::from(home).join(".cache/xwin");
    require_pinned_sysroot(&cache, target)?;
    let host_sysroot = version("rustc", &["--print", "sysroot"])?;
    let rust_lld =
        Path::new(host_sysroot.trim()).join("lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld");
    if !rust_lld.is_file() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "rust-lld is not installed beside the host rustc",
            "Install the rustup llvm-tools component that provides rust-lld. Sash does not fetch a linker.",
        ));
    }
    Ok(cache)
}

/// The selected machine needs its own import libraries. An x64 directory
/// does not satisfy i686 or ARM64. Sash does not download the missing tree.
fn require_pinned_sysroot(cache: &Path, target: WindowsTarget) -> Result<(), Diagnostic> {
    let marker = cache.join("windows-msvc-sysroot/DONE");
    let inner = cache.join("windows-msvc-sysroot/windows-msvc-sysroot");
    let lib = inner.join("lib").join(target.sysroot_lib);
    if !marker.is_file() || !inner.join("include").is_dir() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "the pinned windows-msvc-sysroot is not present",
            "Sash does not download a Windows SDK. Restore the local sysroot marked DONE before linking.",
        ));
    }
    if !lib.is_dir() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            format!(
                "the pinned windows-msvc-sysroot has no libraries for {}",
                target.triple
            ),
            "Restore that architecture in the local sysroot. Sash does not download it.",
        ));
    }
    Ok(())
}

fn active_rustup_toolchain() -> Option<String> {
    let output = Command::new("rustup")
        .args(["show", "active-toolchain"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let name = text.split_whitespace().next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn lld_link_binary(cache: &Path) -> Result<PathBuf, Diagnostic> {
    let cached = cache.join("lld-link");
    if cached.is_file() {
        return Ok(cached);
    }
    Err(Diagnostic::new(
        "SASH-TOOLCHAIN",
        "build",
        "the pinned xwin cache has no lld-link",
        "Restore the local windows-msvc-sysroot cache. Sash does not download a linker.",
    ))
}

fn run_link(project: &Path, cache: &Path, target: WindowsTarget) -> Result<String, Diagnostic> {
    // clang uses the existing splat. clang-cl looks for a separate CRT unpack
    // and would download the SDK when that unpack is absent.
    let lld = lld_link_binary(cache)?;
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![project.join("shim")];
    paths.extend(std::env::split_paths(&inherited));
    let path_var = std::env::join_paths(paths).map_err(|_| {
        Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "PATH could not include the linker shim",
            "Remove a NUL from PATH and retry.",
        )
    })?;
    let mut command = Command::new("cargo");
    command
        .arg("xwin")
        .arg("build")
        .arg("--cross-compiler")
        .arg("clang")
        .arg("--release")
        .arg("--offline")
        .arg("--target")
        .arg(target.triple)
        .arg("--manifest-path")
        .arg(project.join("Cargo.toml"))
        .current_dir(project)
        .env("CARGO_TARGET_DIR", project.join("target"))
        .env("CARGO_INCREMENTAL", "0")
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_NET_OFFLINE", "true")
        .env("XWIN_CACHE_DIR", cache)
        .env("XWIN_CROSS_COMPILER", "clang")
        .env("PATH", path_var)
        .env("SASH_LLD_LINK", &lld)
        .env(
            "SASH_DEPS_DIR",
            project
                .join("target")
                .join(target.triple)
                .join("release")
                .join("deps"),
        )
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    // The generated crate is outside this repo, so rustup would otherwise
    // ignore rust-toolchain.toml and compile with the default toolchain.
    // That toolchain does not have the standard library probe_tools checked.
    if let Some(toolchain) = active_rustup_toolchain() {
        command.env("RUSTUP_TOOLCHAIN", toolchain);
    }
    let output = command.output().map_err(|_| {
        Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "cargo xwin could not be started",
            "Install cargo-xwin on the build host.",
        )
    })?;
    let log = tail(&output.stdout, &output.stderr);
    if !output.status.success() {
        let code = classify_link_failure(&log);
        let remediation = if code == "SASH-TOOLCHAIN" {
            "The Windows toolchain or SDK on this host is incomplete. Sash did not download a replacement and did not promote App.exe."
        } else {
            "Read the native tool diagnostic. Sash did not promote an executable."
        };
        return Err(Diagnostic::new(
            code,
            "build",
            format!("Windows link failed\n{log}"),
            remediation,
        ));
    }
    Ok(log)
}

fn promote(
    output: &Path,
    exe: &[u8],
    pdb: &[u8],
    generated: &Generated,
    scene: &SceneEvidence,
    facts: &BuildFacts,
) -> Result<(), Diagnostic> {
    fs::create_dir(output).map_err(|error| io_diag("build", &error))?;
    let result = (|| {
        write_new(&output.join("App.exe"), exe)?;
        write_new(&output.join("App.pdb"), pdb)?;
        for (name, bytes) in report::files(scene, generated, facts) {
            let path = output.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| io_diag("build", &error))?;
            }
            write_new(&path, &bytes)?;
        }
        let promoted =
            fs::read(output.join("App.exe")).map_err(|error| io_diag("build", &error))?;
        if crate::sha256_hex(&promoted) != facts.exe_sha256 {
            return Err(Diagnostic::new(
                "SASH-VERIFY",
                "qualify",
                "the promoted App.exe does not match the assessed bytes",
                "Discard the output and link again.",
            ));
        }
        let file = File::open(output.join("App.exe")).map_err(|error| io_diag("build", &error))?;
        file.sync_all().map_err(|error| io_diag("build", &error))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(output);
    }
    result
}

fn link_error(log: &str, message: &str) -> Diagnostic {
    Diagnostic::new(
        "SASH-LINK",
        "build",
        format!("{message}\n{log}"),
        "Inspect the linker log. No App.exe was promoted.",
    )
}

fn tail(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(stderr).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(stdout).into_owned();
    }
    text = text.replace('\0', "");
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.len() > 60 {
        lines.drain(0..lines.len() - 60);
    }
    let joined = lines.join("\n");
    if joined.len() > 6_000 {
        joined[joined.len() - 6_000..].to_string()
    } else {
        joined
    }
}

fn free_enough(output: &Path) -> bool {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let output = Command::new("df").args(["-B1", "-P"]).arg(parent).output();
    let Ok(output) = output else {
        return true;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(line) = text.lines().nth(1) else {
        return true;
    };
    let Some(available) = line.split_whitespace().nth(3) else {
        return true;
    };
    available
        .parse::<u64>()
        .map(|bytes| bytes >= MIN_FREE_BYTES)
        .unwrap_or(true)
}

fn io_diag(stage: &'static str, error: &io::Error) -> Diagnostic {
    Diagnostic::new(
        "SASH-LINK",
        stage,
        format!("file operation failed: {error}"),
        "Choose a writable output directory and retry.",
    )
}

/// First 16 hex characters of a SHA-256 over the target and the crate bytes.
///
/// The same scene, generator, and triple always produce the same build
/// directory. That is what lets `/Brepro` see the same PDB paths on a second
/// link. It is not a Windows launch.
fn build_directory_key(target: &str, parts: &[&[u8]]) -> String {
    let mut bytes = Vec::new();
    bytes.extend(target.as_bytes());
    bytes.push(0xff);
    for part in parts {
        bytes.extend((part.len() as u64).to_le_bytes());
        bytes.extend(*part);
    }
    crate::sha256_hex(&bytes)[..16].to_string()
}

struct TempDir {
    path: PathBuf,
    lock: PathBuf,
}

impl TempDir {
    fn new(key: &str) -> Result<Self, Diagnostic> {
        let root = std::env::temp_dir();
        let path = root.join(format!("sash-build-{key}"));
        let lock = root.join(format!("sash-build-{key}.lock"));
        acquire_build_lock(&lock)?;
        if path.exists() {
            if let Err(error) = fs::remove_dir_all(&path) {
                let _ = fs::remove_file(&lock);
                return Err(io_diag("build", &error));
            }
        }
        if let Err(error) = fs::create_dir(&path) {
            let _ = fs::remove_file(&lock);
            return Err(io_diag("build", &error));
        }
        Ok(Self { path, lock })
    }
}

fn acquire_build_lock(lock: &Path) -> Result<(), Diagnostic> {
    for _ in 0..4 {
        match OpenOptions::new().write(true).create_new(true).open(lock) {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())
                    .map_err(|error| io_diag("build", &error))?;
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if lock_holder_dead(lock) && fs::remove_file(lock).is_ok() {
                    continue;
                }
                return Err(Diagnostic::new(
                    "SASH-LINK",
                    "build",
                    format!("another sash link holds {}", lock.display()),
                    "Wait for that link to finish, or remove the lock file if that process has exited.",
                ));
            }
            Err(error) => return Err(io_diag("build", &error)),
        }
    }
    Err(Diagnostic::new(
        "SASH-LINK",
        "build",
        format!("could not claim {}", lock.display()),
        "Remove the lock file if no sash link is running and retry.",
    ))
}

fn lock_holder_dead(lock: &Path) -> bool {
    let Ok(text) = fs::read_to_string(lock) else {
        return false;
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    !Path::new(&format!("/proc/{pid}")).exists()
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
        let _ = fs::remove_file(&self.lock);
    }
}

pub fn profile_supported(profile: &str) -> bool {
    profile == PROFILE_PORTABLE
}

/// Map a native compiler or linker log to a stable diagnostic code.
///
/// SDK setup and disk exhaustion are toolchain blocks. A rustc, resource, or
/// linker error is a link failure. Neither path promotes an executable.
pub fn classify_link_failure(log: &str) -> &'static str {
    let lower = log.to_ascii_lowercase();
    let sdk = lower.contains("unable to find")
        && (lower.contains("header") || lower.contains("crt") || lower.contains("sdk"));
    if lower.contains("no space left")
        || lower.contains("failed to setup msvc")
        || lower.contains("failed to download")
        || lower.contains("failed to unpack msvc")
        || sdk
    {
        "SASH-TOOLCHAIN"
    } else {
        "SASH-LINK"
    }
}

#[cfg(test)]
mod tests {
    use super::{build_directory_key, classify_link_failure};

    #[test]
    fn build_directory_key_follows_the_crate_and_the_target() {
        let same = build_directory_key("x86_64-pc-windows-msvc", &[b"crate"]);
        assert_eq!(
            same,
            build_directory_key("x86_64-pc-windows-msvc", &[b"crate"])
        );
        assert_ne!(
            same,
            build_directory_key("i686-pc-windows-msvc", &[b"crate"])
        );
        assert_ne!(
            same,
            build_directory_key("x86_64-pc-windows-msvc", &[b"other"])
        );
        assert_eq!(same.len(), 16);
        assert!(same.chars().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn native_tool_failures_keep_their_codes() {
        let crt = "Error: Failed to setup MSVC CRT\nunable to find x86_64 headers for Win11SDK_10.0.26100";
        assert_eq!(classify_link_failure(crt), "SASH-TOOLCHAIN");
        assert_eq!(
            classify_link_failure("No space left on device"),
            "SASH-TOOLCHAIN"
        );
        assert_eq!(
            classify_link_failure("llvm-rc failed\napp.rc(3): error"),
            "SASH-LINK"
        );
        assert_eq!(
            classify_link_failure("lld-link: error: undefined symbol main"),
            "SASH-LINK"
        );
    }

    #[test]
    fn legacy_floor_amends_the_dpi_dispositions() {
        use crate::api_floor::ApiFloor;
        let source = include_str!("../fixtures/static-window.gscene");
        let window = crate::admit::admit(source).unwrap();
        let win10 = super::legacy_dispositions(window.dispositions.clone(), ApiFloor::Win10);
        assert_eq!(win10, window.dispositions);
        assert!(win10.iter().any(|item| {
            item.subject == "background"
                && item.detail.contains("recreated when that DPI changes")
        }));
        assert!(!win10.iter().any(|item| item.subject == "dpi"));

        let legacy = super::legacy_dispositions(window.dispositions, ApiFloor::Win8);
        let background = legacy
            .iter()
            .find(|item| item.subject == "background")
            .expect("background disposition");
        assert!(!background.detail.contains("recreated when that DPI changes"));
        assert!(background.detail.contains("system DPI aware"));
        let dpi = legacy
            .iter()
            .find(|item| item.subject == "dpi")
            .expect("legacy dpi disposition");
        assert_eq!(dpi.disposition, "equivalently-transformed");
        assert!(dpi.detail.contains("GetDeviceCaps"));
        assert!(dpi.detail.contains("not been executed on Windows"));
    }

    #[test]
    fn sysroot_libraries_follow_the_selected_triple() {
        let root = std::env::temp_dir().join(format!("sash-sysroot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let inner = root.join("windows-msvc-sysroot/windows-msvc-sysroot");
        std::fs::create_dir_all(inner.join("include")).unwrap();
        std::fs::create_dir_all(
            inner
                .join("lib")
                .join(crate::windows_target::X64.sysroot_lib),
        )
        .unwrap();
        std::fs::write(root.join("windows-msvc-sysroot/DONE"), b"done").unwrap();
        assert!(super::require_pinned_sysroot(&root, crate::windows_target::X64).is_ok());
        let missing = super::require_pinned_sysroot(&root, crate::windows_target::X86).unwrap_err();
        assert_eq!(missing.code, "SASH-TOOLCHAIN");
        assert!(missing.message.contains("i686-pc-windows-msvc"));
        let arm = super::require_pinned_sysroot(&root, crate::windows_target::ARM64).unwrap_err();
        assert!(arm.message.contains("aarch64-pc-windows-msvc"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
