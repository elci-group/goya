//! Source-preview handoff plan for a future isolated Apple worker.
//! Constructing this plan never runs Apple tools and never implies build success.
use serde_json::{json, Value};
use std::path::Path;

/// Required normalized toolchain identities, supplied by the project owner.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainLock {
    pub schema_version: String,
    pub xcode_version: String,
    pub xcode_build: String,
    pub swift_version: String,
    pub sdk_version: String,
    pub sdk_build: String,
    pub minimum_os: String,
}

fn version(text: &str, key: &str) -> Result<(String, [u32; 3]), String> {
    if text.len() > 32 {
        return Err(format!("invalid {key}"));
    }
    let parts: Vec<_> = text.split('.').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(format!("invalid {key}"));
    }
    let mut numbers = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty()
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(format!("invalid {key}"));
        }
        numbers[i] = part.parse().map_err(|_| format!("invalid {key}"))?;
    }
    Ok((text.to_owned(), numbers))
}

fn build_id(text: &str, key: &str) -> Result<String, String> {
    if text.is_empty() || text.len() > 32 || !text.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(format!("invalid {key}"));
    }
    Ok(text.to_owned())
}

/// Plan type checking for the static source preview with an explicit lock.
/// The returned job is BLOCKED until a worker executes and reports the result.
pub fn plan_simulator_typecheck(directory: &Path, lock: &ToolchainLock) -> Result<Value, String> {
    if lock.schema_version != "1" {
        return Err("unsupported toolchain lock schema".into());
    }
    let (xcode_version, _) = version(&lock.xcode_version, "xcode_version")?;
    let xcode_build = build_id(&lock.xcode_build, "xcode_build")?;
    let (swift_version, _) = version(&lock.swift_version, "swift_version")?;
    let (sdk_version, sdk) = version(&lock.sdk_version, "sdk_version")?;
    let sdk_build = build_id(&lock.sdk_build, "sdk_build")?;
    let (minimum_os, minimum) = version(&lock.minimum_os, "minimum_os")?;
    if minimum > sdk {
        return Err("minimum_os exceeds the locked SDK version".into());
    }
    let hashes = crate::artifacts::verified_preview_hashes(directory)
        .map_err(|e| format!("preview integrity check failed: {e}"))?;
    let target = format!("arm64-apple-ios{minimum_os}-simulator");
    Ok(json!({
        "schema_version":"1", "operation":"apple-simulator-typecheck-plan", "status":"BLOCKED",
        "reason":"An authorized Mac worker has not executed this plan.",
        "verification":{"artifact_integrity":"PASS", "artifact_authenticity":"NOT_CHECKED", "ios_admission":"NOT_RUN", "apple_typecheck":"BLOCKED", "simulator_behavior":"BLOCKED", "physical_device":"BLOCKED"},
        "toolchain_lock":{"schema_version":"1", "xcode_version":xcode_version,"xcode_build":xcode_build,"swift_version":swift_version,"sdk":"iphonesimulator","sdk_version":sdk_version,"sdk_build":sdk_build,"minimum_os":minimum_os},
        "artifact_sha256":hashes,
        "invocation":{"executable":"/usr/bin/xcrun", "arguments":["--sdk","iphonesimulator","swiftc","-target",target,"-typecheck","-parse-as-library","-strict-concurrency=complete"], "sdk_root":"worker_discovered_and_lock_verified", "source_artifact":"App.swift"},
        "worker_requirements":{"isolation":"per_job", "network":"disabled_after_dependency_preparation", "project_hooks":"prohibited", "signing":"not_requested", "sdk_path":"resolve_with_xcrun_and_verify_lock", "source":"copy_verified_bytes_into_isolated_workspace"}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_versions_and_shell_syntax() {
        assert!(version("17.0", "minimum_os").is_ok());
        for bad in ["17.0;touch", "../17.0", "017.0", "17", "17.0.0.1", "17.0\n"] {
            assert!(version(bad, "minimum_os").is_err());
        }
        assert!(build_id("bad;sh", "xcode_build").is_err());
        let duplicate = br#"{"schema_version":"1","schema_version":"2","xcode_version":"16.2","xcode_build":"A","swift_version":"6.0","sdk_version":"18.0","sdk_build":"B","minimum_os":"17.0"}"#;
        assert!(serde_json::from_slice::<ToolchainLock>(duplicate).is_err());
    }
}
