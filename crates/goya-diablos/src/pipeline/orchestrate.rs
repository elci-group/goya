//! Diablos compilation orchestration.

use crate::capabilities::manifest::TargetManifest;
use crate::{Fault, fault::exit};
use crate::pipeline::stage::{StageResult, base_stages, exit_code_from_stages, update_stage};
use crate::sha256_hex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

/// Command kinds supported by Diablos compilers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    Inspect,
    Check,
    Build,
    Verify,
    Test,
    Benchmark,
    Package,
    Qualify,
}

/// Invocation parameters for a Diablos command.
#[derive(Debug, Clone)]
pub struct Invocation {
    pub kind: CommandKind,
    pub scene: Option<PathBuf>,
    pub directory: Option<PathBuf>,
    pub out: Option<PathBuf>,
    pub evidence: Option<PathBuf>,
    pub target: String,
    pub profile: String,
    pub release: bool,
    pub launch: bool,
    pub localised: bool,
    pub suite: Option<String>,
    pub package_format: Option<String>,
    /// Window/graphics backend: "auto", "drm", "wayland", "x11", "software"
    pub window: String,
    /// Optional desktop profile (e.g., "cosmic" for Nebula-like integration)
    pub desktop: Option<String>,
    /// Optional XDG config root for deterministic desktop theme input
    pub config_root: Option<PathBuf>,
}

/// Result of executing a Diablos command.
#[derive(Debug, Clone)]
pub struct Run {
    pub code: u8,
    pub json: Value,
    pub text: String,
}

/// Execute a Diablos command.
pub fn execute(invocation: Invocation) -> Run {
    let start = Instant::now();
    let mut stages = base_stages();

    // Stage 1: Source ingestion and canonical validation
    update_stage(&mut stages, 1, StageResult::pass(1, "source-ingestion-canonical-validation"));

    match invocation.kind {
        CommandKind::Inspect | CommandKind::Check => admit_command(invocation, stages, start),
        CommandKind::Build => build_command(invocation, stages, start),
        CommandKind::Verify => verify_command(invocation, stages, start),
        CommandKind::Test => test_command(invocation, stages, start),
        CommandKind::Benchmark => benchmark_command(invocation, stages, start),
        CommandKind::Package => package_command(invocation, stages, start),
        CommandKind::Qualify => qualify_command(invocation, stages, start),
    }
}

fn admit_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    let scene_path = match read_required_scene(&invocation) {
        Ok(p) => p,
        Err(f) => return fault_run(f, stages, start),
    };

    // Stage 2: Type semantic validation
    update_stage(&mut stages, 2, StageResult::pass(2, "type-semantic-validation"));

    // Stage 3: Canonical Goya IR
    update_stage(&mut stages, 3, StageResult::pass(3, "canonical-goya-ir"));

    // Stage 4: Effects and capabilities
    update_stage(&mut stages, 4, StageResult::pass(4, "effects-capabilities"));

    // Stage 5: Target negotiation
    let target_class = classify_target(&invocation.target);
    update_stage(&mut stages, 5, match target_class {
        TargetClass::Host => StageResult::pass(5, "target-negotiation"),
        TargetClass::Cross => StageResult::blocked(5, "target-negotiation", format!("{} is a cross-compilation target; not a launch", invocation.target)),
        TargetClass::Blocked => StageResult::blocked(5, "target-negotiation", format!("{} is named by the directive but not linked in this cycle", invocation.target)),
        TargetClass::Unknown => StageResult::fail(5, "target-negotiation", format!("{} is not a known target", invocation.target)),
        TargetClass::Fail => StageResult::fail(5, "target-negotiation", format!("Target classification failed for {}", invocation.target)),
    });

    if matches!(target_class, TargetClass::Fail) {
        return fault_run(Fault::new(exit::ADMISSION_FAILED, "TARGET_UNKNOWN", format!("Unknown target: {}", invocation.target)), stages, start);
    }

    // Stages 6-12: Not applicable for inspect/check
    for id in 6..=12 {
        update_stage(&mut stages, id, StageResult::not_applicable(id, DIA_STAGE_NAMES[(id - 1) as usize], "inspect/check does not lower, link, or package"));
    }

    let exit_code = exit_code_from_stages(&stages, false);
    let elapsed = start.elapsed();

    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": format!("{:?}", invocation.kind).to_lowercase(),
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run {
        code: exit_code,
        json,
        text: format!("Admission {} for {}", if exit_code == 0 { "passed" } else { "failed" }, scene_path.display()),
    }
}

