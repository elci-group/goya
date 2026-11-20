//! A bounded Samsung Pay payment-request capability for Samsung Android
//! devices.
//!
//! The negotiated artifact binds to this crate's process-local payment
//! runtime. It does not contact a payment network, move funds, or prove that
//! Samsung Wallet services are linked in a consumer crate. Hosts running on
//! Samsung hardware report their real Samsung Pay facilities through
//! [`RuntimeCapabilities`].

use std::fmt;

pub const CONTRACT: &str = "goya.hardware.payments.samsung_pay.v1";
pub const RECIPE_VERSION: &str = "0.0.1";
pub const MAX_MERCHANT_ID_BYTES: usize = crate::apple_runtime::MAX_MERCHANT_ID_BYTES;
pub const MAX_MINOR_UNITS: u64 = crate::apple_runtime::MAX_MINOR_UNITS;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamsungPayDeclaration {
    /// Merchant identifier, a non-empty dotted ASCII name within
    /// `MAX_MERCHANT_ID_BYTES`.
    pub merchant_id: String,
    /// ISO 4217 currency code, three uppercase ASCII letters.
    pub currency_code: String,
    /// Payment amount bound in minor units, from one through
    /// `MAX_MINOR_UNITS`.
    pub max_minor_units: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamsungPayIr {
    merchant_id: String,
    currency_code: String,
    max_minor_units: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    /// Samsung Android device with Samsung Wallet provisioned.
    SamsungAndroid,
    /// Samsung Pay is not available on other Android vendors.
    Android,
    CrossPlatform,
    Browser,
    Node,
}

/// Explicitly reported host facilities required by this lowering. The default
/// is deliberately unsupported until capabilities are supplied.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub samsung_pay_services: bool,
    /// Device is a Samsung handset or tablet.
    pub samsung_device: bool,
    /// Device reports a Secure Element payment credential.
    pub secure_element_credential: bool,
    pub network_session: bool,
    pub utf8_strings: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    BadMerchantId,
    BadCurrencyCode,
    ZeroAmountBound,
    AmountBoundTooLarge,
    UnsupportedTarget,
    MissingSamsungPayServices,
    MissingSamsungDevice,
    MissingSecureElementCredential,
    MissingNetworkSession,
    MissingUtf8Strings,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadMerchantId => {
                "merchant identifier must be a non-empty dotted ASCII name within the bound"
            }
            Self::BadCurrencyCode => "currency code must be three uppercase ASCII letters",
            Self::ZeroAmountBound => "payment amount bound must be positive",
            Self::AmountBoundTooLarge => "payment amount bound exceeds the supported bound",
            Self::UnsupportedTarget => "Samsung Pay factory supports only Samsung Android devices",
            Self::MissingSamsungPayServices => "target runtime must provide Samsung Pay services",
            Self::MissingSamsungDevice => "target must be a Samsung device",
            Self::MissingSecureElementCredential => {
                "device must report a Secure Element payment credential"
            }
            Self::MissingNetworkSession => "target runtime must provide a payment session",
            Self::MissingUtf8Strings => "target runtime must provide UTF-8 strings",
        })
    }
}

impl std::error::Error for Diagnostic {}

/// Negotiated design and source text. The artifact is a factory definition;
/// successful construction is not proof that the generated code was compiled,
/// that a session ran, or that funds moved.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SamsungPayArtifact {
    pub contract: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

impl SamsungPayDeclaration {
    pub fn normalize(self) -> Result<SamsungPayIr, Diagnostic> {
        if self.merchant_id.is_empty()
            || self.merchant_id.len() > MAX_MERCHANT_ID_BYTES
            || !self
                .merchant_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err(Diagnostic::BadMerchantId);
        }
        let bytes = self.currency_code.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(|b| b.is_ascii_uppercase()) {
            return Err(Diagnostic::BadCurrencyCode);
        }
        if self.max_minor_units == 0 {
            return Err(Diagnostic::ZeroAmountBound);
        }
        if self.max_minor_units > MAX_MINOR_UNITS {
            return Err(Diagnostic::AmountBoundTooLarge);
        }
        Ok(SamsungPayIr {
            merchant_id: self.merchant_id,
            currency_code: self.currency_code,
            max_minor_units: self.max_minor_units,
        })
    }
}

