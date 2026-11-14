//! Integrity checks for the experimental source-preview artifact set.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const FILES: [&str; 3] = ["App.swift", "source-map.json", "capabilities.json"];

/// Artifact bytes and source identity checked together against one report.
pub struct VerifiedPreview {
    pub artifacts: std::collections::BTreeMap<String, Vec<u8>>,
    pub hashes: std::collections::BTreeMap<String, String>,
    pub input_sha256: String,
}

fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(io::Error::other(
            "artifact must be a regular file no larger than 1 MiB",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(io::Error::other("artifact exceeds 1 MiB"));
    }
    Ok(bytes)
}

/// Verify hashes in a source-preview report. This is an integrity check, not
/// an authenticity check or evidence that Apple tools compiled the source.
pub fn verify_preview(directory: &Path) -> io::Result<()> {
    verified_preview_hashes(directory).map(|_| ())
}

/// Returns hashes computed from the artifact bytes that were checked.
pub fn verified_preview_hashes(
    directory: &Path,
) -> io::Result<std::collections::BTreeMap<String, String>> {
    load_verified_preview(directory).map(|preview| preview.hashes)
}

/// Read each bounded artifact once and verify the exact returned bytes.
pub fn load_verified_preview(directory: &Path) -> io::Result<VerifiedPreview> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::other("preview path must be a directory"));
    }
    let report: serde_json::Value =
        serde_json::from_slice(&read_bounded(&directory.join("build-report.json"))?)
            .map_err(io::Error::other)?;
    if report["schema_version"] != "1"
        || report["stages"]["ios_admission"] != "NOT_RUN"
        || report["stages"]["apple_validation"] != "NOT_RUN"
    {
        return Err(io::Error::other(
            "unsupported report schema or validation status",
        ));
    }
    let hashes = report["artifact_sha256"]
        .as_object()
        .ok_or_else(|| io::Error::other("missing artifact hashes"))?;
    if hashes.len() != FILES.len() {
        return Err(io::Error::other("unexpected artifact hash set"));
    }
    let input_sha256 = report["input_sha256"]
        .as_str()
        .filter(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .ok_or_else(|| io::Error::other("invalid source hash"))?
        .to_owned();
    let mut verified = std::collections::BTreeMap::new();
    let mut artifacts = std::collections::BTreeMap::new();
    for name in FILES {
        let expected = hashes
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| io::Error::other("missing artifact hash"))?;
        let bytes = read_bounded(&directory.join(name))?;
        let actual = format!("{:x}", Sha256::digest(&bytes));
        if actual != expected {
            return Err(io::Error::other(format!("artifact hash mismatch: {name}")));
        }
        verified.insert(name.to_owned(), actual);
        artifacts.insert(name.to_owned(), bytes);
    }
    Ok(VerifiedPreview {
        artifacts,
        hashes: verified,
        input_sha256,
    })
}
