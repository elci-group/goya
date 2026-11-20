//! A namespace-bounded Apple Keychain secure-storage capability for iOS and
//! Apple Silicon Mac hosts.
//!
//! The negotiated artifact binds to this crate's process-local runtime; the
//! artifact is a factory definition. It does not call Keychain Services, claim
//! Secure Enclave hardware backing, or prove that the generated crate path is
//! linked in a consumer crate. Hosts on Apple hardware report their real
//! Keychain facilities through [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.secure_storage.keychain.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_NAMESPACE_BYTES: usize = 128;
pub const MAX_VALUE_BYTES: usize = 1 << 20;
pub const MAX_ITEMS: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeychainDeclaration {
    /// Keychain service namespace, non-empty and bounded.
    pub namespace: String,
    /// Maximum accepted value bytes, from one through `MAX_VALUE_BYTES`.
    pub max_value_bytes: usize,
    /// Keychain accessibility class applied to every item in the namespace.
    pub accessibility: Accessibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Accessibility {
    /// Readable only while the device is unlocked.
    WhenUnlocked,
    /// Readable only while the device is unlocked; never migrated between
    /// devices.
    WhenUnlockedThisDeviceOnly,
}

impl Accessibility {
    fn label(self) -> &'static str {
        match self {
            Self::WhenUnlocked => "when_unlocked",
            Self::WhenUnlockedThisDeviceOnly => "when_unlocked_this_device_only",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeychainIr {
    namespace: String,
    max_value_bytes: usize,
    accessibility: Accessibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// iOS device or simulator Keychain.
    Ios,
    /// Apple Silicon macOS Keychain.
    AppleSiliconMac,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub keychain_services: bool,
    /// Device exposes hardware-backed key material (Secure Enclave).
    pub hardware_backed_keys: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    EmptyNamespace,
    NamespaceTooLong,
    ValueBytesZero,
    ValueBytesTooLarge,
    ItemBoundTooLarge,
    UnsupportedTarget,
    MissingKeychainServices,
    MissingHardwareBackedKeys,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyNamespace => "keychain namespace must not be empty",
            Self::NamespaceTooLong => "keychain namespace exceeds the supported bound",
            Self::ValueBytesZero => "keychain value bound must be positive",
            Self::ValueBytesTooLarge => "keychain value bound exceeds the supported bound",
            Self::ItemBoundTooLarge => "item count bound exceeds the supported bound",
            Self::UnsupportedTarget => {
                "keychain factory supports only iOS and Apple Silicon Mac targets"
            }
            Self::MissingKeychainServices => "target runtime must provide Keychain Services",
            Self::MissingHardwareBackedKeys => "target must report hardware-backed key material",
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeychainArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl KeychainDeclaration {
    pub fn normalize(self) -> Result<KeychainIr, Diagnostic> {
        if self.namespace.is_empty() {
            return Err(Diagnostic::EmptyNamespace);
        }
        if self.namespace.len() > MAX_NAMESPACE_BYTES {
            return Err(Diagnostic::NamespaceTooLong);
        }
        if self.max_value_bytes == 0 {
            return Err(Diagnostic::ValueBytesZero);
        }
        if self.max_value_bytes > MAX_VALUE_BYTES {
            return Err(Diagnostic::ValueBytesTooLarge);
        }
        Ok(KeychainIr {
            namespace: self.namespace,
            max_value_bytes: self.max_value_bytes,
            accessibility: self.accessibility,
        })
    }
}

impl KeychainIr {
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn max_value_bytes(&self) -> usize {
        self.max_value_bytes
    }

    pub fn accessibility(&self) -> Accessibility {
        self.accessibility
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::Ios => "ios",
            Target::AppleSiliconMac => "apple_silicon_mac",
            Target::Browser | Target::Node => "unsupported",
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nstore=apple_keychain\nnamespace={}\nmax_value_bytes={}\naccessibility={}\nsync=this_device_scope\n",
            Self::target_label(target),
            self.namespace,
            self.max_value_bytes,
            self.accessibility.label()
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<KeychainArtifact, Diagnostic> {
        if !matches!(target, Target::Ios | Target::AppleSiliconMac) {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.keychain_services {
            return Err(Diagnostic::MissingKeychainServices);
        }
        if !capabilities.hardware_backed_keys {
            return Err(Diagnostic::MissingHardwareBackedKeys);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        Ok(KeychainArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; {} Keychain namespace, oversized values are rejected.\npub fn make_keychain_vault() -> Result<crate::apple_runtime::KeychainVault, crate::apple_runtime::KeychainError> {{\n    crate::apple_runtime::KeychainVault::new({:?}.to_owned(), {}usize, {}usize)\n}}\n",
                Self::target_label(target),
                self.namespace,
                self.max_value_bytes,
                MAX_ITEMS
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IOS: RuntimeCapabilities = RuntimeCapabilities {
        keychain_services: true,
        hardware_backed_keys: true,
        utf8_strings: true,
    };

    fn declaration(namespace: &str) -> KeychainDeclaration {
        KeychainDeclaration {
            namespace: namespace.to_owned(),
            max_value_bytes: 4096,
            accessibility: Accessibility::WhenUnlockedThisDeviceOnly,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration("com.goya.ios.test").normalize().unwrap();
        assert_eq!(ir.namespace(), "com.goya.ios.test");
        assert_eq!(ir.max_value_bytes(), 4096);
        assert_eq!(
            ir.accessibility(),
            Accessibility::WhenUnlockedThisDeviceOnly
        );
        let first = ir.lower_checked(Target::Ios, IOS).unwrap();
        let second = ir.lower_checked(Target::Ios, IOS).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=ios\n"));
        assert!(first.canonical_ir.contains("store=apple_keychain\n"));
        assert!(first.canonical_ir.contains("namespace=com.goya.ios.test\n"));
        assert!(first
            .canonical_ir
            .contains("accessibility=when_unlocked_this_device_only\n"));
        assert!(first.canonical_ir.contains("sync=this_device_scope\n"));
        assert!(first.source.contains("make_keychain_vault"));
        assert!(first
            .source
            .contains("KeychainVault::new(\"com.goya.ios.test\".to_owned(), 4096usize"));
    }

    #[test]
    fn apple_silicon_mac_lowers_with_its_own_target_label() {
        let ir = declaration("com.goya.macos.test").normalize().unwrap();
        let artifact = ir.lower_checked(Target::AppleSiliconMac, IOS).unwrap();
        assert!(artifact.canonical_ir.contains("target=apple_silicon_mac\n"));
        assert!(artifact.source.contains("apple_silicon_mac Keychain"));
    }

    #[test]
    fn rejects_empty_long_namespace_and_bad_value_bounds() {
        assert_eq!(declaration("").normalize(), Err(Diagnostic::EmptyNamespace));
        let long = "n".repeat(MAX_NAMESPACE_BYTES + 1);
        assert_eq!(
            declaration(&long).normalize(),
            Err(Diagnostic::NamespaceTooLong)
        );
        let mut oversized = declaration("ns");
        oversized.max_value_bytes = MAX_VALUE_BYTES + 1;
        assert_eq!(oversized.normalize(), Err(Diagnostic::ValueBytesTooLarge));
        let mut zero = declaration("ns");
        zero.max_value_bytes = 0;
        assert_eq!(zero.normalize(), Err(Diagnostic::ValueBytesZero));
    }

    #[test]
    fn negotiation_rejects_other_targets_and_missing_facilities() {
        let ir = declaration("ns").normalize().unwrap();
        for target in [Target::Browser, Target::Node] {
            assert_eq!(
                ir.lower_checked(target, IOS),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    keychain_services: false,
                    ..IOS
                },
                Diagnostic::MissingKeychainServices,
            ),
            (
                RuntimeCapabilities {
                    hardware_backed_keys: false,
                    ..IOS
                },
                Diagnostic::MissingHardwareBackedKeys,
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

        let artifact = declaration("com.goya.compile.test")
            .normalize()
            .unwrap()
            .lower_checked(Target::Ios, IOS)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let vault = make_keychain_vault().expect(\"factory should construct vault\");\n    assert_eq!(vault.namespace(), \"com.goya.compile.test\");\n    vault.set(\"token\", b\"value\").unwrap();\n    assert_eq!(vault.get(\"token\").unwrap(), Some(b\"value\".to_vec()));\n    assert_eq!(vault.set(\"big\", &[0u8; 4097]), Err(apple_runtime::KeychainError::ValueTooLarge));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-keychain-factory-{}-{nonce}",
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
            .expect("run generated keychain factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
