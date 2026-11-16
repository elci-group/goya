//! Construction and process-local runtime for bounded, read-only file access.
//!
//! Reads and metadata queries use the caller-supplied path and the current process's
//! operating-system permissions. `std::fs::File::open` and `std::fs::metadata` follow
//! the platform's normal symlink behavior (metadata describes the followed target);
//! this capability does not establish a root directory, sandbox, or path isolation.
//! The runtime reads at most the declared maximum plus one byte, so an oversized file
//! or a file that grows beyond the limit while being read returns a size error. No
//! filesystem writes or silent backend fallback are provided.

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub const CONTRACT: &str = "goya.filesystem.read_file.v1";
pub const RECIPE: &str = "rust-std-bounded-file-read";
pub const RECIPE_VERSION: &str = "0.2.0";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadDeclaration {
    /// Positive maximum payload size. The reader consumes at most one byte more
    /// to detect an over-limit file.
    pub max_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadIr {
    max_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    CrossProcess,
    Browser,
    Node,
}

/// Caller-reported properties of the target that will compile and run the adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub standard_library: bool,
    pub file_read: bool,
    pub pointer_width_bits: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroLimit,
    UnsupportedTarget,
    MissingStandardLibrary,
    MissingFileRead,
    UnsupportedPointerWidth,
    LimitDoesNotFitTarget,
    LimitExceedsAllocationBound,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroLimit => "maximum file size must be positive",
            Self::UnsupportedTarget => "file read adapter supports only process-local Rust",
            Self::MissingStandardLibrary => "target requires the Rust standard library",
            Self::MissingFileRead => "target runtime requires file read access",
            Self::UnsupportedPointerWidth => "target pointer width must be 16, 32, or 64 bits",
            Self::LimitDoesNotFitTarget => {
                "maximum file size plus the oversize check byte must fit the target pointer width"
            }
            Self::LimitExceedsAllocationBound => {
                "maximum file size plus the check byte exceeds the target Vec isize limit"
            }
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Debug)]
pub enum FileReadError {
    InvalidLimit,
    LimitNotRepresentable,
    LimitExceedsAllocationBound,
    AllocationFailed,
    SizeLimitExceeded { max_bytes: u64 },
    Io(io::Error),
}

impl fmt::Display for FileReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimit => f.write_str("maximum file size must be positive"),
            Self::LimitNotRepresentable => {
                f.write_str("maximum file size cannot be represented by this process")
            }
            Self::LimitExceedsAllocationBound => {
                f.write_str("maximum file size plus the check byte exceeds Vec's isize limit")
            }
            Self::AllocationFailed => f.write_str("file buffer allocation failed"),
            Self::SizeLimitExceeded { max_bytes } => {
                write!(f, "file exceeds the maximum size of {max_bytes} bytes")
            }
            Self::Io(error) => write!(f, "file read failed: {error}"),
        }
    }
}

impl std::error::Error for FileReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileEntryType {
    RegularFile,
    Directory,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    /// Length reported by `std::fs::Metadata::len` for the followed target.
    pub length_bytes: u64,
    pub entry_type: FileEntryType,
}

#[derive(Debug)]
pub enum FileMetadataError {
    Io(io::Error),
}

impl fmt::Display for FileMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "file metadata query failed: {error}"),
        }
    }
}

impl std::error::Error for FileMetadataError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
        }
    }
}

/// Query length and file type for the caller path. Symlinks are followed using
/// `std::fs::metadata` and the current process's operating-system permissions.
pub fn file_metadata(path: impl AsRef<Path>) -> Result<FileMetadata, FileMetadataError> {
    let metadata = std::fs::metadata(path).map_err(FileMetadataError::Io)?;
    let file_type = metadata.file_type();
    let entry_type = if file_type.is_file() {
        FileEntryType::RegularFile
    } else if file_type.is_dir() {
        FileEntryType::Directory
    } else {
        FileEntryType::Other
    };
    Ok(FileMetadata {
        length_bytes: metadata.len(),
        entry_type,
    })
}

