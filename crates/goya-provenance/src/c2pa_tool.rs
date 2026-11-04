//! Standards-based adapter to the official `c2patool` executable.
//!
//! Goya's compact Ed25519 claim envelope is not a C2PA manifest. This adapter
//! sends an asset and a Goya custom assertion to `c2patool`, which builds the
//! JUMBF manifest store, adds the asset hard binding, signs the claim and
//! embeds the result in the selected media format. Signing is delegated to an
//! explicit subprocess signer so private key material does not enter Goya or
//! the generated manifest definition.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{KeyRole, ManifestClaims};

const GOYA_ASSERTION_LABEL: &str = "org.goya.provenance.claims.v1";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Signing algorithms accepted by official c2patool releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum C2paSigningAlgorithm {
    Ps256,
    Ps384,
    Ps512,
    Es256,
    Es384,
    Es512,
    Ed25519,
}

impl C2paSigningAlgorithm {
    fn as_c2pa_str(self) -> &'static str {
        match self {
            Self::Ps256 => "ps256",
            Self::Ps384 => "ps384",
            Self::Ps512 => "ps512",
            Self::Es256 => "es256",
            Self::Es384 => "es384",
            Self::Es512 => "es512",
            Self::Ed25519 => "ed25519",
        }
    }
}

/// A successful `c2patool` operation and its generated file, when signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct C2paToolReport {
    /// Raw tool output; callers may parse it using their preferred JSON stack.
    pub stdout: String,
    pub stderr: String,
    /// Present for signing, absent when inspecting an asset.
    pub output: Option<PathBuf>,
    /// Size of the signed output in bytes.
    pub output_bytes: Option<u64>,
}

/// Errors from the official C2PA command adapter.
#[derive(Debug)]
pub enum C2paToolError {
    InvalidAsset(PathBuf),
    InvalidOutput(PathBuf),
    InvalidSigner(PathBuf),
    InvalidCertificate(PathBuf),
    ToolUnavailable(PathBuf),
    Launch(io::Error),
    ToolFailed {
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    MissingOutput(PathBuf),
    TemporaryManifest(io::Error),
}

impl fmt::Display for C2paToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAsset(path) => {
                write!(f, "asset is not a readable file: {}", path.display())
            }
            Self::InvalidOutput(path) => write!(f, "invalid C2PA output path: {}", path.display()),
            Self::InvalidSigner(path) => {
                write!(f, "signer executable is not a file: {}", path.display())
            }
            Self::InvalidCertificate(path) => {
                write!(
                    f,
                    "signer certificate is not a UTF-8 PEM file: {}",
                    path.display()
                )
            }
            Self::ToolUnavailable(path) => {
                write!(f, "C2PA tool is unavailable: {}", path.display())
            }
            Self::Launch(error) => write!(f, "could not launch c2patool: {error}"),
            Self::ToolFailed { status, stderr, .. } => {
                write!(f, "c2patool failed (status {status:?}): {}", stderr.trim())
            }
            Self::MissingOutput(path) => write!(
                f,
                "c2patool reported success without a non-empty output at {}",
                path.display()
            ),
            Self::TemporaryManifest(error) => write!(
                f,
                "could not create temporary C2PA manifest definition: {error}"
            ),
        }
    }
}

impl std::error::Error for C2paToolError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Launch(error) | Self::TemporaryManifest(error) => Some(error),
            _ => None,
        }
    }
}

/// Adapter to the official C2PA command-line implementation.
///
/// `c2patool` must be installed separately. The default executable is resolved
/// via `PATH`; tests and controlled installations can provide an explicit path.
#[derive(Debug, Clone)]
pub struct C2paTool {
    executable: PathBuf,
}

impl Default for C2paTool {
    fn default() -> Self {
        Self::new()
    }
}

impl C2paTool {
    /// Use `c2patool` from the current process `PATH`.
    pub fn new() -> Self {
        Self {
            executable: PathBuf::from("c2patool"),
        }
    }

