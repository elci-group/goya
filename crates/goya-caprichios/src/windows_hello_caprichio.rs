//! A bounded Windows Hello local-authentication capability, the Windows
//! counterpart to the Face ID caprichio.
//!
//! The negotiated artifact binds to this crate's process-local attempt
//! runtime. It does not operate a camera or infrared sensor, claim face-data
//! access, or prove that the Windows Hello authentication provider is linked
//! in a consumer crate. Hosts running on Windows hardware report their real
//! Windows Hello facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.biometry.windows_hello.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_ATTEMPTS: u8 = crate::apple_runtime::MAX_ATTEMPTS;
pub const MAX_TIMEOUT_MS: u32 = crate::apple_runtime::MAX_AUTH_TIMEOUT_MS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsHelloDeclaration {
    /// Attempt bound for one prompt, from one through `MAX_ATTEMPTS`.
    pub max_attempts: u8,
    /// Prompt timeout in milliseconds, from one through `MAX_TIMEOUT_MS`.
    pub timeout_ms: u32,
    /// Whether unmatched prompts may fall back to the Windows Hello PIN.
    pub allow_pin_fallback: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowsHelloIr {
    max_attempts: u8,
    timeout_ms: u32,
    allow_pin_fallback: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Windows desktop or laptop with Windows Hello enrolled.
    Windows,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    /// Device reports a depth or infrared camera suitable for face or
    /// fingerprint recognition.
    pub hello_sensor: bool,
    pub hello_provider: bool,
    /// Match verification is backed by TPM or Secure Enclave-class hardware.
    pub tpm_backed_match: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroAttempts,
    AttemptsTooLarge,
    ZeroTimeout,
    TimeoutTooLarge,
    UnsupportedTarget,
    MissingHelloSensor,
    MissingHelloProvider,
    MissingTpmBackedMatch,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroAttempts => "attempt bound must be positive",
            Self::AttemptsTooLarge => "attempt bound exceeds the supported bound",
            Self::ZeroTimeout => "prompt timeout must be positive",
            Self::TimeoutTooLarge => "prompt timeout exceeds the supported bound",
            Self::UnsupportedTarget => {
                "Windows Hello factory supports only Windows targets"
            }
            Self::MissingHelloSensor => {
                "target must report a depth or infrared Windows Hello sensor"
            }
            Self::MissingHelloProvider => {
                "target runtime must provide the Windows Hello authentication provider"
            }
            Self::MissingTpmBackedMatch => {
                "target must report TPM-backed match verification"
            }
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled
/// or that a sensor matched.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WindowsHelloArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl WindowsHelloDeclaration {
    pub fn normalize(self) -> Result<WindowsHelloIr, Diagnostic> {
        if self.max_attempts == 0 {
            return Err(Diagnostic::ZeroAttempts);
        }
        if self.max_attempts > MAX_ATTEMPTS {
            return Err(Diagnostic::AttemptsTooLarge);
        }
        if self.timeout_ms == 0 {
            return Err(Diagnostic::ZeroTimeout);
        }
        if self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(Diagnostic::TimeoutTooLarge);
        }
        Ok(WindowsHelloIr {
            max_attempts: self.max_attempts,
            timeout_ms: self.timeout_ms,
            allow_pin_fallback: self.allow_pin_fallback,
        })
    }
}

