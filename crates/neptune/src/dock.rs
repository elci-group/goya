//! Neptune Dock deployment contracts. No device transport or signing backend
//! is admitted by this module yet; inputs are validated before orchestration.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    io::Read,
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
        if path.as_os_str().is_empty()
            || path.is_absolute()
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
    pub fn verify_artifact_reader(&self, mut reader: impl Read) -> Result<(), Error> {
        self.validate()?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(count) => count,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(Error::ArtifactMissing),
            };
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        if format!("{:x}", hash.finalize()) == self.artifact.sha256 {
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
    let numeric = |part: &str| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit());
    numeric(a)
        && numeric(b)
        && match it.next() {
            Some(patch) => numeric(patch) && it.next().is_none(),
            None => true,
        }
}

#[cfg(test)]
mod tests {
    fn manifest() -> super::Manifest {
        serde_json::from_value(serde_json::json!({
            "schema": super::MANIFEST_SCHEMA,
            "application": {"name":"App", "bundle_id":"com.example.app", "version":"1.0", "build":1},
            "target": {"platform":"ios", "architecture":"arm64", "minimum_os":"17.0"},
            "artifact": {"kind":"ipa", "path":"App.ipa", "sha256":format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(b"ipa"))},
            "signing": {"mode":"development", "identity":"Developer", "profile":"profile"},
            "deployment": {"transport":"usb", "preserve_data":true, "launch_after_install":false}
        })).unwrap()
    }

    #[test]
    fn artifact_path_must_name_a_relative_file() {
        let mut manifest = manifest();
        assert_eq!(manifest.validate(), Ok(()));
        for path in ["", ".", "../App.ipa", "/App.ipa"] {
            manifest.artifact.path = path.into();
            assert_eq!(
                manifest.validate(),
                Err(super::Error::ArtifactPath),
                "{path}"
            );
        }
    }

    #[test]
    fn artifact_reader_retries_interrupted_reads() {
        struct InterruptedOnce(bool, std::io::Cursor<&'static [u8]>);
        impl std::io::Read for InterruptedOnce {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                std::io::Read::read(&mut self.1, buffer)
            }
        }
        assert_eq!(
            manifest().verify_artifact_reader(InterruptedOnce(false, std::io::Cursor::new(b"ipa"))),
            Ok(())
        );
    }

    #[test]
    fn minimum_os_requires_two_or_three_nonempty_numeric_components() {
        for valid in ["17.0", "18.1.2"] {
            assert!(super::version(valid), "{valid}");
        }
        for invalid in ["", ".", ".18", "18.", "18", "18.0.", "18.0.1.2", "18.x"] {
            assert!(!super::version(invalid), "{invalid}");
        }
    }
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