    /// Use an explicit official `c2patool` executable.
    pub fn with_executable(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    /// Create and embed a C2PA manifest carrying `claims` as a Goya custom
    /// assertion. A trusted signer executable, its PEM certificate-chain
    /// path, and algorithm are mandatory; the tool's built-in test
    /// certificate is never selected by this API.
    ///
    /// The source and destination must be distinct files. Existing output is
    /// rejected unless `overwrite` is true. The official tool handles media
    /// format parsing, JUMBF serialization, content binding and COSE signing.
    pub fn sign_asset(
        &self,
        asset: impl AsRef<Path>,
        output: impl AsRef<Path>,
        signer: impl AsRef<Path>,
        signer_certificate: impl AsRef<Path>,
        algorithm: C2paSigningAlgorithm,
        claims: &ManifestClaims,
        overwrite: bool,
    ) -> Result<C2paToolReport, C2paToolError> {
        let asset = asset.as_ref();
        let output = output.as_ref();
        let signer = signer.as_ref();
        let signer_certificate = signer_certificate.as_ref();
        validate_asset(asset)?;
        validate_signer(signer)?;
        validate_certificate(signer_certificate)?;
        validate_output(asset, output, overwrite)?;

        let certificate_path = signer_certificate
            .to_str()
            .ok_or_else(|| C2paToolError::InvalidCertificate(signer_certificate.into()))?;
        let manifest = TemporaryManifest::create(claims, certificate_path, algorithm)
            .map_err(C2paToolError::TemporaryManifest)?;
        let mut command = Command::new(&self.executable);
        command
            .arg(asset)
            .arg("--manifest")
            .arg(&manifest.path)
            .arg("--output")
            .arg(output)
            .arg("--signer-path")
            .arg(signer);
        if overwrite {
            command.arg("--force");
        }
        let result = self.run(command)?;
        let metadata =
            fs::metadata(output).map_err(|_| C2paToolError::MissingOutput(output.into()))?;
        if !metadata.is_file() || metadata.len() == 0 {
            return Err(C2paToolError::MissingOutput(output.into()));
        }
        Ok(report(result, Some(output.into()), Some(metadata.len())))
    }

    /// Ask `c2patool` to read and report an asset's embedded manifest.
    ///
    /// The raw report is returned without interpreting trust policy. A zero
    /// process status means the tool read the asset; it does not mean the
    /// signer chains to a caller-approved trust anchor. Use the official
    /// tool's trust options/policy for that distinction.
    pub fn inspect_asset(&self, asset: impl AsRef<Path>) -> Result<C2paToolReport, C2paToolError> {
        let asset = asset.as_ref();
        validate_asset(asset)?;
        let mut command = Command::new(&self.executable);
        command.arg(asset);
        let output = self.run(command)?;
        Ok(report(output, None, None))
    }

    fn run(&self, mut command: Command) -> Result<Output, C2paToolError> {
        let output = command.output().map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                C2paToolError::ToolUnavailable(self.executable.clone())
            } else {
                C2paToolError::Launch(error)
            }
        })?;
        if !output.status.success() {
            return Err(C2paToolError::ToolFailed {
                status: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }
        Ok(output)
    }
}

fn validate_asset(path: &Path) -> Result<(), C2paToolError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 => Ok(()),
        _ => Err(C2paToolError::InvalidAsset(path.into())),
    }
}

fn validate_signer(path: &Path) -> Result<(), C2paToolError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        _ => Err(C2paToolError::InvalidSigner(path.into())),
    }
}

fn validate_certificate(path: &Path) -> Result<(), C2paToolError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > 0 && path.to_str().is_some() => {
            Ok(())
        }
        _ => Err(C2paToolError::InvalidCertificate(path.into())),
    }
}

