//! OpenBSD pledge/unveil integration.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Pledge promises for OpenBSD.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PledgePromises(pub BTreeSet<String>);

impl PledgePromises {
    /// Create new empty promises.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a promise.
    pub fn add(mut self, promise: &str) -> Self {
        self.0.insert(promise.to_string());
        self
    }

    /// Add multiple promises.
    pub fn extend(mut self, promises: &[&str]) -> Self {
        for p in promises {
            self.0.insert(p.to_string());
        }
        self
    }

    /// Convert to space-separated string for pledge().
    pub fn to_string(&self) -> String {
        self.0.iter().cloned().collect::<Vec<_>>().join(" ")
    }
}

/// Unveil path with permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnveilPath {
    /// Path to unveil
    pub path: String,
    /// Permissions: "r", "w", "x", "rw", "rx", "rwx", "c" (create)
    pub permissions: String,
}

/// Pledge sandbox configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PledgeSandbox {
    /// Promises to make
    pub promises: PledgePromises,
    /// Paths to unveil
    pub unveil_paths: Vec<UnveilPath>,
    /// Whether to call pledge()
    pub do_pledge: bool,
    /// Whether to call unveil()
    pub do_unveil: bool,
}

impl Default for PledgeSandbox {
    fn default() -> Self {
        Self {
            promises: PledgePromises::new(),
            unveil_paths: Vec::new(),
            do_pledge: true,
            do_unveil: true,
        }
    }
}

/// Standard pledge promises for GUI applications.
pub fn gui_promises() -> PledgePromises {
    PledgePromises::new()
        .add("stdio")
        .add("rpath")
        .add("wpath")
        .add("cpath")
        .add("fatpath")
        .add("unix")
        .add("inet")
        .add("dns")
        .add("ttY")
        .add("proc")
        .add("exec")
}

/// Standard pledge promises for headless applications.
pub fn headless_promises() -> PledgePromises {
    PledgePromises::new()
        .add("stdio")
        .add("rpath")
        .add("wpath")
        .add("cpath")
        .add("unix")
        .add("proc")
}

/// Restricted pledge promises (minimal).
pub fn restricted_promises() -> PledgePromises {
    PledgePromises::new()
        .add("stdio")
        .add("rpath")
        .add("proc")
}

/// Initialise pledge sandbox (stub - would call pledge(), unveil()).
pub fn initialise_pledge(sandbox: &PledgeSandbox) -> Result<(), Fault> {
    // In real implementation:
    // 1. Call unveil() for each path
    // 2. Call pledge() with promises
    // 3. Any subsequent syscall outside promises/unveil will kill process

    if sandbox.do_unveil {
        for up in &sandbox.unveil_paths {
            // unveil(up.path, up.permissions) would go here
        }
        // unveil(nullptr, nullptr) to lock
    }

    if sandbox.do_pledge {
        // pledge(promises.to_string(), "") would go here
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pledge_promises_to_string() {
        let p = gui_promises();
        let s = p.to_string();
        assert!(s.contains("stdio"));
        assert!(s.contains("rpath"));
        assert!(s.contains("unix"));
    }

    #[test]
    fn gui_vs_headless_promises() {
        let gui = gui_promises();
        let headless = headless_promises();

        // GUI has more promises
        assert!(gui.0.len() > headless.0.len());
        assert!(gui.0.contains("inet"));
        assert!(!headless.0.contains("inet"));
    }
}