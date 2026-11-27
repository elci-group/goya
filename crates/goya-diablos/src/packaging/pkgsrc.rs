//! NetBSD pkgsrc format packaging.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// NetBSD pkgsrc package information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetbsdPkg {
    /// Package name
    pub name: String,
    /// Version
    pub version: String,
    /// Category
    pub category: String,
    /// Homepage
    pub homepage: String,
    /// Comment
    pub comment: String,
    /// License
    pub license: String,
    /// Dependencies
    pub depends: Vec<String>,
    /// Build dependencies
    pub build_depends: Vec<String>,
    /// Tools dependencies
    pub tool_depends: Vec<String>,
    /// Files
    pub files: Vec<PkgFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

impl NetbsdPkg {
    /// Create a new package builder.
    pub fn new(name: &str, version: &str, category: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            category: category.to_string(),
            homepage: String::new(),
            comment: String::new(),
            license: String::new(),
            depends: Vec::new(),
            build_depends: Vec::new(),
            tool_depends: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn homepage(mut self, homepage: &str) -> Self {
        self.homepage = homepage.to_string();
        self
    }

    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = comment.to_string();
        self
    }

    pub fn license(mut self, license: &str) -> Self {
        self.license = license.to_string();
        self
    }

    pub fn depend(mut self, dep: &str) -> Self {
        self.depends.push(dep.to_string());
        self
    }

    pub fn build_depend(mut self, dep: &str) -> Self {
        self.build_depends.push(dep.to_string());
        self
    }

    pub fn file(mut self, path: &str, size: u64, sha256: &str) -> Self {
        self.files.push(PkgFile {
            path: path.to_string(),
            size,
            sha256: sha256.to_string(),
        });
        self
    }

    pub fn build(self, _output: &Path) -> Result<(), Fault> {
        // In real implementation:
        // 1. Create pkgsrc package Makefile
        // 2. Generate PLIST
        // 3. Create distinfo
        // 4. Build binary package

        Ok(())
    }
}

/// Create a NetBSD pkgsrc package.
pub fn create_pkgsrc(
    app_name: &str,
    version: &str,
    _binary_path: &Path,
    _output_dir: &Path,
) -> Result<(), Fault> {
    let _pkg = NetbsdPkg::new(app_name, version, "goya")
        .homepage("https://goya.example")
        .comment("Goya application")
        .license("MIT")
        .depend("libc");

    // Placeholder for Phase 0
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkgsrc_basic() {
        let pkg = NetbsdPkg::new("test", "1.0.0", "goya");
        assert_eq!(pkg.name, "test");
        assert_eq!(pkg.version, "1.0.0");
        assert_eq!(pkg.category, "goya");
    }
}