fn validate_output(asset: &Path, output: &Path, overwrite: bool) -> Result<(), C2paToolError> {
    if output.as_os_str().is_empty() {
        return Err(C2paToolError::InvalidOutput(output.into()));
    }
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(C2paToolError::InvalidOutput(output.into()));
    }
    let output_name = output
        .file_name()
        .ok_or_else(|| C2paToolError::InvalidOutput(output.into()))?;
    if let (Ok(asset_real), Ok(parent_real)) = (fs::canonicalize(asset), fs::canonicalize(parent)) {
        if asset_real == parent_real.join(output_name) {
            return Err(C2paToolError::InvalidOutput(output.into()));
        }
    } else if asset == output {
        return Err(C2paToolError::InvalidOutput(output.into()));
    }
    if output.exists() && !overwrite {
        return Err(C2paToolError::InvalidOutput(output.into()));
    }
    Ok(())
}

fn report(output: Output, path: Option<PathBuf>, bytes: Option<u64>) -> C2paToolReport {
    C2paToolReport {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        output: path,
        output_bytes: bytes,
    }
}

struct TemporaryManifest {
    path: PathBuf,
}

impl TemporaryManifest {
    fn create(
        claims: &ManifestClaims,
        certificate_path: &str,
        algorithm: C2paSigningAlgorithm,
    ) -> io::Result<Self> {
        let json = manifest_definition(claims, certificate_path, algorithm);
        for _ in 0..32 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("goya-c2pa-{}-{sequence}.json", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(json.as_bytes()) {
                        let _ = fs::remove_file(&path);
                        return Err(error);
                    }
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "temporary manifest name space exhausted",
        ))
    }
}

