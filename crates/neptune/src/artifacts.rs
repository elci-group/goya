//! Integrity checks for the experimental source-preview artifact set.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const FILES: [&str; 3] = ["App.swift", "source-map.json", "capabilities.json"];

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
    for name in FILES {
        let expected = hashes
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| io::Error::other("missing artifact hash"))?;
        let actual = format!("{:x}", Sha256::digest(read_bounded(&directory.join(name))?));
        if actual != expected {
            return Err(io::Error::other(format!("artifact hash mismatch: {name}")));
        }
    }
    Ok(())
}
