//! Link an admitted scene with the pinned host's Windows cross tools.
//!
//! The output directory is created only after the PE policy checks pass.
//! A failed build does not leave an App.exe behind.

use crate::admit::{self, SceneTarget};
use crate::emit::Generated;
use crate::interactive;
use crate::pe;
use crate::report::{self, BuildFacts, SceneEvidence};
use crate::{Diagnostic, PROFILE_PORTABLE, TARGET_MSVC_X64};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const MIN_FREE_BYTES: u64 = 1_200_000_000;

pub struct Linked {
    pub output: PathBuf,
    pub exe_sha256: String,
    pub imports: Vec<String>,
    pub presentation_profile: &'static str,
}

pub fn link(source: &str, output: &Path) -> Result<Linked, Diagnostic> {
    if !free_enough(output) {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "the build host does not have enough free disk for a Windows link",
            "Free space and retry. Sash does not delete unrelated build caches.",
        ));
    }
    let prepared = prepare(source)?;
    let tools = probe_tools()?;
    let cache = pinned_windows_cache()?;
    let temp = TempDir::new()?;
    write_project(&temp.0, &prepared.generated)?;
    let log = run_link(&temp.0, &cache)?;
    let exe = temp.0.join("target/x86_64-pc-windows-msvc/release/app.exe");
    let pdb = temp.0.join("target/x86_64-pc-windows-msvc/release/app.pdb");
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

fn prepare(source: &str) -> Result<Prepared, Diagnostic> {
    match admit::open(source)? {
        SceneTarget::Static(window) => {
            let evidence = SceneEvidence {
                presentation_profile: admit::PROFILE,
                semantic_id: window.semantic_id.clone(),
                dispositions: window.dispositions.clone(),
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
                generated: crate::emit::generate(&window),
                evidence,
                source_digest,
            })
        }
        SceneTarget::Interactive(app) => {
            let clipboard = app.profile == interactive::PROFILE_CLIPBOARD;
            let evidence = SceneEvidence {
                presentation_profile: app.profile,
                semantic_id: app.semantic_id.clone(),
                dispositions: app.dispositions.clone(),
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
                generated: interactive::generate(&app),
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

fn probe_tools() -> Result<Tools, Diagnostic> {
    let rustc = version("rustc", &["--version"])?;
    let sysroot = version("rustc", &["--print", "sysroot"])?;
    let target_lib = Path::new(sysroot.trim()).join("lib/rustlib/x86_64-pc-windows-msvc");
    if !target_lib.is_dir() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "the Rust standard library for x86_64-pc-windows-msvc is not installed",
            "Install that target with rustup before asking Sash to link.",
        ));
    }
    let cargo_xwin = version("cargo", &["xwin", "--version"])?;
    let llvm_rc = llvm_rc_identity()?;
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

fn write_project(dir: &Path, generated: &Generated) -> Result<(), Diagnostic> {
    fs::create_dir(dir.join("src")).map_err(|error| io_diag("emit", &error))?;
    fs::create_dir(dir.join(".cargo")).map_err(|error| io_diag("emit", &error))?;
    write_new(&dir.join("Cargo.toml"), generated.cargo_toml.as_bytes())?;
    write_new(
        &dir.join(".cargo/config.toml"),
        generated.config_toml.as_bytes(),
    )?;
    write_new(&dir.join("build.rs"), generated.build_rs.as_bytes())?;
    write_new(&dir.join("app.manifest"), generated.manifest_xml.as_bytes())?;
    write_new(&dir.join("app.rc"), generated.resource_script.as_bytes())?;
    write_new(&dir.join("src/main.rs"), generated.main_rs.as_bytes())?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Diagnostic> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_diag("emit", &error))?;
    file.write_all(bytes)
        .map_err(|error| io_diag("emit", &error))?;
    Ok(())
}

fn pinned_windows_cache() -> Result<PathBuf, Diagnostic> {
    let Some(home) = std::env::var_os("HOME") else {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "HOME is not set, so the pinned Windows sysroot cannot be located",
            "Set HOME to the account that holds the local windows-msvc-sysroot.",
        ));
    };
    let cache = PathBuf::from(home).join(".cache/xwin");
    let marker = cache.join("windows-msvc-sysroot/DONE");
    let inner = cache.join("windows-msvc-sysroot/windows-msvc-sysroot");
    let lib = inner.join("lib/x86_64-unknown-windows-msvc");
    if !marker.is_file() || !inner.join("include").is_dir() || !lib.is_dir() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "the pinned windows-msvc-sysroot is not present",
            "Sash does not download a Windows SDK. Restore the local sysroot marked DONE before linking.",
        ));
    }
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

fn run_link(project: &Path, cache: &Path) -> Result<String, Diagnostic> {
    // clang uses the existing splat. clang-cl looks for a separate CRT unpack
    // and would download the SDK when that unpack is absent.
    let mut command = Command::new("cargo");
    command
        .arg("xwin")
        .arg("build")
        .arg("--cross-compiler")
        .arg("clang")
        .arg("--release")
        .arg("--offline")
        .arg("--target")
        .arg(TARGET_MSVC_X64)
        .arg("--manifest-path")
        .arg(project.join("Cargo.toml"))
        .current_dir(project)
        .env("CARGO_TARGET_DIR", project.join("target"))
        .env("CARGO_INCREMENTAL", "0")
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_NET_OFFLINE", "true")
        .env("XWIN_CACHE_DIR", cache)
        .env("XWIN_CROSS_COMPILER", "clang")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
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

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self, Diagnostic> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("sash-build-{}-{nanos}", std::process::id()));
        fs::create_dir(&path).map_err(|error| io_diag("build", &error))?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
    use super::classify_link_failure;

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
}
