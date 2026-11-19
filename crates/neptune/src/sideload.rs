//! Handoff to the open-i CLI for sideloading a built artifact onto a
//! physical device for manual testing.
//!
//! Neptune does not compile iOS applications; sideloading only applies to
//! artifacts a worker produced outside Neptune (for example a manually built
//! `.ipa`). Device admission is never claimed by this operation.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Locates the open-i executable. `NEPTUNE_OPEN_I_BIN` overrides the PATH
/// lookup so tests can supply a deterministic mock.
pub fn find_open_i() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("NEPTUNE_OPEN_I_BIN") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join("open-i");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Sideloads the artifact at `ipa` by invoking `open-i --json sideload <path>`.
///
/// The final JSON line on open-i's stdout must be `{"ok": true, ...}` for
/// success. Returns a machine-readable report value on success.
pub fn sideload(ipa: &Path) -> Result<Value, String> {
    if !ipa.exists() {
        return Err(format!("artifact not found: {}", ipa.display()));
    }

    let exe = find_open_i().ok_or_else(|| "open-i unavailable".to_string())?;

    let output = std::process::Command::new(&exe)
        .arg("--json")
        .arg("sideload")
        .arg(ipa)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("failed to run open-i: {e}"))?;

    // The last non-empty stdout line is the open-i result object.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let last_line = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "open-i produced no JSON result".to_string())?;

    let result: Value = serde_json::from_str(last_line)
        .map_err(|e| format!("open-i result was not valid JSON: {e}"))?;

    if !output.status.success() || result.get("ok") != Some(&Value::Bool(true)) {
        let message = result
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("open-i reported failure without a message");
        return Err(message.to_string());
    }

    Ok(serde_json::json!({
        "schema_version": "1",
        "operation": "sideload",
        "status": "PASS",
        "stages": {
            "goya_parse": "NOT_RUN",
            "ios_validation": "NOT_RUN",
            "apple_validation": "NOT_RUN",
            "device_install": "PASS",
        },
        "admission": "MANUAL_TEST_ONLY",
        "note": "installed for manual testing; device admission is not claimed",
    }))
}

/// Report emitted when the sideload handoff is unavailable.
pub fn blocked_report() -> Value {
    serde_json::json!({
        "schema_version": "1",
        "operation": "sideload",
        "status": "BLOCKED",
        "stages": {
            "goya_parse": "NOT_RUN",
            "ios_validation": "NOT_RUN",
            "apple_validation": "NOT_RUN",
            "device_install": "BLOCKED",
        },
        "message": "open-i unavailable; install open-i or set NEPTUNE_OPEN_I_BIN",
    })
}