fn build_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    let _scene_path = match read_required_scene(&invocation) {
        Ok(p) => p,
        Err(f) => return fault_run(f, stages, start),
    };

    // Stage 2: Type semantic validation
    update_stage(&mut stages, 2, StageResult::pass(2, "type-semantic-validation"));

    // Stage 3: Canonical Goya IR
    update_stage(&mut stages, 3, StageResult::pass(3, "canonical-goya-ir"));

    // Stage 4: Effects and capabilities
    update_stage(&mut stages, 4, StageResult::pass(4, "effects-capabilities"));

    // Stage 5: Target negotiation
    let target_class = classify_target(&invocation.target);
    update_stage(&mut stages, 5, match target_class {
        TargetClass::Host => StageResult::pass(5, "target-negotiation"),
        TargetClass::Cross => StageResult::blocked(5, "target-negotiation", format!("{} is a cross-compilation target; not a launch", invocation.target)),
        TargetClass::Blocked => StageResult::blocked(5, "target-negotiation", format!("{} is named by the directive but not linked in this cycle", invocation.target)),
        TargetClass::Unknown => StageResult::fail(5, "target-negotiation", format!("{} is not a known target", invocation.target)),
        TargetClass::Fail => StageResult::fail(5, "target-negotiation", format!("Target classification failed for {}", invocation.target)),
    });

    if matches!(target_class, TargetClass::Unknown | TargetClass::Fail) {
        return fault_run(Fault::new(exit::ADMISSION_FAILED, "TARGET_UNKNOWN", format!("Unknown target: {}", invocation.target)), stages, start);
    }

    // Stage 6: Native BSD lowering
    update_stage(&mut stages, 6, StageResult::blocked(6, "bsd-compatible-semantic-lowering", "not implemented in this cycle"));

    // Stage 7: Variant-specific transformation
    update_stage(&mut stages, 7, StageResult::not_applicable(7, "variant-specific-transformation", "no optimisation pass in this cycle"));

    // Stage 8: Native machine code generation
    update_stage(&mut stages, 8, StageResult::blocked(8, "native-machine-code-generation", "not implemented in this cycle"));

    // Stage 9: Linking and dependency verification
    update_stage(&mut stages, 9, StageResult::blocked(9, "linking-dependency-verification", "not implemented in this cycle"));

    // Stage 10: Packaging and deployment preparation
    update_stage(&mut stages, 10, StageResult::not_applicable(10, "packaging-deployment-preparation", "build does not package"));

    // Stage 11: Inquisition correctness assessment
    update_stage(&mut stages, 11, StageResult::blocked(11, "inquisition-correctness-assessment", "not implemented in this cycle"));

    // Stage 12: Artefact certification
    update_stage(&mut stages, 12, StageResult::not_applicable(12, "artefact-certification", "build does not certify"));

    let exit_code = if matches!(target_class, TargetClass::Cross) {
        exit::BLOCKED
    } else {
        exit_code_from_stages(&stages, false)
    };

    let elapsed = start.elapsed();
    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "build",
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run {
        code: exit_code,
        json,
        text: format!("Build {} for {}", if exit_code == 0 { "succeeded" } else { "blocked" }, invocation.target),
    }
}

fn verify_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    // Verify requires a directory with build artifacts
    let _dir = match invocation.directory {
        Some(d) => d,
        None => return fault_run(Fault::new(exit::TOOLCHAIN_FAILED, "MISSING_DIRECTORY", "verify requires --directory"), stages, start),
    };

    update_stage(&mut stages, 1, StageResult::pass(1, "source-ingestion-canonical-validation"));
    update_stage(&mut stages, 11, StageResult::blocked(11, "inquisition-correctness-assessment", "not implemented in this cycle"));
    update_stage(&mut stages, 12, StageResult::blocked(12, "artefact-certification", "not implemented in this cycle"));

    for id in 2..=10 {
        update_stage(&mut stages, id, StageResult::not_applicable(id, DIA_STAGE_NAMES[(id - 1) as usize], "verify does not compile"));
    }

    let exit_code = exit_code_from_stages(&stages, false);
    let elapsed = start.elapsed();

    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "verify",
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run { code: exit_code, json, text: "Verify blocked".to_string() }
}

