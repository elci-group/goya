//! Security initialisation for Diablo Free runtime (Capsicum).

use goya_diablos::fault::Fault;
use goya_diablos::security::capsicum::{CapsicumSandbox as DiablosCapsicumSandbox, initialise_capsicum as diablos_initialise_capsicum, default_gui_rights, default_headless_rights};

/// Capsicum sandbox configuration (re-exported from goya-diablos).
pub type CapsicumSandbox = DiablosCapsicumSandbox;

/// Initialise Capsicum sandbox for the runtime.
pub fn initialise_capsicum(sandbox: &CapsicumSandbox) -> Result<(), Fault> {
    // This would be called early in runtime startup, before any privileged operations
    diablos_initialise_capsicum(sandbox)
}

/// Create default Capsicum sandbox for a GUI application.
pub fn default_gui_sandbox() -> CapsicumSandbox {
    CapsicumSandbox {
        enter_capability_mode: true,
        fd_rights: default_gui_rights(),
        allowed_fds: vec![
            "/dev/dri/card0".to_string(),
            "/dev/dri/renderD128".to_string(),
            "/var/run/wayland-0".to_string(),
        ],
    }
}

/// Create restricted Capsicum sandbox for a headless application.
pub fn default_headless_sandbox() -> CapsicumSandbox {
    CapsicumSandbox {
        enter_capability_mode: true,
        fd_rights: default_headless_rights(),
        allowed_fds: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_sandbox_has_dri() {
        let sandbox = default_gui_sandbox();
        assert!(sandbox.allowed_fds.iter().any(|f| f.contains("dri")));
    }

    #[test]
    fn headless_sandbox_empty_fds() {
        let sandbox = default_headless_sandbox();
        assert!(sandbox.allowed_fds.is_empty());
    }
}