//! GY-TOV/1 — process adapter for the Tov compiler.
//!
//! The adapter calls Tov's supported CLI (`tov check PATH` and
//! `tov build PATH -o OUTPUT`) without a shell. This keeps Goya independent
//! of a particular Tov checkout or compiler installation path while
//! providing a stable Rust interface for checking and building Tov projects.
//! The Tov compiler is an external tool and must be installed separately.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

/// Adapter contract implemented by this crate.
pub const TOV_ADAPTER_CONTRACT: &str = "GY-TOV/1";

/// Tov operation issued by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TovOperation {
    /// Type-check source without producing an executable.
    Check,
    /// Compile source to a native executable.
    Build,
}

/// Successful result from the Tov CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TovReport {
    pub operation: TovOperation,
    /// Standard output, decoded lossily so arbitrary compiler bytes are safe to display.
    pub stdout: String,
    /// Standard error, decoded lossily so arbitrary compiler bytes are safe to display.
    pub stderr: String,
    /// Set by `build`; absent for `check`.
    pub artifact: Option<BuildArtifact>,
}

/// Metadata for an executable produced by `tov build`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildArtifact {
    pub path: PathBuf,
    pub bytes: u64,
}

/// Errors surfaced by the Tov adapter.
#[derive(Debug)]
pub enum TovError {
    /// Input does not exist or is neither a regular file nor a directory.
    InvalidInput(PathBuf),
    /// Build output is empty, aliases the input, or has no existing parent.
    InvalidOutput(PathBuf),
    /// The configured executable could not be found. Install Tov or provide
    /// its executable path with [`TovAdapter::with_executable`].
    ToolUnavailable(PathBuf),
    /// The process could not be started for a reason other than absence.
    Launch(std::io::Error),
    /// Tov returned a non-zero status. The diagnostic text is retained.
    Failed {
        operation: TovOperation,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    /// Tov reported success but did not create a non-empty executable.
    MissingArtifact(PathBuf),
}

impl fmt::Display for TovError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TovError::InvalidInput(path) => write!(
                f,
                "Tov input is not an existing file or directory: {}",
                path.display()
            ),
            TovError::InvalidOutput(path) => {
                write!(f, "invalid Tov build output path: {}", path.display())
            }
            TovError::ToolUnavailable(path) => write!(
                f,
                "Tov compiler '{}' is unavailable; install Tov or configure its executable path",
                path.display()
            ),
            TovError::Launch(error) => write!(f, "could not launch Tov compiler: {error}"),
            TovError::Failed {
                operation,
                status,
                stderr,
                ..
            } => write!(
                f,
                "Tov {operation:?} failed (status {status:?}): {}",
                stderr.trim()
            ),
            TovError::MissingArtifact(path) => write!(
                f,
                "Tov reported a successful build but produced no non-empty file at {}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for TovError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TovError::Launch(error) => Some(error),
            _ => None,
        }
    }
}

/// Safe adapter for the Tov compiler CLI.
///
/// The default executable is `tov`, resolved through `PATH`. Callers can
/// select a specific installation with [`TovAdapter::with_executable`].
#[derive(Debug, Clone)]
pub struct TovAdapter {
    executable: PathBuf,
}

impl Default for TovAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TovAdapter {
    /// Use `tov` from the current process `PATH`.
    pub fn new() -> Self {
        Self {
            executable: PathBuf::from("tov"),
        }
    }

    /// Use a specific Tov executable path.
    pub fn with_executable(path: impl Into<PathBuf>) -> Self {
        Self {
            executable: path.into(),
        }
    }

    /// Ask Tov to type-check a source file or project directory.
    pub fn check(&self, input: impl AsRef<Path>) -> Result<TovReport, TovError> {
        let input = validate_input(input.as_ref())?;
        let args = arguments(TovOperation::Check, &input, None);
        let output = self.execute(TovOperation::Check, &args)?;
        Ok(report(TovOperation::Check, output, None))
    }

