//! NetBSD kauth/Veriexec integration.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Kauth listener scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KauthScope {
    Filesystem,
    Network,
    Process,
    Device,
    Vfs,
    Mount,
    Cred,
    System,
}

/// Kauth listener configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KauthListener {
    /// Listener scope
    pub scope: KauthScope,
    /// Action: allow, deny, defer
    pub action: KauthAction,
    /// Optional path pattern for filesystem scope
    pub path_pattern: Option<String>,
}

/// Kauth action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KauthAction {
    Allow,
    Deny,
    Defer,
}

/// Kauth sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KauthSandbox {
    /// Listeners to register
    pub listeners: Vec<KauthListener>,
    /// Whether to enable Veriexec
    pub veriexec: bool,
    /// Veriexec mode: strict, logging, disabled
    pub veriexec_mode: VeriexecMode,
}

impl Default for KauthSandbox {
    fn default() -> Self {
        Self {
            listeners: Vec::new(),
            veriexec: true,
            veriexec_mode: VeriexecMode::Strict,
        }
    }
}

/// Veriexec enforcement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VeriexecMode {
    Strict,
    Logging,
    Disabled,
}

/// Standard kauth listeners for GUI applications.
pub fn gui_listeners() -> Vec<KauthListener> {
    vec![
        KauthListener {
            scope: KauthScope::Filesystem,
            action: KauthAction::Allow,
            path_pattern: Some("/home/**".to_string()),
        },
        KauthListener {
            scope: KauthScope::Filesystem,
            action: KauthAction::Deny,
            path_pattern: Some("/etc/**".to_string()),
        },
        KauthListener {
            scope: KauthScope::Filesystem,
            action: KauthAction::Deny,
            path_pattern: Some("/usr/**".to_string()),
        },
        KauthListener {
            scope: KauthScope::Network,
            action: KauthAction::Allow,
            path_pattern: None,
        },
        KauthListener {
            scope: KauthScope::Process,
            action: KauthAction::Allow,
            path_pattern: None,
        },
    ]
}

/// Standard kauth listeners for headless applications.
pub fn headless_listeners() -> Vec<KauthListener> {
    vec![
        KauthListener {
            scope: KauthScope::Filesystem,
            action: KauthAction::Allow,
            path_pattern: Some("/home/**".to_string()),
        },
        KauthListener {
            scope: KauthScope::Filesystem,
            action: KauthAction::Deny,
            path_pattern: Some("/etc/**".to_string()),
        },
        KauthListener {
            scope: KauthScope::Process,
            action: KauthAction::Allow,
            path_pattern: None,
        },
    ]
}

/// Restricted kauth listeners (minimal).
pub fn restricted_listeners() -> Vec<KauthListener> {
    vec![
        KauthListener {
            scope: KauthScope::Filesystem,
            action: KauthAction::Allow,
            path_pattern: Some("/home/**".to_string()),
        },
        KauthListener {
            scope: KauthScope::Process,
            action: KauthAction::Allow,
            path_pattern: None,
        },
    ]
}

/// Initialise kauth sandbox (stub - would register listeners with kauth).
pub fn initialise_kauth(sandbox: &KauthSandbox) -> Result<(), Fault> {
    // In real implementation:
    // 1. Register kauth listeners for each scope
    // 2. Enable Veriexec if requested
    // 3. Set Veriexec mode

    // kauth_listen_scope(scope, callback) would go here
    // For Veriexec: sysctl kern.veriexec.enforce=1 (or 2 for logging)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_listeners_have_filesystem_network() {
        let listeners = gui_listeners();
        assert!(listeners.iter().any(|l| l.scope == KauthScope::Filesystem));
        assert!(listeners.iter().any(|l| l.scope == KauthScope::Network));
    }

    #[test]
    fn headless_listeners_no_network() {
        let listeners = headless_listeners();
        assert!(!listeners.iter().any(|l| l.scope == KauthScope::Network));
    }
}