/// Read at most `max_bytes + 1` bytes, returning an explicit size error if the
/// extra byte exists. Opening and reading follow the current process's OS rules.
pub fn read_bounded_file(path: impl AsRef<Path>, max_bytes: u64) -> Result<Vec<u8>, FileReadError> {
    if max_bytes == 0 {
        return Err(FileReadError::InvalidLimit);
    }
    let read_limit_bytes = max_bytes
        .checked_add(1)
        .ok_or(FileReadError::LimitNotRepresentable)?;
    if read_limit_bytes > isize::MAX as u64 {
        return Err(FileReadError::LimitExceedsAllocationBound);
    }
    let read_limit =
        usize::try_from(read_limit_bytes).map_err(|_| FileReadError::LimitNotRepresentable)?;
    let mut bytes = Vec::new();
    let mut reader = File::open(path)
        .map_err(FileReadError::Io)?
        .take(read_limit as u64);
    let mut chunk = [0u8; 8192];
    loop {
        let count = reader.read(&mut chunk).map_err(FileReadError::Io)?;
        if count == 0 {
            break;
        }
        bytes
            .try_reserve(count)
            .map_err(|_| FileReadError::AllocationFailed)?;
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() as u64 > max_bytes {
            return Err(FileReadError::SizeLimitExceeded { max_bytes });
        }
    }
    Ok(bytes)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileReadArtifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl ReadDeclaration {
    pub fn normalize(self) -> Result<ReadIr, Diagnostic> {
        if self.max_bytes == 0 {
            return Err(Diagnostic::ZeroLimit);
        }
        Ok(ReadIr {
            max_bytes: self.max_bytes,
        })
    }
}

impl ReadIr {
    pub fn max_bytes(self) -> u64 {
        self.max_bytes
    }

    pub fn canonical(self) -> String {
        format!(
            "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\noperations=read_file_and_metadata\nmax_bytes={}\nlimit_check=read_at_most_max_plus_one\nmetadata=std_fs_followed_target_length_and_type\npath_source=caller\nsymlinks=os_default_follow\npermissions=current_process\nroot_isolation=none\n",
            self.max_bytes
        )
    }

    pub fn lower_checked(
        self,
        target: Target,
        host: HostProfile,
    ) -> Result<FileReadArtifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(host.pointer_width_bits, 16 | 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        let target_max = match host.pointer_width_bits {
            16 => u16::MAX as u64,
            32 => u32::MAX as u64,
            64 => u64::MAX,
            _ => unreachable!("pointer width validated above"),
        };
        if self
            .max_bytes
            .checked_add(1)
            .is_none_or(|limit| limit > target_max)
        {
            return Err(Diagnostic::LimitDoesNotFitTarget);
        }
        let target_isize_max = match host.pointer_width_bits {
            16 => i16::MAX as u64,
            32 => i32::MAX as u64,
            64 => i64::MAX as u64,
            _ => unreachable!("pointer width validated above"),
        };
        if self
            .max_bytes
            .checked_add(1)
            .is_none_or(|limit| limit > target_isize_max)
        {
            return Err(Diagnostic::LimitExceedsAllocationBound);
        }
        if !host.standard_library {
            return Err(Diagnostic::MissingStandardLibrary);
        }
        if !host.file_read {
            return Err(Diagnostic::MissingFileRead);
        }
        let source = format!(
            "// {CONTRACT}; recipe={RECIPE}@{RECIPE_VERSION}.\n// Uses caller paths, current OS permissions, and default symlink behavior; no root isolation.\npub const MAX_FILE_BYTES: u64 = {}u64;\npub fn read_file(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, crate::filesystem_caprichio::FileReadError> {{\n    crate::filesystem_caprichio::read_bounded_file(path, MAX_FILE_BYTES)\n}}\npub fn file_metadata(path: impl AsRef<std::path::Path>) -> Result<crate::filesystem_caprichio::FileMetadata, crate::filesystem_caprichio::FileMetadataError> {{\n    crate::filesystem_caprichio::file_metadata(path)\n}}\n",
            self.max_bytes
        );
        let canonical_ir = format!(
            "{}target_pointer_width_bits={}\n",
            self.canonical(),
            host.pointer_width_bits
        );
        Ok(FileReadArtifact {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir,
            target_pointer_width_bits: host.pointer_width_bits,
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    fn host64() -> HostProfile {
        HostProfile {
            standard_library: true,
            file_read: true,
            pointer_width_bits: 64,
        }
    }

    fn temp_path(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "goya-filesystem-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn validates_positive_limit_and_target_representability() {
        assert_eq!(
            ReadDeclaration { max_bytes: 0 }.normalize(),
            Err(Diagnostic::ZeroLimit)
        );
        let ir = ReadDeclaration {
            max_bytes: u32::MAX as u64,
        }
        .normalize()
        .unwrap();
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    pointer_width_bits: 32,
                    ..host64()
                }
            ),
            Err(Diagnostic::LimitDoesNotFitTarget)
        );
        let accepted_32 = ReadDeclaration {
            max_bytes: i32::MAX as u64 - 1,
        }
        .normalize()
        .unwrap();
        assert!(accepted_32
            .lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    pointer_width_bits: 32,
                    ..host64()
                }
            )
            .is_ok());
        let rejected_32 = ReadDeclaration {
            max_bytes: i32::MAX as u64,
        }
        .normalize()
        .unwrap();
        assert_eq!(
            rejected_32.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    pointer_width_bits: 32,
                    ..host64()
                }
            ),
            Err(Diagnostic::LimitExceedsAllocationBound)
        );
        assert!(ir.lower_checked(Target::ProcessLocalRust, host64()).is_ok());
    }

    #[test]
    fn runtime_rejects_vec_isize_boundary_before_opening_the_path() {
        assert!(matches!(
            read_bounded_file("path-does-not-need-to-exist", isize::MAX as u64),
            Err(FileReadError::LimitExceedsAllocationBound)
        ));
    }

    #[test]
    fn canonical_ir_and_lowering_are_deterministic_and_fail_closed() {
        let ir = ReadDeclaration { max_bytes: 4096 }.normalize().unwrap();
        assert_eq!(ir.canonical(), ir.canonical());
        let first = ir
            .lower_checked(Target::ProcessLocalRust, host64())
            .unwrap();
        assert_eq!(
            first,
            ir.lower_checked(Target::ProcessLocalRust, host64())
                .unwrap()
        );
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("max_bytes=4096\n"));
        assert!(first
            .canonical_ir
            .contains("metadata=std_fs_followed_target_length_and_type\n"));
        assert!(first
            .source
            .contains("read_bounded_file(path, MAX_FILE_BYTES)"));
        assert!(first.source.contains("pub fn file_metadata"));
        assert!(first.source.contains("no root isolation"));
        for target in [Target::CrossProcess, Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, host64()),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    file_read: false,
                    ..host64()
                }
            ),
            Err(Diagnostic::MissingFileRead)
        );
    }

    #[test]
    fn generated_adapter_compiles_and_reads_exact_limit_but_rejects_one_extra_byte() {
        use std::io::Write;

        let artifact = ReadDeclaration { max_bytes: 4 }
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, host64())
            .unwrap();
        let exact_file = temp_path("exact");
        let oversized_file = temp_path("oversized");
        std::fs::write(&exact_file, b"four").unwrap();
        std::fs::write(&oversized_file, b"fiver").unwrap();
        let runtime_path = format!("{}/src/filesystem_caprichio.rs", env!("CARGO_MANIFEST_DIR"));
        let harness = format!(
            "#[path = {runtime_path:?}] mod filesystem_caprichio;\nuse std::path::Path;\n{}\nfn main() {{\n    let args: Vec<String> = std::env::args().collect();\n    let exact = Path::new(&args[1]);\n    assert_eq!(read_file(exact).unwrap(), b\"four\");\n    let metadata = file_metadata(exact).unwrap();\n    assert_eq!(metadata.length_bytes, 4);\n    assert_eq!(metadata.entry_type, filesystem_caprichio::FileEntryType::RegularFile);\n    assert!(matches!(read_file(Path::new(&args[2])), Err(filesystem_caprichio::FileReadError::SizeLimitExceeded {{ max_bytes: 4 }})));\n    assert!(matches!(file_metadata(Path::new(&args[3])), Err(filesystem_caprichio::FileMetadataError::Io(_))));\n}}\n",
            artifact.source
        );
        let executable = temp_path("adapter");
        let mut compiler = Command::new("rustc")
            .args([
                "--edition=2021",
                "--crate-name",
                "generated_filesystem_adapter",
                "-o",
            ])
            .arg(&executable)
            .arg("-")
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to test generated source");
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let compile = compiler.wait_with_output().unwrap();
        assert!(
            compile.status.success(),
            "generated adapter did not compile: {}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&executable)
            .arg(&exact_file)
            .arg(&oversized_file)
            .arg(temp_path("missing"))
            .status()
            .unwrap();
        let _ = std::fs::remove_file(executable);
        let _ = std::fs::remove_file(exact_file);
        let _ = std::fs::remove_file(oversized_file);
        assert!(
            run.success(),
            "generated adapter did not enforce the byte limit"
        );
    }

    #[test]
    #[ignore = "manual source construction benchmark"]
    fn benchmark_adapter_construction() {
        let ir = ReadDeclaration {
            max_bytes: 1_048_576,
        }
        .normalize()
        .unwrap();
        let started = Instant::now();
        for _ in 0..100_000 {
            std::hint::black_box(
                ir.lower_checked(Target::ProcessLocalRust, host64())
                    .unwrap(),
            );
        }
        eprintln!(
            "100,000 bounded file adapter constructions: {:?}",
            started.elapsed()
        );
    }

    #[test]
    #[ignore = "manual bounded file read benchmark"]
    fn benchmark_bounded_file_reads() {
        let path = temp_path("benchmark");
        std::fs::write(&path, vec![0x5a; 16 * 1024]).unwrap();
        let started = Instant::now();
        for _ in 0..10_000 {
            std::hint::black_box(read_bounded_file(&path, 16 * 1024).unwrap());
        }
        eprintln!("10,000 16 KiB bounded file reads: {:?}", started.elapsed());
        let _ = std::fs::remove_file(path);
    }
}
