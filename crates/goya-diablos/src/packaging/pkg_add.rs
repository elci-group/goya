//! OpenBSD pkg_add format packaging.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// OpenBSD package information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenbsdPkg {
    /// Package name
    pub name: String,
    /// Version
    pub version: String,
    /// Comment
    pub comment: String,
    /// Description
    pub desc: String,
    /// Maintainer
    pub maintainer: String,
    /// Homepage
    pub homepage: String,
    /// License
    pub license: String,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Run depends
    pub run_depends: Vec<String>,
    /// Build depends
    pub build_depends: Vec<String>,
    /// Files
    pub files: Vec<PkgFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

impl OpenbsdPkg {
    /// Create a new package builder.
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            comment: String::new(),
            desc: String::new(),
            maintainer: String::new(),
            homepage: String::new(),
            license: String::new(),
            dependencies: Vec::new(),
            run_depends: Vec::new(),
            build_depends: Vec::new(),
            files: Vec::new(),
        }
    }

    pub fn comment(mut self, comment: &str) -> Self {
        self.comment = comment.to_string();
        self
    }

    pub fn desc(mut self, desc: &str) -> Self {
        self.desc = desc.to_string();
        self
    }

    pub fn maintainer(mut self, maintainer: &str) -> Self {
        self.maintainer = maintainer.to_string();
        self
    }

    pub fn homepage(mut self, homepage: &str) -> Self {
        self.homepage = homepage.to_string();
        self
    }

    pub fn license(mut self, license: &str) -> Self {
        self.license = license.to_string();
        self
    }

    pub fn depend(mut self, dep: &str) -> Self {
        self.dependencies.push(dep.to_string());
        self
    }

    pub fn run_depend(mut self, dep: &str) -> Self {
        self.run_depends.push(dep.to_string());
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
        // 1. Create package directory structure
        // 2. Write +CONTENTS, +DESC, +COMMENT, +MTREE
        // 3. Create tarball
        // 4. Sign if needed

        Ok(())
    }
}

/// Create an OpenBSD pkg_add package.
pub fn create_pkg_add(
    app_name: &str,
    version: &str,
    _binary_path: &Path,
    _output_dir: &Path,
) -> Result<(), Fault> {
    let _pkg = OpenbsdPkg::new(app_name, version)
        .comment("Goya application")
        .desc("Native BSD application compiled with Diablos")
        .maintainer("Goya Diablos <diablos@goya.example>")
        .homepage("https://goya.example")
        .license("MIT")
        .run_depend("libc");

    // Placeholder for Phase 0
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkg_add_basic() {
        let pkg = OpenbsdPkg::new("test", "1.0.0");
        assert_eq!(pkg.name, "test");
        assert_eq!(pkg.version, "1.0.0");
    }
}