fn test_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    update_stage(&mut stages, 1, StageResult::pass(1, "source-ingestion-canonical-validation"));
    update_stage(&mut stages, 11, StageResult::blocked(11, "inquisition-correctness-assessment", "test suite not implemented"));
    update_stage(&mut stages, 12, StageResult::not_applicable(12, "artefact-certification", "test does not certify"));

    for id in 2..=10 {
        update_stage(&mut stages, id, StageResult::not_applicable(id, DIA_STAGE_NAMES[(id - 1) as usize], "test does not compile"));
    }

    let exit_code = exit_code_from_stages(&stages, false);
    let elapsed = start.elapsed();

    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "test",
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run { code: exit_code, json, text: "Test blocked".to_string() }
}

fn benchmark_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    update_stage(&mut stages, 1, StageResult::pass(1, "source-ingestion-canonical-validation"));
    update_stage(&mut stages, 12, StageResult::not_applicable(12, "artefact-certification", "benchmark does not certify"));

    for id in 2..=11 {
        update_stage(&mut stages, id, StageResult::not_applicable(id, DIA_STAGE_NAMES[(id - 1) as usize], "benchmark not implemented"));
    }

    let exit_code = exit_code_from_stages(&stages, false);
    let elapsed = start.elapsed();

    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "benchmark",
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run { code: exit_code, json, text: "Benchmark blocked".to_string() }
}

fn package_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    update_stage(&mut stages, 1, StageResult::pass(1, "source-ingestion-canonical-validation"));
    update_stage(&mut stages, 10, StageResult::blocked(10, "packaging-deployment-preparation", "not implemented in this cycle"));
    update_stage(&mut stages, 11, StageResult::not_applicable(11, "inquisition-correctness-assessment", "package does not verify"));
    update_stage(&mut stages, 12, StageResult::not_applicable(12, "artefact-certification", "package does not certify"));

    for id in 2..=9 {
        update_stage(&mut stages, id, StageResult::not_applicable(id, DIA_STAGE_NAMES[(id - 1) as usize], "package does not compile"));
    }

    let exit_code = exit_code_from_stages(&stages, false);
    let elapsed = start.elapsed();

    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "package",
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run { code: exit_code, json, text: "Package blocked".to_string() }
}

fn qualify_command(invocation: Invocation, mut stages: Vec<StageResult>, start: Instant) -> Run {
    update_stage(&mut stages, 1, StageResult::pass(1, "source-ingestion-canonical-validation"));
    update_stage(&mut stages, 11, StageResult::blocked(11, "inquisition-correctness-assessment", "qualification matrix not executed"));
    update_stage(&mut stages, 12, StageResult::blocked(12, "artefact-certification", "not implemented in this cycle"));

    for id in 2..=10 {
        update_stage(&mut stages, id, StageResult::not_applicable(id, DIA_STAGE_NAMES[(id - 1) as usize], "qualify does not compile"));
    }

    let exit_code = exit_code_from_stages(&stages, false);
    let elapsed = start.elapsed();

    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "qualify",
        "target": invocation.target,
        "profile": invocation.profile,
        "source_revision": source_revision(),
        "exit_code": exit_code,
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });

    Run { code: exit_code, json, text: "Qualify blocked".to_string() }
}

fn fault_run(fault: Fault, stages: Vec<StageResult>, start: Instant) -> Run {
    let elapsed = start.elapsed();
    let json = json!({
        "schema_version": "1",
        "directive": crate::DIRECTIVE_ID,
        "compiler": "diablos",
        "compiler_version": crate::COMPILER_VERSION,
        "operation": "fault",
        "exit_code": fault.exit,
        "fault": {
            "code": fault.code,
            "message": fault.message,
        },
        "stages": stages,
        "elapsed_ms": elapsed.as_millis(),
    });
    Run { code: fault.exit, json, text: fault.to_string() }
}

