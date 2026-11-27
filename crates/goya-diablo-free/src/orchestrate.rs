//! Diablo Free compilation orchestration.

use crate::{Fault, COMPILER_VERSION, DIRECTIVE_ID, PROFILE};
use crate::target::{classify_target, TargetClass};
use crate::manifest::freebsd_desktop;
use goya_diablos::pipeline::{CommandKind, Invocation, Run, execute as diablos_execute};
use goya_diablos::fault::exit;
use goya_diablos::sha256_hex;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Instant;

/// Execute a Diablo Free command.
pub fn execute(invocation: Invocation) -> Run {
    // Validate target is FreeBSD
    if !crate::is_freebsd_target(&invocation.target) {
        return Run {
            code: exit::ADMISSION_FAILED,
            json: json!({
                "schema_version": "1",
                "directive": DIRECTIVE_ID,
                "compiler": "diablo-free",
                "compiler_version": COMPILER_VERSION,
                "operation": format!("{:?}", invocation.kind).to_lowercase(),
                "exit_code": exit::ADMISSION_FAILED,
                "fault": {
                    "code": "TARGET_NOT_FREEBSD",
                    "message": format!("Target {} is not a FreeBSD target", invocation.target),
                },
            }),
            text: format!("Target {} is not a FreeBSD target", invocation.target),
        };
    }

    // Delegate to shared Diablos orchestration
    diablos_execute(invocation)
}

/// Create a default Invocation for Diablo Free.
pub fn default_invocation(kind: CommandKind, scene: Option<PathBuf>, target: String) -> Invocation {
    Invocation {
        kind,
        scene,
        directory: None,
        out: None,
        evidence: None,
        target,
        profile: PROFILE.to_string(),
        release: false,
        launch: false,
        localised: false,
        suite: None,
        package_format: None,
        window: "auto".to_string(),
        desktop: None,
        config_root: None,
        escene_disabled: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_invocation_creation() {
        let inv = default_invocation(CommandKind::Check, Some(PathBuf::from("test.gscene")), "x86_64-unknown-freebsd".to_string());
        assert_eq!(inv.target, "x86_64-unknown-freebsd");
        assert_eq!(inv.profile, PROFILE);
    }

    #[test]
    fn execute_rejects_non_freebsd() {
        let inv = default_invocation(CommandKind::Check, Some(PathBuf::from("test.gscene")), "x86_64-unknown-linux-gnu".to_string());
        let run = execute(inv);
        assert_eq!(run.code, exit::ADMISSION_FAILED);
    }
}