impl SamsungPayIr {
    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }

    pub fn currency_code(&self) -> &str {
        &self.currency_code
    }

    pub fn max_minor_units(&self) -> u64 {
        self.max_minor_units
    }

    fn target_label(target: Target) -> &'static str {
        match target {
            Target::SamsungAndroid => "samsung_android",
            Target::Android | Target::CrossPlatform | Target::Browser | Target::Node => {
                "unsupported"
            }
        }
    }

    pub fn canonical_for(&self, target: Target) -> String {
        format!(
            "contract={CONTRACT}\nrecipe_version={RECIPE_VERSION}\ntarget={}\nrequest=samsung_pay\nmerchant_id={}\ncurrency={}\nmax_minor_units={}\nnetworks=visa,mastercard\nfunds_moved=none\n",
            Self::target_label(target),
            self.merchant_id,
            self.currency_code,
            self.max_minor_units
        )
    }

    pub fn lower_checked(
        &self,
        target: Target,
        capabilities: RuntimeCapabilities,
    ) -> Result<SamsungPayArtifact, Diagnostic> {
        if target != Target::SamsungAndroid {
            return Err(Diagnostic::UnsupportedTarget);
        }
        if !capabilities.samsung_pay_services {
            return Err(Diagnostic::MissingSamsungPayServices);
        }
        if !capabilities.samsung_device {
            return Err(Diagnostic::MissingSamsungDevice);
        }
        if !capabilities.secure_element_credential {
            return Err(Diagnostic::MissingSecureElementCredential);
        }
        if !capabilities.network_session {
            return Err(Diagnostic::MissingNetworkSession);
        }
        if !capabilities.utf8_strings {
            return Err(Diagnostic::MissingUtf8Strings);
        }
        Ok(SamsungPayArtifact {
            contract: CONTRACT,
            recipe_version: RECIPE_VERSION,
            canonical_ir: self.canonical_for(target),
            source: format!(
                "// {CONTRACT}; recipe={RECIPE_VERSION}; samsung_android Samsung Pay request, no payment session runs and no funds move.\npub fn make_payment_request() -> Result<crate::apple_runtime::PaymentRequest, crate::apple_runtime::PaymentError> {{\n    crate::apple_runtime::PaymentRequest::new({:?}.to_owned(), {:?}, {}u64)\n}}\n",
                self.merchant_id,
                self.currency_code,
                self.max_minor_units
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMSUNG: RuntimeCapabilities = RuntimeCapabilities {
        samsung_pay_services: true,
        samsung_device: true,
        secure_element_credential: true,
        network_session: true,
        utf8_strings: true,
    };

    fn declaration() -> SamsungPayDeclaration {
        SamsungPayDeclaration {
            merchant_id: "com.goya.pay".into(),
            currency_code: "EUR".into(),
            max_minor_units: 50_000,
        }
    }

    #[test]
    fn declaration_normalizes_and_lowers_to_deterministic_factory() {
        let ir = declaration().normalize().unwrap();
        assert_eq!(ir.merchant_id(), "com.goya.pay");
        assert_eq!(ir.currency_code(), "EUR");
        assert_eq!(ir.max_minor_units(), 50_000);
        let first = ir.lower_checked(Target::SamsungAndroid, SAMSUNG).unwrap();
        let second = ir.lower_checked(Target::SamsungAndroid, SAMSUNG).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.contract, CONTRACT);
        assert!(first.canonical_ir.contains("target=samsung_android\n"));
        assert!(first.canonical_ir.contains("request=samsung_pay\n"));
        assert!(first.canonical_ir.contains("merchant_id=com.goya.pay\n"));
        assert!(first.canonical_ir.contains("currency=EUR\n"));
        assert!(first.canonical_ir.contains("max_minor_units=50000\n"));
        assert!(first.canonical_ir.contains("networks=visa,mastercard\n"));
        assert!(first.canonical_ir.contains("funds_moved=none\n"));
        assert!(first
            .source
            .contains("PaymentRequest::new(\"com.goya.pay\".to_owned(), \"EUR\", 50000u64)"));
    }

    #[test]
    fn rejects_non_samsung_android_and_bad_bounds() {
        let ir = declaration().normalize().unwrap();
        // Samsung Pay is not available on other Android vendors or browsers.
        for target in [
            Target::Android,
            Target::CrossPlatform,
            Target::Browser,
            Target::Node,
        ] {
            assert_eq!(
                ir.lower_checked(target, SAMSUNG),
                Err(Diagnostic::UnsupportedTarget)
            );
        }
        assert_eq!(
            SamsungPayDeclaration {
                merchant_id: String::new(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadMerchantId)
        );
        assert_eq!(
            SamsungPayDeclaration {
                currency_code: "eur".into(),
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::BadCurrencyCode)
        );
        assert_eq!(
            SamsungPayDeclaration {
                max_minor_units: 0,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::ZeroAmountBound)
        );
        assert_eq!(
            SamsungPayDeclaration {
                max_minor_units: MAX_MINOR_UNITS + 1,
                ..declaration()
            }
            .normalize(),
            Err(Diagnostic::AmountBoundTooLarge)
        );
    }

    #[test]
    fn negotiation_rejects_missing_facilities() {
        let ir = declaration().normalize().unwrap();
        for (missing, expected) in [
            (
                RuntimeCapabilities {
                    samsung_pay_services: false,
                    ..SAMSUNG
                },
                Diagnostic::MissingSamsungPayServices,
            ),
            (
                RuntimeCapabilities {
                    samsung_device: false,
                    ..SAMSUNG
                },
                Diagnostic::MissingSamsungDevice,
            ),
            (
                RuntimeCapabilities {
                    secure_element_credential: false,
                    ..SAMSUNG
                },
                Diagnostic::MissingSecureElementCredential,
            ),
            (
                RuntimeCapabilities {
                    network_session: false,
                    ..SAMSUNG
                },
                Diagnostic::MissingNetworkSession,
            ),
            (
                RuntimeCapabilities {
                    utf8_strings: false,
                    ..SAMSUNG
                },
                Diagnostic::MissingUtf8Strings,
            ),
        ] {
            assert_eq!(
                ir.lower_checked(Target::SamsungAndroid, missing),
                Err(expected)
            );
        }
    }

    #[test]
    fn generated_factory_compiles_and_runs_against_payment_runtime() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        use std::time::{SystemTime, UNIX_EPOCH};

        let artifact = declaration()
            .normalize()
            .unwrap()
            .lower_checked(Target::SamsungAndroid, SAMSUNG)
            .unwrap();
        let runtime_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/apple_runtime.rs");
        assert!(runtime_path.is_file());
        let harness = format!(
            "#[path = {runtime_path:?}]\nmod apple_runtime;\n{}\nfn main() {{\n    let request = make_payment_request().expect(\"factory should construct request\");\n    assert_eq!(request.merchant_id(), \"com.goya.pay\");\n    assert_eq!(request.currency_code(), \"EUR\");\n    assert_eq!(request.max_minor_units(), 50000);\n    request.authorize(apple_runtime::PaymentNetwork::Visa, 12345).unwrap();\n    assert_eq!(request.authorize(apple_runtime::PaymentNetwork::Amex, 1), Err(apple_runtime::PaymentError::UnsupportedNetwork));\n}}\n",
            artifact.source
        );
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let output_path = std::env::temp_dir().join(format!(
            "goya-generated-samsung-pay-factory-{}-{nonce}",
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
            .expect("run generated Samsung Pay factory");
        let _ = std::fs::remove_file(&output_path);
        assert!(
            execution.status.success(),
            "generated factory failed at runtime:\n{}",
            String::from_utf8_lossy(&execution.stderr)
        );
    }
}
