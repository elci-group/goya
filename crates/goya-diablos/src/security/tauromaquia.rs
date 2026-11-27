//! Tauromaquia integration for Diablos (stub for Phase 0).

use crate::Fault;
use crate::security::privilege::ApplicationPrivileges;
use serde::{Deserialize, Serialize};

/// Tauromaquia security policy (placeholder for future implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TauromaquiaPolicy {
    /// Cape: capability attenuation
    pub capes: Vec<Cape>,
    /// Shield: execution isolation
    pub shields: Vec<Shield>,
    /// Spear: attestation/provenance
    pub spears: Vec<Spear>,
}

/// Capability attenuation (Cape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cape {
    pub name: String,
    pub attenuated_rights: Vec<String>,
}

/// Execution isolation (Shield).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shield {
    pub name: String,
    pub isolation_type: ShieldType,
}

/// Shield types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShieldType {
    Process,
    Jail,
    Container,
    Vm,
}

/// Attestation/Provenance (Spear).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spear {
    pub name: String,
    pub attestation_type: AttestationType,
}

/// Attestation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttestationType {
    Tpm,
    Sigstore,
    Notary,
    Local,
}

/// Verify Tauromaquia policy against application privileges (stub).
pub fn verify_tauromaquia(_policy: &TauromaquiaPolicy, _privileges: &ApplicationPrivileges) -> Result<(), Fault> {
    // Tauromaquia not yet implemented - this is a stub
    Ok(())
}

/// Synthesise Tauromaquia policy from BSD-native security configuration.
pub fn synthesise_tauromaquia(
    sandbox_type: &str,
    capsicum_rights: Option<&crate::security::capsicum::CapsicumRights>,
    pledge_promises: Option<&crate::security::pledge::PledgePromises>,
) -> TauromaquiaPolicy {
    let mut capes = Vec::new();
    let mut shields = Vec::new();
    let mut spears = Vec::new();

    match sandbox_type {
        "capsicum" => {
            shields.push(Shield {
                name: "capsicum-sandbox".to_string(),
                isolation_type: ShieldType::Process,
            });
            if let Some(rights) = capsicum_rights {
                capes.push(Cape {
                    name: "capsicum-rights".to_string(),
                    attenuated_rights: vec![
                        format!("fd_rights: {:#x}", rights.fd_rights),
                        format!("cap_rights: {:#x}", rights.cap_rights),
                    ],
                });
            }
        }
        "pledge" => {
            shields.push(Shield {
                name: "pledge-sandbox".to_string(),
                isolation_type: ShieldType::Process,
            });
            if let Some(promises) = pledge_promises {
                capes.push(Cape {
                    name: "pledge-promises".to_string(),
                    attenuated_rights: promises.0.iter().cloned().collect(),
                });
            }
        }
        "kauth" => {
            shields.push(Shield {
                name: "kauth-sandbox".to_string(),
                isolation_type: ShieldType::Process,
            });
        }
        "jail" => {
            shields.push(Shield {
                name: "freebsd-jail".to_string(),
                isolation_type: ShieldType::Jail,
            });
        }
        _ => {}
    }

    // Default spear for provenance
    spears.push(Spear {
        name: "provenance".to_string(),
        attestation_type: AttestationType::Local,
    });

    TauromaquiaPolicy { capes, shields, spears }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesise_capsicum() {
        let policy = synthesise_tauromaquia("capsicum", None, None);
        assert!(policy.shields.iter().any(|s| s.name == "capsicum-sandbox"));
    }

    #[test]
    fn synthesise_pledge() {
        let policy = synthesise_tauromaquia("pledge", None, None);
        assert!(policy.shields.iter().any(|s| s.name == "pledge-sandbox"));
    }

    #[test]
    fn synthesise_jail() {
        let policy = synthesise_tauromaquia("jail", None, None);
        assert!(policy.shields.iter().any(|s| matches!(s.isolation_type, ShieldType::Jail)));
    }
}