impl WindowsHelloIr {
    pub fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    pub fn allow_pin_fallback(&self) -> bool {
        self.allow_pin_fallback
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Windows => "windows",
            Target::CrossPlatform | Target::Browser | Target::Node => "unsupported",
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsensor=windows_hello\nmax_attempts={}\ntimeout_ms={}\nfallback={}\ndata_access=none\n",
            Self::target_label(target),
            self.max_attempts,
            self.timeout_ms,
            if self.allow_pin_fallback {
                "hello_pin"
            } else {
                "none"
            }
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<WindowsHelloArtifact, Diagnostic> {
        if target != Target::Windows {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.hello_sensor {
            return Err(Diagnostic::MissingHelloSensor);
        }
        if !capabilities.hello_provider {
            return Err(Diagnostic::MissingHelloProvider);
        }
        if !capabilities.tpm_backed_match {
            return Err(Diagnostic::MissingTpmBackedMatch);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        Ok(WindowsHelloArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; windows Windows Hello attempt gate, no sensor or biometric data is accessed.\npub fn make_windows_hello_gate() -> Result<crate::apple_runtime::BiometricGate, crate::apple_runtime::AuthError> {{\n    crate::apple_runtime::BiometricGate::new({}u8, {}u32)\n}}\n",
                self.max_attempts, self.timeout_ms
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOWS: RuntimeCapabilities = RuntimeCapabilities {
        hello_sensor: true,
        hello_provider: true,
        tpm_backed_match: true,
        utf8_strings: true,
    };

    fn declaration() -> WindowsHelloDeclaration {
        WindowsHelloDeclaration {
            max_attempts: 3,
            timeout_ms: 20_000,
            allow_pin_fallback: true,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.max_attempts(), 3);
        assert_eq!(ir.timeout_ms(), 20_000);
        assert!(ir.allow_pin_fallback());
        let first = ir.lower_checked(Target::Windows, WINDOWS).unwrap();
        let second = ir.lower_checked(Target::Windows, WINDOWS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=windows\n"));
        assert!(first.canonical_ir.contains("sensor=windows_hello\n"));
        assert!(first.canonical_ir.contains("max_attempts=3\n"));
        assert!(first.canonical_ir.contains("fallback=hello_pin\n"));
        assert!(first.canonical_ir.contains("data_access=none\n"));
        assert!(first.source.contains("BiometricGate::new(3u8, 20000u32)"));
    }

    #[test]
    fn rejects_bad_attempt_and_timeout_bounds() {
        assert_eq!(
            WindowsHelloDeclaration {
                max_attempts: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroAttempts)
        );
        assert_eq!(
            WindowsHelloDeclaration {
                max_attempts: MAX_ATTEMPTS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::AttemptsTooLarge)
        );
        assert_eq!(
            WindowsHelloDeclaration {
                timeout_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroTimeout)
        );
        assert_eq!(
            WindowsHelloDeclaration {
                timeout_ms: MAX_TIMEOUT_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::TimeoutTooLarge)
        );
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration().normalize().unwrap();
        for target in [
            Target::CrossPlatform,
            Target::Browser,
            Target::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, WINDOWS),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    hello_sensor: false,
                    ..WINDOWS
                },
                Diagnostic::MissingHelloSensor,
            ),
            (
                RuntimeCapabilities {
                    hello_provider: false,
                    ..WINDOWS
                },
                Diagnostic::MissingHelloProvider,
            ),
            (
                RuntimeCapabilities {
                    tpm_backed_match: false,
                    ..WINDOWS
                },
                Diagnostic::MissingTpmBackedMatch,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..WINDOWS
                },
                Diagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::Windows, missing), Err(expected));
        }
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_attempt_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::Windows, WINDOWS)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let mut gate = make_windows_hello_gate().expect(\"factory should construct gate\");\n    assert_eq!(gate.max_attempts(), 3);\n    assert_eq!(gate.timeout_ms(), 20000);\n    assert_eq!(gate.attempt(false), Ok(apple_runtime::AuthOutcome::Retry));\n    assert_eq!(gate.attempt(false), Err(apple_runtime::AuthError::AttemptsExhausted));\n    assert!(gate.is_resolved());\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-windows-hello-factory-{}-{nonce}",
            std::process::id()
        ));
        let mut rustc = Command::new("rustc")
            .args(["--edition=2021", "-o"])
            .arg(&output_path)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("rustc must be available to compile generated source");
        rustc
            .stdin
            .take()
            .expect("rustc stdin")
            .write_all(harness.as_bytes())
            .expect("write generated factory harness");
        let compile = rustc.wait_with_output().expect("wait for rustc");
        assert!(
            compile.status.success(),
            "generated factory failed to compile:\n{}",
            String::from_utf8_lossy(&compile.stderr)
        );
        let execution = Command::new(&output_path)
            .output()
            .expect("run generated Windows Hello factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
