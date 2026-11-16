//! Integrity checks for the experimental source-preview artifact set.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const FILES: [&str; 5] = [
    "App.swift",
    "source-map.json",
    "capabilities.json",
    "AndroidInputPreview.kt",
    "AndroidD1Runtime.kt",
];

/// Artifact bytes and source identity checked together against one report.
pub struct VerifiedPreview {
    pub(crate) artifacts: std::collections::BTreeMap<String, Vec<u8>>,
    pub(crate) hashes: std::collections::BTreeMap<String, String>,
    pub(crate) input_sha256: String,
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
    let mut artifacts = std::collections::BTreeMap::new();
    for name in FILES {
        artifacts.insert(name.to_owned(), read_bounded(&directory.join(name))?);
    }
    verify_parts(&report, artifacts)
}

/// Verify the same preview format when provided inline to the local service.
pub fn verify_preview_bundle(bundle: &serde_json::Value) -> io::Result<VerifiedPreview> {
    if bundle["schema_version"] != "2"
        || bundle["operation"] != "preview"
        || bundle["verification"] != "SOURCE_PREVIEW"
    {
        return Err(io::Error::other("unsupported preview bundle"));
    }
    let values = bundle["artifacts"]
        .as_object()
        .ok_or_else(|| io::Error::other("missing artifacts"))?;
    if values.len() != FILES.len() {
        return Err(io::Error::other("unexpected artifact set"));
    }
    let mut artifacts = std::collections::BTreeMap::new();
    for name in FILES {
        let value = values
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| io::Error::other("missing artifact"))?;
        if value.len() as u64 > MAX_FILE_BYTES {
            return Err(io::Error::other("artifact exceeds 1 MiB"));
        }
        artifacts.insert(name.to_owned(), value.as_bytes().to_vec());
    }
    verify_parts(&bundle["report"], artifacts)
}

fn verify_parts(
    report: &serde_json::Value,
    artifacts: std::collections::BTreeMap<String, Vec<u8>>,
) -> io::Result<VerifiedPreview> {
    if report["schema_version"] != "1"
        || report["stages"]["goya_frontend"] != "PASS"
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
    for name in FILES {
        let expected = hashes
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| io::Error::other("missing artifact hash"))?;
        let bytes = artifacts
            .get(name)
            .ok_or_else(|| io::Error::other("missing artifact"))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != expected {
            return Err(io::Error::other(format!("artifact hash mismatch: {name}")));
        }
        verified.insert(name.to_owned(), actual);
    }
    Ok(VerifiedPreview {
        artifacts,
        hashes: verified,
        input_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn bundle() -> Value {
        let artifacts = json!({
            "App.swift": "import SwiftUI\n",
            "source-map.json": "{}",
            "capabilities.json": "{}",
            "AndroidInputPreview.kt": "// Enhanced Android input preview\nclass GoyaAndroidInputPreview { }",
            "AndroidD1Runtime.kt": "// Enhanced Android D1 runtime\nclass GoyaAndroidD1Runtime { }"
        });
        let hashes: serde_json::Map<String, Value> = artifacts
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    json!(format!(
                        "{:x}",
                        Sha256::digest(value.as_str().unwrap().as_bytes())
                    )),
                )
            })
            .collect();
        json!({
            "schema_version":"2", "operation":"preview", "verification":"SOURCE_PREVIEW",
            "artifacts": artifacts,
            "report": {
                "schema_version":"1", "input_sha256":"0".repeat(64),
                "artifact_sha256": hashes,
                "stages":{"goya_frontend":"PASS", "ios_admission":"NOT_RUN", "apple_validation":"NOT_RUN"}
            }
        })
    }

    #[test]
    fn preview_requires_successful_frontend_and_unrun_platform_validation() {
        assert!(verify_preview_bundle(&bundle()).is_ok());
        for (stage, invalid) in [
            ("goya_frontend", Value::Null),
            ("goya_frontend", json!("FAIL")),
            ("goya_frontend", json!(true)),
            ("ios_admission", json!("PASS")),
            ("apple_validation", json!("PASS")),
        ] {
            let mut bundle = bundle();
            bundle["report"]["stages"][stage] = invalid;
            assert!(verify_preview_bundle(&bundle).is_err(), "accepted {stage}");
        }
    }

    #[test]
    fn bundle_rejects_tampered_artifacts_and_hash_sets() {
        let mut changed = bundle();
        changed["artifacts"]["App.swift"] = json!("changed");
        assert!(verify_preview_bundle(&changed).is_err());
        let mut changed = bundle();
        changed["report"]["artifact_sha256"]["extra"] = json!("0".repeat(64));
        assert!(verify_preview_bundle(&changed).is_err());
        let mut changed = bundle();
        changed["report"]["input_sha256"] = json!("Z".repeat(64));
        assert!(verify_preview_bundle(&changed).is_err());
    }
}
