//! Privilege derivation from application intent.

use crate::capabilities::registry::ApplicationRequirements;
use crate::Fault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Application privileges derived from intent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationPrivileges {
    /// Filesystem read access
    pub fs_read: BTreeSet<String>,
    /// Filesystem write access
    pub fs_write: BTreeSet<String>,
    /// Network outbound access
    pub net_outbound: BTreeSet<String>,
    /// Network inbound ports
    pub net_inbound: BTreeSet<u16>,
    /// Subprocess execution
    pub subprocess: BTreeSet<String>,
    /// Device access
    pub devices: BTreeSet<String>,
    /// IPC access
    pub ipc: BTreeSet<String>,
    /// Graphics access
    pub graphics: bool,
    /// Neural access
    pub neural_local: bool,
    pub neural_distributed: bool,
}

impl Default for ApplicationPrivileges {
    fn default() -> Self {
        Self {
            fs_read: BTreeSet::new(),
            fs_write: BTreeSet::new(),
            net_outbound: BTreeSet::new(),
            net_inbound: BTreeSet::new(),
            subprocess: BTreeSet::new(),
            devices: BTreeSet::new(),
            ipc: BTreeSet::new(),
            graphics: false,
            neural_local: false,
            neural_distributed: false,
        }
    }
}

/// Derive privileges from application requirements.
pub fn derive_privileges(requirements: &ApplicationRequirements) -> Result<ApplicationPrivileges, Fault> {
    let mut privileges = ApplicationPrivileges::default();

    // Filesystem
    for path in &requirements.filesystem {
        privileges.fs_read.insert(path.clone());
        // Heuristic: paths not in system directories are writable
        if !path.starts_with("/etc") && !path.starts_with("/usr") && !path.starts_with("/boot") && !path.starts_with("/lib") {
            privileges.fs_write.insert(path.clone());
        }
    }

    // Network
    for host in &requirements.network {
        privileges.net_outbound.insert(host.clone());
    }
    if requirements.subprocess {
        // Subprocess might need network for downloading
        privileges.net_outbound.insert("*".to_string());
    }

    // Subprocess
    if requirements.subprocess {
        privileges.subprocess.insert("*".to_string());
    }

    // Devices
    for dev in &requirements.devices {
        privileges.devices.insert(dev.clone());
    }

    // IPC
    if requirements.ipc {
        privileges.ipc.insert("unix".to_string());
        privileges.ipc.insert("shared_memory".to_string());
    }

    // Graphics
    if requirements.graphics.hardware_acceleration {
        privileges.graphics = true;
    }

    // Neural
    privileges.neural_local = requirements.neural.local;
    privileges.neural_distributed = requirements.neural.distributed;

    Ok(privileges)
}

/// Minimise privileges to the narrowest practical set.
pub fn minimise_privileges(privileges: &mut ApplicationPrivileges, target_sandbox: &str) {
    match target_sandbox {
        "capsicum" => {
            // Capsicum: limit to specific file descriptors
            // Remove wildcard network if not explicitly needed
            if privileges.net_outbound.contains("*") && privileges.net_outbound.len() == 1 {
                // Keep if explicitly needed
            }
        }
        "pledge" => {
            // Pledge: use promise strings
            // Remove capabilities not covered by promises
        }
        "kauth" => {
            // Kauth: use listener scopes
        }
        _ => {}
    }

    // Always deny system directories
    privileges.fs_read.retain(|p| !is_system_directory(p));
    privileges.fs_write.retain(|p| !is_system_directory(p));
}

fn is_system_directory(path: &str) -> bool {
    path.starts_with("/etc")
        || path.starts_with("/usr")
        || path.starts_with("/boot")
        || path.starts_with("/lib")
        || path.starts_with("/bin")
        || path.starts_with("/sbin")
        || path.starts_with("/proc")
        || path.starts_with("/sys")
        || path.starts_with("/dev")
        || path.starts_with("/run")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_privileges_basic() {
        let mut req = ApplicationRequirements::default();
        req.filesystem.push("/home/user/data".to_string());
        req.network.push("api.example.com".to_string());

        let privs = derive_privileges(&req).unwrap();
        assert!(privs.fs_read.contains("/home/user/data"));
        assert!(privs.fs_write.contains("/home/user/data"));
        assert!(privs.net_outbound.contains("api.example.com"));
    }

    #[test]
    fn derive_privileges_system_dirs_not_writable() {
        let mut req = ApplicationRequirements::default();
        req.filesystem.push("/etc/config".to_string());

        let privs = derive_privileges(&req).unwrap();
        assert!(privs.fs_read.contains("/etc/config"));
        assert!(!privs.fs_write.contains("/etc/config")); // System dir not writable
    }

    #[test]
    fn minimise_privileges_removes_system_dirs() {
        let mut privs = ApplicationPrivileges::default();
        privs.fs_read.insert("/etc/passwd".to_string());
        privs.fs_write.insert("/etc/passwd".to_string());
        privs.fs_read.insert("/home/user/data".to_string());
        privs.fs_write.insert("/home/user/data".to_string());

        minimise_privileges(&mut privs, "capsicum");

        assert!(!privs.fs_read.contains("/etc/passwd"));
        assert!(!privs.fs_write.contains("/etc/passwd"));
        assert!(privs.fs_read.contains("/home/user/data"));
        assert!(privs.fs_write.contains("/home/user/data"));
    }
}