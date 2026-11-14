//! Link an admitted scene with the pinned host's Windows cross tools.
//!
//! The output directory is created only after the PE policy checks pass.
//! A failed build does not leave an App.exe behind.

use crate::admit::{self, AdmittedWindow};
use crate::emit::{self, Generated};
use crate::pe::{self, PeInfo};
use crate::report::{self, BuildFacts};
use crate::{Diagnostic, TARGET_MSVC_X64};
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
    let window = admit::admit(source)?;
    let generated = emit::generate(&window);
    let tools = probe_tools()?;
    let temp = TempDir::new()?;
    write_project(&temp.0, &generated)?;
    let log = run_link(&temp.0)?;
    let exe = temp
        .0
        .join("target/x86_64-pc-windows-msvc/release/app.exe");
    let pdb = temp
        .0
        .join("target/x86_64-pc-windows-msvc/release/app.pdb");
    if !exe.is_file() {
        return Err(link_error(&log, "the linker did not produce app.exe"));
    }
    if !pdb.is_file() {
        return Err(link_error(&log, "the linker did not produce private symbols"));
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
        manifest_sha256: crate::sha256_hex(generated.manifest_xml.as_bytes()),
        generated_sha256: crate::sha256_hex(generated.main_rs.as_bytes()),
        source_digest: crate::sha256_hex(&window.source_digest),
        pe: info.clone(),
        rustc: tools.rustc,
        cargo_xwin: tools.cargo_xwin,
        llvm_rc: tools.llvm_rc,
    };
    promote(output, &exe_bytes, &pdb_bytes, &window, &generated, &facts)?;
    Ok(Linked {
        output: output.to_path_buf(),
        exe_sha256: facts.exe_sha256,
        imports: info.imports,
    })
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
    let llvm_rc = version("llvm-rc", &["--version"])?;
    if !llvm_rc.to_ascii_lowercase().contains("llvm") && llvm_rc.trim().is_empty() {
        return Err(Diagnostic::new(
            "SASH-TOOLCHAIN",
            "build",
            "llvm-rc did not identify itself",
            "Install the LLVM resource compiler on the build host.",
        ));
    }
    Ok(Tools {
        rustc: rustc.trim().to_string(),
        cargo_xwin: cargo_xwin.trim().to_string(),
        llvm_rc: llvm_rc.trim().to_string(),
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
    write_new(&dir.join(".cargo/config.toml"), generated.config_toml.as_bytes())?;
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
    file.write_all(bytes).map_err(|error| io_diag("emit", &error))?;
    Ok(())
}

fn run_link(project: &Path) -> Result<String, Diagnostic> {
    let mut command = Command::new("cargo");
    command
        .arg("xwin")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg(TARGET_MSVC_X64)
        .arg("--manifest-path")
        .arg(project.join("Cargo.toml"))
        .current_dir(project)
        .env("CARGO_TARGET_DIR", project.join("target"))
        .env("CARGO_INCREMENTAL", "0")
        .env("CARGO_BUILD_JOBS", "1")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    if std::env::var_os("XWIN_CACHE_DIR").is_none() {
        if let Some(home) = std::env::var_os("HOME") {
            let cache = PathBuf::from(&home).join(".cache/xwin");
            if cache.join("windows-msvc-sysroot").is_dir() {
                command.env("XWIN_CACHE_DIR", cache);
            }
        }
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
        let code = if log.contains("No space left") {
            "SASH-TOOLCHAIN"
        } else {
            "SASH-LINK"
        };
        return Err(Diagnostic::new(
            code,
            "build",
            format!("Windows link failed\n{log}"),
            "Read the toolchain diagnostic. Sash did not promote an executable.",
        ));
    }
    Ok(log)
}

fn promote(
    output: &Path,
    exe: &[u8],
    pdb: &[u8],
    window: &AdmittedWindow,
    generated: &Generated,
    facts: &BuildFacts,
) -> Result<(), Diagnostic> {
    fs::create_dir(output).map_err(|error| io_diag("build", &error))?;
    let result = (|| {
        write_new(&output.join("App.exe"), exe)?;
        write_new(&output.join("App.pdb"), pdb)?;
        for (name, bytes) in report::files(window, generated, facts) {
            let path = output.join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| io_diag("build", &error))?;
            }
            write_new(&path, &bytes)?;
        }
        let promoted = fs::read(output.join("App.exe")).map_err(|error| io_diag("build", &error))?;
        if crate::sha256_hex(&promoted) != facts.exe_sha256 {
            return Err(Diagnostic::new(
                "SASH-VERIFY",
                "qualify",
                "the promoted App.exe does not match the assessed bytes",
                "Discard the output and link again.",
            ));
        }
        let mut file = File::open(output.join("App.exe")).map_err(|error| io_diag("build", &error))?;
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
    let parent = output.parent().filter(|path| !path.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
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
