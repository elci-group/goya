//! Neptune Dock deployment contracts. No device transport or signing backend
//! is admitted by this module yet; inputs are validated before orchestration.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    path::{Component, Path},
};

pub const MANIFEST_SCHEMA: &str = "goya.neptune.deploy/0.0.1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub application: Application,
    pub target: Target,
    pub artifact: Artifact,
    pub signing: Signing,
    pub deployment: Deployment,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Application {
    pub name: String,
    pub bundle_id: String,
    pub version: String,
    pub build: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub platform: String,
    pub architecture: String,
    pub minimum_os: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub kind: String,
    pub path: String,
    pub sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signing {
    pub mode: String,
    pub identity: String,
    pub profile: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    pub transport: String,
    pub preserve_data: bool,
    pub launch_after_install: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Schema,
    Field(&'static str),
    ArtifactPath,
    ArtifactHash,
    ArtifactMissing,
    ArtifactDigest,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

impl Manifest {
    /// Fail closed on unsupported targets, malformed identifiers and unsafe
    /// artifact paths. The digest is checked against actual artifact bytes.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != MANIFEST_SCHEMA {
            return Err(Error::Schema);
        }
        if self.application.name.trim().is_empty()
            || self.application.version.is_empty()
            || self.application.build == 0
        {
            return Err(Error::Field("application"));
        }
        let parts: Vec<_> = self.application.bundle_id.split('.').collect();
        if parts.len() < 2
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        {
            return Err(Error::Field("bundle_id"));
        }
        if self.target.platform != "ios"
            || self.target.architecture != "arm64"
            || !version(&self.target.minimum_os)
        {
            return Err(Error::Field("target"));
        }
        if self.artifact.kind != "ipa"
            || self.artifact.sha256.len() != 64
            || !self
                .artifact
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(Error::Field("artifact"));
        }
        let path = Path::new(&self.artifact.path);
        if path.is_absolute()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::ArtifactPath);
        }
        if self.signing.mode != "development"
            || self.signing.identity.trim().is_empty()
            || self.signing.profile.trim().is_empty()
        {
            return Err(Error::Field("signing"));
        }
        if self.deployment.transport != "usb" {
            return Err(Error::Field("transport"));
        }
        Ok(())
    }
    pub fn verify_artifact(&self, bytes: &[u8]) -> Result<(), Error> {
        self.validate()?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual == self.artifact.sha256 {
            Ok(())
        } else {
            Err(Error::ArtifactDigest)
        }
    }
}
fn version(value: &str) -> bool {
    let mut it = value.split('.');
    let Some(a) = it.next() else { return false };
    let Some(b) = it.next() else { return false };
    a.bytes().all(|c| c.is_ascii_digit())
        && b.bytes().all(|c| c.is_ascii_digit())
        && it.all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stage {
    Discovered,
    Paired,
    Ready,
    Provisioned,
    Signed,
    Staged,
    Installed,
    Verified,
    Launched,
}
impl Stage {
    pub fn can_advance_to(self, next: Self) -> bool {
        use Stage::*;
        matches!(
            (self, next),
            (Discovered, Paired)
                | (Paired, Ready)
                | (Ready, Provisioned)
                | (Provisioned, Signed)
                | (Signed, Staged)
                | (Staged, Installed)
                | (Installed, Verified)
                | (Verified, Launched)
        )
    }
}

/// Evidence must be appended in legal order; a failed transition never
/// mutates the recorded lifecycle.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub stages: Vec<Stage>,
}
impl Progress {
    pub fn advance(&mut self, next: Stage) -> Result<(), Error> {
        match self.stages.last().copied() {
            None if next == Stage::Discovered => self.stages.push(next),
            Some(current) if current.can_advance_to(next) => self.stages.push(next),
            _ => return Err(Error::Field("lifecycle_transition")),
        }
        Ok(())
    }
}