impl Drop for TemporaryManifest {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn manifest_definition(
    claims: &ManifestClaims,
    certificate_path: &str,
    algorithm: C2paSigningAlgorithm,
) -> String {
    let mut json = String::from("{\"claim_generator\":\"Goya\",\"alg\":");
    push_json_string(&mut json, algorithm.as_c2pa_str());
    json.push_str(",\"sign_cert\":");
    push_json_string(&mut json, certificate_path);
    json.push_str(",\"assertions\":[{\"label\":");
    push_json_string(&mut json, GOYA_ASSERTION_LABEL);
    json.push_str(",\"data\":{\"issuer\":");
    push_json_string(&mut json, &claims.issuer);
    json.push_str(",\"timestamp_ns\":");
    json.push_str(&claims.timestamp_ns.to_string());
    // Preserved as Goya metadata only; c2patool creates the actual
    // format-specific hard-binding assertion over the asset bytes.
    json.push_str(",\"goya_identifier\":{\"algorithm\":");
    push_json_string(&mut json, &claims.algorithm);
    json.push_str(",\"value\":");
    push_json_string(&mut json, &format!("0x{:016x}", claims.artifact_hash));
    json.push_str("},\"key_role\":");
    push_json_string(
        &mut json,
        match claims.key_role {
            KeyRole::Release => "release",
            KeyRole::Development => "development",
        },
    );
    json.push_str(",\"assertions\":[");
    for (index, assertion) in claims.assertions.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        push_json_string(&mut json, assertion);
    }
    json.push_str("]}}]}");
    json
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch < '\u{20}' => {
                use fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    struct FakeTool(PathBuf);

    #[cfg(unix)]
    impl FakeTool {
        fn new(body: &str) -> Self {
            use std::os::unix::fs::PermissionsExt;
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("goya-c2pa-fake-{}-{stamp}", std::process::id()));
            let script = format!("#!/bin/sh\n{body}\n");
            fs::write(&path, script).unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&path, permissions).unwrap();
            Self(path)
        }
    }

    #[cfg(unix)]
    impl Drop for FakeTool {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn unique_temp(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("goya-c2pa-{label}-{}-{stamp}", std::process::id()))
    }

    fn test_claims() -> ManifestClaims {
        ManifestClaims {
            artifact_hash: 0xdead_beef,
            algorithm: "fnv1a64".into(),
            assertions: vec!["a \"quoted\" assertion".into(), "line\nbreak".into()],
            issuer: "Goya test".into(),
            timestamp_ns: 7,
            key_role: KeyRole::Development,
        }
    }

    #[test]
    fn manifest_definition_is_json_escaped_and_uses_custom_assertion() {
        let definition = manifest_definition(
            &test_claims(),
            "/tmp/signer-chain.pem",
            C2paSigningAlgorithm::Es256,
        );
        assert!(definition.contains("org.goya.provenance.claims.v1"));
        assert!(definition.contains("a \\\"quoted\\\" assertion"));
        assert!(definition.contains("line\\nbreak"));
        assert!(definition.contains("0x00000000deadbeef"));
        assert!(definition.contains("\"alg\":\"es256\""));
        assert!(definition.contains("\"sign_cert\":\"/tmp/signer-chain.pem\""));
    }

    #[cfg(unix)]
    #[test]
    fn signs_through_c2patool_without_a_shell_and_checks_output() {
        let dir = unique_temp("sign");
        fs::create_dir(&dir).unwrap();
        let asset = dir.join("source.bin");
        let output = dir.join("signed.bin");
        let signer = dir.join("signer");
        let certificate = dir.join("signer-chain.pem");
        fs::write(&asset, b"asset").unwrap();
        fs::write(&signer, b"signer executable placeholder").unwrap();
        fs::write(
            &certificate,
            b"-----BEGIN CERTIFICATE-----\nplaceholder\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        let tool = FakeTool::new(
            "out=\"\"\nmanifest=\"\"\nprev=\"\"\nfor arg in \"$@\"; do if [ \"$prev\" = --output ]; then out=\"$arg\"; fi; if [ \"$prev\" = --manifest ]; then manifest=\"$arg\"; fi; prev=\"$arg\"; done\nprintf signed > \"$out\"\ncat \"$manifest\"\nprintf '%s\\n' \"$@\"",
        );
        let result = C2paTool::with_executable(&tool.0)
            .sign_asset(
                &asset,
                &output,
                &signer,
                &certificate,
                C2paSigningAlgorithm::Es256,
                &test_claims(),
                false,
            )
            .unwrap();
        assert_eq!(result.output_bytes, Some(6));
        assert!(result.stdout.contains("--manifest"));
        let args = &result.stdout;
        assert!(args.contains("\"alg\":\"es256\""));
        assert!(args.contains(&format!("\"sign_cert\":\"{}\"", certificate.display())));
        assert!(args.contains("--signer-path"));
        assert!(!args.contains("--force"));
        assert!(!output.exists() || fs::metadata(output).unwrap().len() > 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_overwrite_asset_without_explicit_permission() {
        let dir = unique_temp("overwrite");
        fs::create_dir(&dir).unwrap();
        let asset = dir.join("same.bin");
        let signer = dir.join("signer");
        let certificate = dir.join("signer-chain.pem");
        fs::write(&asset, b"asset").unwrap();
        fs::write(&signer, b"signer").unwrap();
        fs::write(&certificate, b"certificate").unwrap();
        let tool = FakeTool::new("exit 0");
        assert!(matches!(
            C2paTool::with_executable(&tool.0).sign_asset(
                &asset,
                &asset,
                &signer,
                &certificate,
                C2paSigningAlgorithm::Es256,
                &test_claims(),
                true
            ),
            Err(C2paToolError::InvalidOutput(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn absent_official_tool_is_reported() {
        let tool = C2paTool::with_executable(unique_temp("missing-tool"));
        let asset = unique_temp("input");
        fs::write(&asset, b"asset").unwrap();
        assert!(matches!(
            tool.inspect_asset(&asset),
            Err(C2paToolError::ToolUnavailable(_))
        ));
        fs::remove_file(asset).unwrap();
    }
}
