//! A bounded Face ID local-authentication capability for iOS hosts.
//!
//! The negotiated artifact binds to this crate's process-local attempt
//! runtime. It does not operate a camera, claim face-data access, or prove
//! that LocalAuthentication is linked in a consumer crate. Hosts on Apple
//! hardware report their real Face ID facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.biometry.face_id.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_ATTEMPTS: u8 = crate::apple_runtime::MAX_ATTEMPTS;
pub const MAX_TIMEOUT_MS: u32 = crate::apple_runtime::MAX_AUTH_TIMEOUT_MS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaceIdDeclaration {
    /// Attempt bound for one prompt, from one through `MAX_ATTEMPTS`.
    pub max_attempts: u8,
    /// Prompt timeout in milliseconds, from one through `MAX_TIMEOUT_MS`.
    pub timeout_ms: u32,
    /// Whether the prompt requires user attention (eyes on the device).
    pub require_attention: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FaceIdIr {
    max_attempts: u8,
    timeout_ms: u32,
    require_attention: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// iOS device with a Face ID sensor.
    Ios,
    /// Face ID hardware is not part of any Mac configuration.
    AppleSiliconMac,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub face_id_sensor: bool,
    pub camera_available: bool,
    /// LocalAuthentication context is available on the target.
    pub local_authentication: bool,
    pub secure_enclave_match: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    ZeroAttempts,
    AttemptsTooLarge,
    ZeroTimeout,
    TimeoutTooLarge,
    UnsupportedTarget,
    MissingFaceIdSensor,
    MissingCamera,
    MissingLocalAuthentication,
    MissingSecureEnclaveMatch,
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
                "Face ID factory supports only iOS devices with a Face ID sensor"
            }
            Self::MissingFaceIdSensor => "target must report a Face ID sensor",
            Self::MissingCamera => "target must report an available TrueDepth camera",
            Self::MissingLocalAuthentication => {
                "target runtime must provide a LocalAuthentication context"
            }
            Self::MissingSecureEnclaveMatch => {
                "target must report Secure Enclave match verification"
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
pub struct FaceIdArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl FaceIdDeclaration {
    pub fn normalize(self) -> Result<FaceIdIr, Diagnostic> {
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
        Ok(FaceIdIr {
            max_attempts: self.max_attempts,
            timeout_ms: self.timeout_ms,
            require_attention: self.require_attention,
        })
    }
}

impl FaceIdIr {
    pub fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    pub fn timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    pub fn require_attention(&self) -> bool {
        self.require_attention
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Ios => "ios",
            Target::AppleSiliconMac | Target::CrossPlatform | Target::Browser | Target::Node => {
                "unsupported"
            }
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nsensor=face_id\nmax_attempts={}\ntimeout_ms={}\nattention={}\ndata_access=none\n",
            Self::target_label(target),
            self.max_attempts,
            self.timeout_ms,
            if self.require_attention {
                "required"
            } else {
                "not_required"
            }
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<FaceIdArtifact, Diagnostic> {
        if target != Target::Ios {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.face_id_sensor {
            return Err(Diagnostic::MissingFaceIdSensor);
        }
        if !capabilities.camera_available {
            return Err(Diagnostic::MissingCamera);
        }
        if !capabilities.local_authentication {
            return Err(Diagnostic::MissingLocalAuthentication);
        }
        if !capabilities.secure_enclave_match {
            return Err(Diagnostic::MissingSecureEnclaveMatch);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        Ok(FaceIdArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; iOS Face ID attempt gate, no camera or face data is accessed.\npub fn make_face_id_gate() -> Result<crate::apple_runtime::BiometricGate, crate::apple_runtime::AuthError> {{\n    crate::apple_runtime::BiometricGate::new({}u8, {}u32)\n}}\n",
                self.max_attempts, self.timeout_ms
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        face_id_sensor: true,
        camera_available: true,
        local_authentication: true,
        secure_enclave_match: true,
        utf8_strings: true,
    };

    fn declaration() -> FaceIdDeclaration {
        FaceIdDeclaration {
            max_attempts: 2,
            timeout_ms: 10_000,
            require_attention: true,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.max_attempts(), 2);
        assert_eq!(ir.timeout_ms(), 10_000);
        assert!(ir.require_attention());
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first.canonical_ir.contains("sensor=face_id\n"));
        assert!(first.canonical_ir.contains("max_attempts=2\n"));
        assert!(first.canonical_ir.contains("attention=required\n"));
        assert!(first.canonical_ir.contains("data_access=none\n"));
        assert!(first.source.contains("BiometricGate::new(2u8, 10000u32)"));
    }

    #[test]
    fn rejects_bad_attempt_and_timeout_bounds() {
        assert_eq!(
            FaceIdDeclaration {
                max_attempts: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroAttempts)
        );
        assert_eq!(
            FaceIdDeclaration {
                max_attempts: MAX_ATTEMPTS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::AttemptsTooLarge)
        );
        assert_eq!(
            FaceIdDeclaration {
                timeout_ms: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroTimeout)
        );
        assert_eq!(
            FaceIdDeclaration {
                timeout_ms: MAX_TIMEOUT_MS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::TimeoutTooLarge)
        );
    }

    #[test]
    fn negotiation_rejects_every_non_ios_target_and_missing_facilities() {
        let ir = declaration().normalize().unwrap();
        // Face ID hardware is not part of any Mac configuration.
        for target in [
            Target::AppleSiliconMac,
            Target::CrossPlatform,
            Target::Browser,
            Target::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, IOS),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    face_id_sensor: false,
                    ..IOS
                },
                Diagnostic::MissingFaceIdSensor,
            ),
            (
                RuntimeCapabilities {
                    camera_available: false,
                    ..IOS
                },
                Diagnostic::MissingCamera,
            ),
            (
                RuntimeCapabilities {
                    local_authentication: false,
                    ..IOS
                },
                Diagnostic::MissingLocalAuthentication,
            ),
            (
                RuntimeCapabilities {
                    secure_enclave_match: false,
                    ..IOS
                },
                Diagnostic::MissingSecureEnclaveMatch,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..IOS
                },
                Diagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(ir.lower_checked(Target::Ios, missing), Err(expected));
        }
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_apple_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::Ios, IOS)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let mut gate = make_face_id_gate().expect(\"factory should construct gate\");\n    assert_eq!(gate.max_attempts(), 2);\n    assert_eq!(gate.timeout_ms(), 10000);\n    assert_eq!(gate.attempt(true), Ok(apple_runtime::AuthOutcome::Admitted));\n    assert!(gate.is_resolved());\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-face-id-factory-{}-{nonce}",
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
            .expect("run generated Face ID factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
