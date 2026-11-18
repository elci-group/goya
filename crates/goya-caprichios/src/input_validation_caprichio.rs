//! Bounded process-local validation for ASCII text.
//!
//! This checks only byte length and membership in an explicit ASCII byte set.
//! It does not detect or prevent XSS, SQL injection, other injection classes,
//! or perform Unicode normalization.

use std::fmt;

pub const CONTRACT: &str = "goya.input.ascii_set_validation.process_local.v1";
pub const RECIPE: &str = "rust-std-bounded-ascii-set-validator";
pub const RECIPE_VERSION: &str = "0.1.0";
pub const MAX_TEXT_BYTES: u32 = 1_048_576;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AllowedAscii {
    Lowercase,
    Uppercase,
    Letters,
    Digits,
    Alphanumeric,
    Printable,
    Explicit(Vec<u8>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Declaration {
    pub min_bytes: u32,
    pub max_bytes: u32,
    pub allowed: AllowedAscii,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    ProcessLocalRust,
    Browser,
    Node,
    CrossProcess,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostProfile {
    pub pointer_width_bits: u8,
    pub standard_library: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    InvalidLengthRange,
    MaximumTooLarge,
    EmptyAllowedSet,
    NonAsciiAllowedByte,
    DuplicateAllowedByte,
    UnsupportedTarget,
    UnsupportedPointerWidth,
    MissingStandardLibrary,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLengthRange => "minimum byte length must not exceed maximum",
            Self::MaximumTooLarge => "maximum byte length exceeds the supported bound",
            Self::EmptyAllowedSet => "allowed ASCII set must contain at least one byte",
            Self::NonAsciiAllowedByte => "allowed set contains a non-ASCII byte",
            Self::DuplicateAllowedByte => "explicit allowed set contains a duplicate byte",
            Self::UnsupportedTarget => "ASCII validator supports only process-local Rust",
            Self::UnsupportedPointerWidth => "target pointer width must be 32 or 64 bits",
            Self::MissingStandardLibrary => "target requires the Rust standard library",
            Self::MissingUtf8Strings => "target requires UTF-8 string support",
        })
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Limits {
    pub min_bytes: usize,
    pub max_bytes: usize,
    pub allowed_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ir(Limits);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Artifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub target_pointer_width_bits: u8,
    pub source: String,
}

impl Declaration {
    pub fn normalize(self) -> Result<Ir, Diagnostic> {
        if self.min_bytes > self.max_bytes {
            return Err(Diagnostic::InvalidLengthRange);
        }
        if self.max_bytes > MAX_TEXT_BYTES {
            return Err(Diagnostic::MaximumTooLarge);
        }
        let mut bytes = match self.allowed {
            AllowedAscii::Lowercase => (b'a'..=b'z').collect(),
            AllowedAscii::Uppercase => (b'A'..=b'Z').collect(),
            AllowedAscii::Letters => (b'A'..=b'Z').chain(b'a'..=b'z').collect(),
            AllowedAscii::Digits => (b'0'..=b'9').collect(),
            AllowedAscii::Alphanumeric => (b'0'..=b'9')
                .chain(b'A'..=b'Z')
                .chain(b'a'..=b'z')
                .collect(),
            AllowedAscii::Printable => (0x20..=0x7e).collect(),
            AllowedAscii::Explicit(bytes) => {
                if bytes.iter().any(|byte| !byte.is_ascii()) {
                    return Err(Diagnostic::NonAsciiAllowedByte);
                }
                let mut seen = [false; 128];
                for byte in &bytes {
                    let slot = &mut seen[*byte as usize];
                    if *slot {
                        return Err(Diagnostic::DuplicateAllowedByte);
                    }
                    *slot = true;
                }
                bytes
            }
        };
        if bytes.is_empty() {
            return Err(Diagnostic::EmptyAllowedSet);
        }
        bytes.sort_unstable();
        Ok(Ir(Limits {
            min_bytes: self.min_bytes as usize,
            max_bytes: self.max_bytes as usize,
            allowed_bytes: bytes,
        }))
    }
}

impl Ir {
    pub fn limits(&self) -> &Limits {
        &self.0
    }

    pub fn canonical(&self) -> String {
        let allowed_hex = self
            .0
            .allowed_bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join("");
        format!(
            "contract={CONTRACT}\nrecipe={RECIPE}@{RECIPE_VERSION}\ntarget=process_local_rust\ninput=utf8_string_checked_as_ascii_bytes\nmin_bytes={}\nmax_bytes={}\nallowed_ascii_hex={}\nchecks=byte_length_then_ascii_and_set_membership\nsecurity_claims=none\nunicode_normalization=none\n",
            self.0.min_bytes, self.0.max_bytes, allowed_hex
        )
    }

    pub fn lower_checked(&self, target: Target, host: HostProfile) -> Result<Artifact, Diagnostic> {
        if target != Target::ProcessLocalRust {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !matches!(host.pointer_width_bits, 32 | 64) {
            return Err(Diagnostic::UnsupportedPointerWidth);
        }
        if !host.standard_library {
            return Err(Diagnostic::MissingStandardLibrary);
        }
        if !host.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        let canonical_ir = format!(
            "{}target_pointer_width_bits={}\n",
            self.canonical(),
            host.pointer_width_bits
        );
        let allowed = self
            .0
            .allowed_bytes
            .iter()
            .map(|byte| format!("{byte}u8"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "// {CONTRACT}; {RECIPE}@{RECIPE_VERSION}; ASCII set and length validation only.\nconst MIN_TEXT_BYTES: usize = {}usize;\nconst MAX_TEXT_BYTES: usize = {}usize;\nconst ALLOWED_ASCII_BYTES: &[u8] = &[{}];\npub fn validate_text(input: &str) -> Result<(), crate::input_validation_caprichio::ValidationError> {{\n    crate::input_validation_caprichio::validate(input, MIN_TEXT_BYTES, MAX_TEXT_BYTES, ALLOWED_ASCII_BYTES)\n}}\n",
            self.0.min_bytes, self.0.max_bytes, allowed
        );
        Ok(Artifact {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir,
            target_pointer_width_bits: host.pointer_width_bits,
            source,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ValidationError {
    InvalidConfiguration,
    TooShort { minimum: usize, actual: usize },
    TooLong { maximum: usize, actual: usize },
    NonAscii { index: usize },
    DisallowedByte { index: usize, byte: u8 },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration => f.write_str("validator configuration is invalid"),
            Self::TooShort { minimum, actual } => {
                write!(f, "input has {actual} bytes; minimum is {minimum}")
            }
            Self::TooLong { maximum, actual } => {
                write!(f, "input has {actual} bytes; maximum is {maximum}")
            }
            Self::NonAscii { index } => write!(f, "input contains non-ASCII text at byte {index}"),
            Self::DisallowedByte { index, byte } => {
                write!(
                    f,
                    "input byte 0x{byte:02x} at offset {index} is not allowed"
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate byte length first, then require every byte to be ASCII and in the
/// supplied set. `allowed_ascii` must be a nonempty set of distinct ASCII bytes.
pub fn validate(
    input: &str,
    min_bytes: usize,
    max_bytes: usize,
    allowed_ascii: &[u8],
) -> Result<(), ValidationError> {
    if min_bytes > max_bytes
        || max_bytes > MAX_TEXT_BYTES as usize
        || allowed_ascii.is_empty()
        || allowed_ascii.iter().any(|byte| !byte.is_ascii())
    {
        return Err(ValidationError::InvalidConfiguration);
    }
    let mut seen = [false; 128];
    for byte in allowed_ascii {
        let slot = &mut seen[*byte as usize];
        if *slot {
            return Err(ValidationError::InvalidConfiguration);
        }
        *slot = true;
    }
    if input.len() < min_bytes {
        return Err(ValidationError::TooShort {
            minimum: min_bytes,
            actual: input.len(),
        });
    }
    if input.len() > max_bytes {
        return Err(ValidationError::TooLong {
            maximum: max_bytes,
            actual: input.len(),
        });
    }
    for (index, byte) in input.bytes().enumerate() {
        if !byte.is_ascii() {
            return Err(ValidationError::NonAscii { index });
        }
        if !allowed_ascii.contains(&byte) {
            return Err(ValidationError::DisallowedByte { index, byte });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declaration() -> Declaration {
        Declaration {
            min_bytes: 1,
            max_bytes: 32,
            allowed: AllowedAscii::Alphanumeric,
        }
    }

    fn host() -> HostProfile {
        HostProfile {
            pointer_width_bits: usize::BITS as u8,
            standard_library: true,
            utf8_strings: true,
        }
    }

    #[test]
    fn canonical_ir_and_binding_are_deterministic_and_fail_closed() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir, declaration().normalize().unwrap());
        assert!(ir.canonical().contains("allowed_ascii_hex=303132333435363738394142434445464748494a4b4c4d4e4f505152535455565758595a6162636465666768696a6b6c6d6e6f707172737475767778797a\n"));
        let artifact = ir.lower_checked(Target::ProcessLocalRust, host()).unwrap();
        assert_eq!(
            artifact,
            ir.lower_checked(Target::ProcessLocalRust, host()).unwrap()
        );
        assert!(artifact.canonical_ir.contains("security_claims=none\n"));
        assert!(artifact
            .source
            .contains("input_validation_caprichio::validate"));
        assert_eq!(
            ir.lower_checked(Target::Browser, host()),
            Err(Diagnostic::UnsupportedTarget)
        );
        assert_eq!(
            ir.lower_checked(
                Target::ProcessLocalRust,
                HostProfile {
                    utf8_strings: false,
                    ..host()
                }
            ),
            Err(Diagnostic::MissingUtf8Strings)
        );
    }

    #[test]
    fn invalid_configuration_and_input_edges_are_precise() {
        assert_eq!(
            Declaration {
                min_bytes: 4,
                max_bytes: 3,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::InvalidLengthRange)
        );
        assert_eq!(
            Declaration {
                max_bytes: MAX_TEXT_BYTES + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::MaximumTooLarge)
        );
        assert_eq!(
            Declaration {
                allowed: AllowedAscii::Explicit(vec![]),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::EmptyAllowedSet)
        );
        assert_eq!(
            Declaration {
                allowed: AllowedAscii::Explicit(vec![b'a', b'a']),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::DuplicateAllowedByte)
        );
        assert_eq!(
            Declaration {
                allowed: AllowedAscii::Explicit(vec![0xff]),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::NonAsciiAllowedByte)
        );
        let ir = declaration().normalize().unwrap();
        let limits = ir.limits();
        assert_eq!(
            validate(
                "",
                limits.min_bytes,
                limits.max_bytes,
                &limits.allowed_bytes
            ),
            Err(ValidationError::TooShort {
                minimum: 1,
                actual: 0
            })
        );
        assert_eq!(
            validate("abc", 0, 2, b"abc"),
            Err(ValidationError::TooLong {
                maximum: 2,
                actual: 3
            })
        );
        assert_eq!(
            validate("é", 0, 4, b"e"),
            Err(ValidationError::NonAscii { index: 0 })
        );
        assert_eq!(
            validate("a-", 0, 4, b"abc"),
            Err(ValidationError::DisallowedByte {
                index: 1,
                byte: b'-'
            })
        );
        assert_eq!(
            validate("ab", 0, 1, b"abc"),
            Err(ValidationError::TooLong {
                maximum: 1,
                actual: 2
            })
        );
        assert_eq!(
            validate("a", 0, 1, &[]),
            Err(ValidationError::InvalidConfiguration)
        );
        assert_eq!(
            validate("a", 0, 1, b"aa"),
            Err(ValidationError::InvalidConfiguration)
        );
    }

    #[test]
    fn deterministic_generated_ascii_strings_obey_explicit_membership() {
        let ir = Declaration {
            min_bytes: 0,
            max_bytes: 128,
            allowed: AllowedAscii::Printable,
        }
        .normalize()
        .unwrap();
        let limits = ir.limits();
        for length in 0..=128 {
            let value: String = (0..length)
                .map(|index| char::from(0x20 + (index % 95) as u8))
                .collect();
            assert_eq!(
                validate(
                    &value,
                    limits.min_bytes,
                    limits.max_bytes,
                    &limits.allowed_bytes
                ),
                Ok(())
            );
        }
        assert!(matches!(
            validate("\u{7f}", 0, 4, &ir.limits().allowed_bytes),
            Err(ValidationError::DisallowedByte { byte: 0x7f, .. })
        ));
    }

    #[test]
    fn generated_binding_compiles_and_runs_against_real_validator() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};
        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::ProcessLocalRust, host())
            .unwrap();
        let module = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/input_validation_caprichio.rs");
        let harness = format!("#[path = {module:?}] mod input_validation_caprichio;\n{}\nfn main() {{ assert_eq!(validate_text(\"abc123\"), Ok(())); assert!(validate_text(\"bad-value\").is_err()); assert!(validate_text(\"é\").is_err()); }}\n", artifact.source);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let binary = std::env::temp_dir().join(format!(
            "goya-input-validator-{}-{nonce}",
            std::process::id()
        ));
        let mut compiler = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&binary)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler
            .stdin
            .take()
            .unwrap()
            .write_all(harness.as_bytes())
            .unwrap();
        let compile = compiler.wait_with_output().unwrap();
        assert!(
            compile.status.success(),
            "generated validator did not compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&binary).output().unwrap();
        let _ = std::fs::remove_file(binary);
        assert!(
            run.status.success(),
            "generated validator failed:\n{}",
            String::from_utf8_lossy(&run.stderr)
        );
    }

    #[test]
    #[ignore = "manual ASCII validation benchmark"]
    fn benchmark_ascii_validator() {
        use std::time::Instant;
        let ir = declaration().normalize().unwrap();
        let limits = ir.limits();
        let input = "safeInput123";
        let start = Instant::now();
        for _ in 0..1_000_000 {
            validate(
                std::hint::black_box(input),
                limits.min_bytes,
                limits.max_bytes,
                &limits.allowed_bytes,
            )
            .unwrap();
        }
        eprintln!("1,000,000 bounded ASCII checks in {:?}", start.elapsed());
    }
}