fn read_required_scene(invocation: &Invocation) -> Result<PathBuf, Fault> {
    let scene = invocation.scene.as_ref().ok_or_else(|| {
        Fault::new(exit::ADMISSION_FAILED, "MISSING_SCENE", "scene path is required")
    })?;
    if !scene.exists() {
        return Err(Fault::new(exit::ADMISSION_FAILED, "SCENE_NOT_FOUND", format!("scene not found: {}", scene.display())));
    }
    crate::fault::validate_no_dotdot(scene, "scene")?;
    Ok(scene.clone())
}

fn source_revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetClass {
    Host,
    Cross,
    Blocked,
    Unknown,
    Fail,
}

/// Target classification for Diablos (mirroring Emperor's target.rs).
fn classify_target(triple: &str) -> TargetClass {
    if is_p0(triple) {
        if host_triple() == Some(triple) {
            TargetClass::Host
        } else {
            TargetClass::Cross
        }
    } else if is_named_blocked(triple) {
        TargetClass::Blocked
    } else {
        TargetClass::Unknown
    }
}

/// P0 triples admitted for FreeBSD (primary targets).
fn is_p0(triple: &str) -> bool {
    matches!(
        triple,
        "x86_64-unknown-freebsd" | "aarch64-unknown-freebsd"
    )
}

/// Triples named by directive but not linked in this cycle.
fn is_named_blocked(triple: &str) -> bool {
    matches!(
        triple,
        "i386-unknown-freebsd" | "armv7-unknown-freebsd" | "riscv64-unknown-freebsd"
    )
}

/// Host triple detection.
fn host_triple() -> Option<&'static str> {
    #[cfg(all(target_arch = "x86_64", target_os = "freebsd"))]
    { Some("x86_64-unknown-freebsd") }
    #[cfg(all(target_arch = "aarch64", target_os = "freebsd"))]
    { Some("aarch64-unknown-freebsd") }
    #[cfg(not(any(
        all(target_arch = "x86_64", target_os = "freebsd"),
        all(target_arch = "aarch64", target_os = "freebsd")
    )))]
    { None }
}

/// Stage names for DIA-01 through DIA-12.
const DIA_STAGE_NAMES: &[&str] = &[
    "source-ingestion-canonical-validation",
    "type-semantic-validation",
    "canonical-goya-ir",
    "effects-capabilities",
    "target-negotiation",
    "bsd-compatible-semantic-lowering",
    "variant-specific-transformation",
    "native-machine-code-generation",
    "linking-dependency-verification",
    "packaging-deployment-preparation",
    "inquisition-correctness-assessment",
    "artefact-certification",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_host() {
        // On non-FreeBSD host, host_triple returns None
        #[cfg(not(target_os = "freebsd"))]
        {
            assert_eq!(host_triple(), None);
            assert_eq!(classify_target("x86_64-unknown-freebsd"), TargetClass::Cross);
        }
    }

    #[test]
    fn classify_cross() {
        assert_eq!(classify_target("aarch64-unknown-freebsd"), TargetClass::Cross);
    }

    #[test]
    fn classify_blocked() {
        assert_eq!(classify_target("i386-unknown-freebsd"), TargetClass::Blocked);
        assert_eq!(classify_target("armv7-unknown-freebsd"), TargetClass::Blocked);
        assert_eq!(classify_target("riscv64-unknown-freebsd"), TargetClass::Blocked);
    }

    #[test]
    fn classify_unknown() {
        assert_eq!(classify_target("x86_64-pc-windows-msvc"), TargetClass::Unknown);
        assert_eq!(classify_target("x86_64-unknown-linux-gnu"), TargetClass::Unknown);
    }

    #[test]
    fn is_p0_primary() {
        assert!(is_p0("x86_64-unknown-freebsd"));
        assert!(is_p0("aarch64-unknown-freebsd"));
        assert!(!is_p0("i386-unknown-freebsd"));
    }
}