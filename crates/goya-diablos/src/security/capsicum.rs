//! FreeBSD Capsicum capability sandbox integration.

use crate::Fault;
use serde::{Deserialize, Serialize};

/// Capsicum capability rights.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapsicumRights {
    /// File descriptor rights (CAP_READ, CAP_WRITE, etc.)
    pub fd_rights: u64,
    /// Capability rights (CAP_FCNTL, CAP_IOCTL, etc.)
    pub cap_rights: u64,
}

impl CapsicumRights {
    /// Create new empty rights.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add file descriptor rights.
    pub fn with_fd_rights(mut self, rights: u64) -> Self {
        self.fd_rights |= rights;
        self
    }

    /// Add capability rights.
    pub fn with_cap_rights(mut self, rights: u64) -> Self {
        self.cap_rights |= rights;
        self
    }
}

/// Capsicum sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapsicumSandbox {
    /// Whether to enter capability mode
    pub enter_capability_mode: bool,
    /// Rights to apply to file descriptors
    pub fd_rights: CapsicumRights,
    /// List of allowed file descriptors (by path)
    pub allowed_fds: Vec<String>,
}

impl Default for CapsicumSandbox {
    fn default() -> Self {
        Self {
            enter_capability_mode: true,
            fd_rights: CapsicumRights::default(),
            allowed_fds: Vec::new(),
        }
    }
}

/// Capsicum constants (from FreeBSD sys/capsicum.h).
pub mod capsicum_consts {
    // File descriptor rights
    pub const CAP_READ: u64 = 1 << 0;
    pub const CAP_WRITE: u64 = 1 << 1;
    pub const CAP_SEEK: u64 = 1 << 2;
    pub const CAP_FCNTL: u64 = 1 << 3;
    pub const CAP_IOCTL: u64 = 1 << 4;
    pub const CAP_PREAD: u64 = 1 << 5;
    pub const CAP_PWRITE: u64 = 1 << 6;
    pub const CAP_READ_STR: u64 = 1 << 7;
    pub const CAP_WRITE_STR: u64 = 1 << 8;
    pub const CAP_LOOKUP: u64 = 1 << 9;
    pub const CAP_CREATE: u64 = 1 << 10;
    pub const CAP_RENAME: u64 = 1 << 11;
    pub const CAP_UNLINK: u64 = 1 << 12;
    pub const CAP_MKDIR: u64 = 1 << 13;
    pub const CAP_RMDIR: u64 = 1 << 14;
    pub const CAP_LINK: u64 = 1 << 15;
    pub const CAP_SYMLINK: u64 = 1 << 16;
    pub const CAP_CHMOD: u64 = 1 << 17;
    pub const CAP_FCHMOD: u64 = 1 << 18;
    pub const CAP_FSYNC: u64 = 1 << 19;
    pub const CAP_MMAP: u64 = 1 << 20;
    pub const CAP_MMAP_R: u64 = 1 << 21;
    pub const CAP_MMAP_W: u64 = 1 << 22;
    pub const CAP_MMAP_X: u64 = 1 << 23;
    pub const CAP_POLL: u64 = 1 << 24;
    pub const CAP_KQUEUE: u64 = 1 << 25;
    pub const CAP_KQUEUE_CHANGE: u64 = 1 << 26;
    pub const CAP_EVENT: u64 = 1 << 27;
    pub const CAP_SEM: u64 = 1 << 28;
    pub const CAP_SHM: u64 = 1 << 29;
    pub const CAP_PDFORK: u64 = 1 << 30;
    pub const CAP_PDKILL: u64 = 1 << 31;

    // Capability rights
    pub const CAP_FCNTL_GETFL: u64 = 1 << 0;
    pub const CAP_FCNTL_SETFL: u64 = 1 << 1;
    pub const CAP_FCNTL_GETFD: u64 = 1 << 2;
    pub const CAP_FCNTL_SETFD: u64 = 1 << 3;
    pub const CAP_FCNTL_DUP: u64 = 1 << 4;
    pub const CAP_FCNTL_DUP2: u64 = 1 << 5;
    pub const CAP_CAP_IOCTL: u64 = 1 << 6;
    pub const CAP_CAP_IOCTL_LONG: u64 = 1 << 7;
}

/// Create default Capsicum rights for a GUI application.
pub fn default_gui_rights() -> CapsicumRights {
    use capsicum_consts::*;
    CapsicumRights::new()
        .with_fd_rights(
            CAP_READ
                | CAP_WRITE
                | CAP_SEEK
                | CAP_FCNTL
                | CAP_IOCTL
                | CAP_PREAD
                | CAP_PWRITE
                | CAP_POLL
                | CAP_KQUEUE
                | CAP_MMAP
                | CAP_MMAP_R
        )
        .with_cap_rights(CAP_FCNTL_GETFL | CAP_FCNTL_SETFL | CAP_IOCTL)
}

/// Create restricted Capsicum rights for a headless application.
pub fn default_headless_rights() -> CapsicumRights {
    use capsicum_consts::*;
    CapsicumRights::new()
        .with_fd_rights(CAP_READ | CAP_WRITE | CAP_SEEK | CAP_FCNTL | CAP_PREAD | CAP_PWRITE)
        .with_cap_rights(CAP_FCNTL_GETFL | CAP_FCNTL_SETFL)
}

/// Initialise Capsicum sandbox (stub - would call cap_enter, cap_rights_limit).
pub fn initialise_capsicum(sandbox: &CapsicumSandbox) -> Result<(), Fault> {
    // In real implementation:
    // 1. Open required file descriptors
    // 2. Apply cap_rights_limit to each
    // 3. Call cap_enter() to enter capability mode
    // 4. Verify with cap_getmode()

    if sandbox.enter_capability_mode {
        // cap_enter() would go here
    }

    Ok(())
}

/// Apply rights to a file descriptor (stub).
pub fn apply_rights(_fd: i32, _rights: &CapsicumRights) -> Result<(), Fault> {
    // cap_rights_limit(fd, &rights) would go here
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capsicum_rights_builder() {
        let rights = CapsicumRights::new()
            .with_fd_rights(1)
            .with_cap_rights(2);
        assert_eq!(rights.fd_rights, 1);
        assert_eq!(rights.cap_rights, 2);
    }

    #[test]
    fn default_gui_rights_has_basic() {
        let rights = default_gui_rights();
        assert!(rights.fd_rights & capsicum_consts::CAP_READ != 0);
        assert!(rights.fd_rights & capsicum_consts::CAP_WRITE != 0);
    }

    #[test]
    fn default_headless_rights_minimal() {
        let rights = default_headless_rights();
        assert!(rights.fd_rights & capsicum_consts::CAP_READ != 0);
        assert!(rights.fd_rights & capsicum_consts::CAP_WRITE != 0);
        // Should not have MMAP rights for headless
        assert!(rights.fd_rights & capsicum_consts::CAP_MMAP == 0);
    }
}