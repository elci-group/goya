//! FreeBSD pkg format packaging.

use crate::Fault;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// FreeBSD pkg manifest (+MANIFEST).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreebsdPkgManifest {
    /// Package name
    pub name: String,
    /// Version
    pub version: String,
    /// Origin (category/port)
    pub origin: String,
    /// Comment
    pub comment: String,
    /// Description
    pub desc: String,
    /// Maintainer
    pub maintainer: String,
    /// Website
    pub www: String,
    /// License
    pub licenses: Vec<String>,
    /// Architectures
    pub arch: Vec<String>,
    /// Dependencies
    pub deps: Vec<PkgDep>,
    /// Files
    pub files: Vec<PkgFile>,
    /// Directories
    pub dirs: Vec<String>,
    /// Scripts
    pub scripts: PkgScripts,
    /// Options
    pub options: Vec<String>,
}

/// Package dependency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgDep {
    pub origin: String,
    pub version: String,
    pub name: String,
}

/// Package file entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkgFile {
    pub path: String,
    pub sum: String, // SHA256
    pub perm: Option<String>,
    pub uname: Option<String>,
    pub gname: Option<String>,
}

/// Package scripts.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PkgScripts {
    pub pre_install: Option<String>,
    pub post_install: Option<String>,
    pub pre_deinstall: Option<String>,
    pub post_deinstall: Option<String>,
    pub pre_upgrade: Option<String>,
    pub post_upgrade: Option<String>,
}

/// FreeBSD package builder.
pub struct FreebsdPkg {
    manifest: FreebsdPkgManifest,
    staging_dir: String,
}

impl FreebsdPkg {
    /// Create a new package builder.
    pub fn new(name: &str, version: &str, origin: &str) -> Self {
        Self {
            manifest: FreebsdPkgManifest {
                name: name.to_string(),
                version: version.to_string(),
                origin: origin.to_string(),
                comment: String::new(),
                desc: String::new(),
                maintainer: String::new(),
                www: String::new(),
                licenses: Vec::new(),
                arch: vec!["freebsd:*".to_string()],
                deps: Vec::new(),
                files: Vec::new(),
                dirs: Vec::new(),
                scripts: PkgScripts::default(),
                options: Vec::new(),
            },
            staging_dir: format!("/tmp/pkg-{}-{}", name, version),
        }
    }

    /// Set comment.
    pub fn comment(mut self, comment: &str) -> Self {
        self.manifest.comment = comment.to_string();
        self
    }

    /// Set description.
    pub fn desc(mut self, desc: &str) -> Self {
        self.manifest.desc = desc.to_string();
        self
    }

    /// Set maintainer.
    pub fn maintainer(mut self, maintainer: &str) -> Self {
        self.manifest.maintainer = maintainer.to_string();
        self
    }

    /// Set website.
    pub fn www(mut self, www: &str) -> Self {
        self.manifest.www = www.to_string();
        self
    }

    /// Add license.
    pub fn license(mut self, license: &str) -> Self {
        self.manifest.licenses.push(license.to_string());
        self
    }

    /// Add dependency.
    pub fn dep(mut self, origin: &str, version: &str, name: &str) -> Self {
        self.manifest.deps.push(PkgDep {
            origin: origin.to_string(),
            version: version.to_string(),
            name: name.to_string(),
        });
        self
    }

    /// Add file.
    pub fn file(mut self, path: &str, sum: &str, perm: Option<&str>) -> Self {
        self.manifest.files.push(PkgFile {
            path: path.to_string(),
            sum: sum.to_string(),
            perm: perm.map(String::from),
            uname: None,
            gname: None,
        });
        self
    }

    /// Add directory.
    pub fn dir(mut self, path: &str) -> Self {
        self.manifest.dirs.push(path.to_string());
        self
    }

    /// Set staging directory.
    pub fn staging_dir(mut self, dir: &str) -> Self {
        self.staging_dir = dir.to_string();
        self
    }

    /// Build the package (creates .pkg file).
    pub fn build(self, output: &Path) -> Result<(), Fault> {
        // In real implementation:
        // 1. Create staging directory structure
        // 2. Copy files to staging
        // 3. Write +MANIFEST
        // 4. Run `pkg create -M staging_dir -o output_dir`
        // 5. Clean up staging

        // For Phase 0, just write a placeholder manifest
        let manifest_json = serde_json::to_vec_pretty(&self.manifest)
            .map_err(|e| Fault::new(crate::fault::exit::VERIFICATION_FAILED, "PKG_MANIFEST_JSON_FAILED", e.to_string()))?;

        std::fs::write(output.with_extension("manifest.json"), manifest_json)
            .map_err(|e| Fault::new(crate::fault::exit::VERIFICATION_FAILED, "PKG_MANIFEST_WRITE_FAILED", e.to_string()))?;

        Ok(())
    }
}

/// Create a FreeBSD pkg from a compiled application.
pub fn create_pkg(
    app_name: &str,
    version: &str,
    binary_path: &Path,
    output_dir: &Path,
) -> Result<(), Fault> {
    let pkg = FreebsdPkg::new(app_name, version, &format!("goya/{}", app_name))
        .comment("Goya application")
        .desc("Native BSD application compiled with Diablos")
        .maintainer("Goya Diablos <diablos@goya.example>")
        .www("https://goya.example")
        .license("MIT")
        .dep("origin", "version", "libc")
        .file(&format!("bin/{}", app_name), "sha256placeholder", Some("0755"));

    pkg.build(&output_dir.join(format!("{}-{}.pkg", app_name, version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkg_manifest_basic() {
        let pkg = FreebsdPkg::new("test", "1.0.0", "goya/test");
        assert_eq!(pkg.manifest.name, "test");
        assert_eq!(pkg.manifest.version, "1.0.0");
        assert_eq!(pkg.manifest.origin, "goya/test");
    }

    #[test]
    fn pkg_builder_chaining() {
        let pkg = FreebsdPkg::new("test", "1.0.0", "goya/test")
            .comment("Test")
            .dep("origin", "1.0", "libc");
        assert_eq!(pkg.manifest.comment, "Test");
        assert_eq!(pkg.manifest.deps.len(), 1);
    }
}