    /// Build a source file or project directory to a native executable.
    pub fn build(
        &self,
        input: impl AsRef<Path>,
        output: impl AsRef<Path>,
    ) -> Result<TovReport, TovError> {
        let input = validate_input(input.as_ref())?;
        let output = output.as_ref();
        validate_output(&input, output)?;
        let args = arguments(TovOperation::Build, &input, Some(output));
        let process_output = self.execute(TovOperation::Build, &args)?;
        let metadata =
            fs::metadata(output).map_err(|_| TovError::MissingArtifact(output.into()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(TovError::MissingArtifact(output.into()));
        }
        let artifact = BuildArtifact {
            path: output.into(),
            bytes: metadata.len(),
        };
        Ok(report(TovOperation::Build, process_output, Some(artifact)))
    }

    fn execute(&self, operation: TovOperation, args: &[OsString]) -> Result<Output, TovError> {
        let output = Command::new(&self.executable)
            .args(args)
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    TovError::ToolUnavailable(self.executable.clone())
                } else {
                    TovError::Launch(error)
                }
            })?;
        if !output.status.success() {
            return Err(TovError::Failed {
                operation,
                status: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output)
    }
}

fn validate_input(input: &Path) -> Result<PathBuf, TovError> {
    match fs::metadata(input) {
        Ok(metadata) if metadata.is_file() || metadata.is_dir() => Ok(input.into()),
        _ => Err(TovError::InvalidInput(input.into())),
    }
}

fn validate_output(input: &Path, output: &Path) -> Result<(), TovError> {
    if output.as_os_str().is_empty() || paths_alias(input, output) {
        return Err(TovError::InvalidOutput(output.into()));
    }
    match output.parent() {
        // A relative path with no directory component writes into the current
        // directory. The filesystem root has no parent directory to create in.
        Some(parent) if parent.as_os_str().is_empty() => Ok(()),
        Some(parent) if parent.is_dir() => Ok(()),
        _ => Err(TovError::InvalidOutput(output.into())),
    }
}

/// True when both paths name the same file.
///
/// `.` and `..` are folded lexically, and existing symlinks are resolved.
/// This rejects `tov build app.tov -o ./app.tov` before the compiler runs.
fn paths_alias(input: &Path, output: &Path) -> bool {
    if input == output {
        return true;
    }
    if lexical_absolute(input) == lexical_absolute(output) {
        return true;
    }
    matches!(
        (fs::canonicalize(input), fs::canonicalize(output)),
        (Ok(input), Ok(output)) if input == output
    )
}

fn lexical_absolute(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push(component);
                }
            }
            other => normalized.push(other),
        }
    }
    normalized
}

fn arguments(operation: TovOperation, input: &Path, output: Option<&Path>) -> Vec<OsString> {
    let mut args = vec![
        OsString::from(match operation {
            TovOperation::Check => "check",
            TovOperation::Build => "build",
        }),
        input.as_os_str().to_owned(),
    ];
    if let Some(output) = output {
        args.push(OsString::from("-o"));
        args.push(output.as_os_str().to_owned());
    }
    args
}

fn report(operation: TovOperation, output: Output, artifact: Option<BuildArtifact>) -> TovReport {
    TovReport {
        operation,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        artifact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    struct FakeCli(PathBuf);

    #[cfg(unix)]
    impl FakeCli {
        fn new(body: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("goya-tov-test-{}-{stamp}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("tov");
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            Self(path)
        }
    }

    #[cfg(unix)]
    impl Drop for FakeCli {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn argument_vector_keeps_spaces_as_one_path_argument() {
        let input = Path::new("scene samples/main.tov");
        let output = Path::new("build output/app");
        let args = arguments(TovOperation::Build, input, Some(output));
        assert_eq!(args[0], "build");
        assert_eq!(args[1], input.as_os_str());
        assert_eq!(args[2], "-o");
        assert_eq!(args[3], output.as_os_str());
    }

    #[test]
    fn missing_compiler_is_reported_as_unavailable() {
        let adapter = TovAdapter::with_executable("/definitely/not/a/tov/compiler");
        let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let error = adapter.check(input).unwrap_err();
        assert!(matches!(error, TovError::ToolUnavailable(_)));
    }

    #[cfg(unix)]
    #[test]
    fn failed_check_preserves_status_and_diagnostics() {
        let fake = FakeCli::new("echo compiler-out; echo compiler-err >&2; exit 7");
        let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let adapter = TovAdapter::with_executable(&fake.0);
        match adapter.check(input).unwrap_err() {
            TovError::Failed {
                operation,
                status,
                stdout,
                stderr,
            } => {
                assert_eq!(operation, TovOperation::Check);
                assert_eq!(status, Some(7));
                assert!(stdout.contains("compiler-out"));
                assert!(stderr.contains("compiler-err"));
            }
            other => panic!("expected compiler failure, got {other:?}"),
        }
    }

    #[test]
    fn rejects_input_output_alias() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(matches!(
            validate_output(&manifest, &manifest),
            Err(TovError::InvalidOutput(_))
        ));
        let dotted = Path::new(env!("CARGO_MANIFEST_DIR")).join("./Cargo.toml");
        let parented = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/../Cargo.toml");
        assert!(matches!(
            validate_output(&manifest, &dotted),
            Err(TovError::InvalidOutput(_))
        ));
        assert!(matches!(
            validate_output(&manifest, &parented),
            Err(TovError::InvalidOutput(_))
        ));
        assert!(matches!(
            validate_output(&manifest, Path::new("")),
            Err(TovError::InvalidOutput(_))
        ));
        assert!(matches!(
            validate_output(&manifest, Path::new("/")),
            Err(TovError::InvalidOutput(_))
        ));
        let missing_parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("missing-parent/app");
        assert!(matches!(
            validate_output(&manifest, &missing_parent),
            Err(TovError::InvalidOutput(_))
        ));
    }

    #[test]
    fn rejects_missing_input_before_launch() {
        let adapter = TovAdapter::with_executable("/definitely/not/a/tov/compiler");
        let missing = Path::new(env!("CARGO_MANIFEST_DIR")).join("no-such-input.tov");
        let error = adapter.check(&missing).unwrap_err();
        assert!(matches!(error, TovError::InvalidInput(_)));
    }

    #[cfg(unix)]
    #[test]
    fn build_reports_artifact_and_rejects_aliases_and_empty_output() {
        use std::os::unix::fs::symlink;

        let fake = FakeCli::new("printf 'xyz' > \"$4\"\necho built");
        let dir = fake.0.parent().unwrap();
        let input = dir.join("app.tov");
        fs::write(&input, "source").unwrap();
        let link = dir.join("link.tov");
        symlink(&input, &link).unwrap();
        let adapter = TovAdapter::with_executable(&fake.0);

        let alias = adapter.build(&link, &input).unwrap_err();
        assert!(matches!(alias, TovError::InvalidOutput(_)));

        let output = dir.join("app");
        let report = adapter.build(&input, &output).unwrap();
        let artifact = report.artifact.expect("build artifact");
        assert_eq!(report.operation, TovOperation::Build);
        assert_eq!(artifact.path, output);
        assert_eq!(artifact.bytes, 3);
        assert!(report.stdout.contains("built"));

        let check = adapter.check(&input).unwrap();
        assert_eq!(check.operation, TovOperation::Check);
        assert!(check.artifact.is_none());

        let empty = FakeCli::new(": > \"$4\"");
        let empty_input = empty.0.parent().unwrap().join("app.tov");
        fs::write(&empty_input, "source").unwrap();
        let empty_output = empty.0.parent().unwrap().join("app");
        let missing = TovAdapter::with_executable(&empty.0)
            .build(&empty_input, &empty_output)
            .unwrap_err();
        assert!(matches!(missing, TovError::MissingArtifact(_)));
    }
}
