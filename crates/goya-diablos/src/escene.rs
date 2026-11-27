//! ESCENE generation for the Diablos compilation family (GOYA-ESCENE-001
//! Phase ESC-2, §6.2).
//!
//! In this Phase-0 skeleton the lowering, code-generation and linking stages
//! are blocked. The artefact therefore records exactly what the pipeline did:
//! admission (source accepted, dependency capability checks) as Compilation
//! evidence, with Coverage honestly disclosing that lowering, code generation
//! and linking produced no observations. No evidence is invented for stages
//! that did not run (ESC-INV-009).
//!
//! Generation failure never invalidates a successful build (ESC-INV-012).

use crate::pipeline::orchestrate::Invocation;
use goya_escene::build::Builder;
use goya_escene::model::{
    ClassCoverage, Coverage, CoverageStatus, ExecutionModel, Target,
};
use goya_escene::rules::{CaprichioApplication, RuleRegistry};
use goya_escene::source::SourceSnapshot;
use goya_escene::{Collector, CompilationEvent, EsceneEvent};
use std::path::Path;

/// Generate and persist a partial `.escene` for a build/check invocation.
/// Returns the artefact path on success; a soft error string on generation
/// failure — callers must treat that as non-fatal (ESC-INV-012).
pub fn generate_for_invocation(invocation: &Invocation, out_dir: &Path) -> Result<std::path::PathBuf, String> {
    let scene_path = invocation
        .scene
        .as_ref()
        .ok_or_else(|| "no scene supplied".to_string())?;
    let bytes = std::fs::read(scene_path).map_err(|e| format!("cannot read scene: {e}"))?;
    let stem = scene_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Scene")
        .to_string();

    let source = SourceSnapshot::capture("src-1", &scene_path.display().to_string(), bytes);
    let collector = compile_time_collector(invocation, &source);
    if !collector.enabled {
        return Ok(out_dir.to_path_buf()); // disabled: nothing generated
    }

    let target = Target {
        id: target_id(&invocation.target),
        platform: "freebsd".into(),
        platform_version: None,
        architecture: Some(arch_of(&invocation.target)),
        toolchain: "diablo-free".into(),
        toolchain_version: crate::COMPILER_VERSION.into(),
        execution_model: ExecutionModel::Compiled,
        configuration_digest: format!(
            "sha256:{}",
            goya_escene::source::sha256_hex(
                format!("{}|{}|release={}", invocation.profile, invocation.window, invocation.release).as_bytes()
            )
        ),
    };

    let artefact = Builder::new(source, target)
        .finish(
            collector.snapshot(),
            &caprichio_applications(),
            vec![],
            vec![],
            collector.snapshot().records.clone(),
            coverage(&collector),
            &RuleRegistry::v1(),
        )
        .map_err(|e| e)?;

    let dir = out_dir.join("escene");
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create escene dir: {e}"))?;
    let path = dir.join(format!("{}-{}.escene", stem, target_id(&invocation.target)));
    // Atomic persistence where the filesystem allows it (§19.2).
    let tmp = dir.join(format!(".{}.tmp", path.file_name().unwrap().to_string_lossy()));
    let payload = serde_json::to_vec_pretty(&artefact).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, &payload).map_err(|e| format!("cannot write artefact: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot publish artefact: {e}"))?;
    Ok(path)
}

/// Compile-time collector: records the events the Phase-0 pipeline genuinely
/// produced. Lowering is blocked, so no lowering evidence is emitted.
fn compile_time_collector(invocation: &Invocation, source: &SourceSnapshot) -> Collector {
    // §6.1: enabled by default; --no-escene style policies disable.
    let mut collector = Collector::enabled("diablo-free", 1_048_576);
    let source_len = source.byte_length;
    collector.emit(&EsceneEvent::Compilation(CompilationEvent::SourceAccepted {
        source_id: source.id.clone(),
        byte_length: source_len,
    }));
    let _ = invocation;
    collector
}

fn caprichio_applications() -> Vec<CaprichioApplication> {
    // No Caprichios are applied in the Phase-0 skeleton; an empty record is
    // the honest state (ESC-INV-010).
    vec![]
}

fn coverage(collector: &Collector) -> Coverage {
    Coverage {
        // Admission evidence only: source accepted, capability checks passed.
        c: ClassCoverage {
            status: CoverageStatus::Partial,
            limitations: vec![
                "lowering, code generation and linking are blocked in this cycle; no implementation evidence available".into(),
            ],
            dropped_events: collector.dropped_events(),
        },
        // No execution occurred during compilation.
        i: ClassCoverage {
            status: CoverageStatus::NotApplicable,
            limitations: vec!["compilation mode does not execute the application".into()],
            dropped_events: 0,
        },
        s: ClassCoverage {
            status: CoverageStatus::Complete,
            limitations: vec!["no Caprichios applied in this cycle".into()],
            dropped_events: 0,
        },
    }
}

fn target_id(triple: &str) -> String {
    // x86_64-unknown-freebsd → "freebsd-x86_64" style stable id.
    let mut parts = triple.split('-');
    let arch = parts.next().unwrap_or("unknown");
    let os = parts
        .find(|p| !p.is_empty() && *p != "unknown")
        .unwrap_or("freebsd");
    format!("{os}-{arch}")
}

fn arch_of(triple: &str) -> String {
    triple.split('-').next().unwrap_or("unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::orchestrate::{CommandKind, Invocation};
    use goya_escene::validate::validate;
    use goya_escene::RuleRegistry;

    fn invocation() -> Invocation {
        Invocation {
            kind: CommandKind::Build,
            scene: Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/calculator.gscene")),
            directory: None,
            out: None,
            evidence: None,
            target: "x86_64-unknown-freebsd".into(),
            profile: "balanced".into(),
            release: false,
            launch: false,
            localised: false,
            suite: None,
            package_format: None,
            window: "auto".into(),
            desktop: None,
            config_root: None,
            escene_disabled: false,
        }
    }

    #[test]
    fn generates_valid_partial_artefact() {
        let tmp = tempfile::tempdir().unwrap();
        let path = generate_for_invocation(&invocation(), tmp.path()).expect("generation must succeed");
        let raw = std::fs::read(&path).unwrap();
        let artefact: goya_escene::Artefact = serde_json::from_slice(&raw).unwrap();
        assert!(validate(&artefact, &RuleRegistry::v1()).is_ok(), "artefact must conform");
        // Honest coverage: C partial, I not applicable, S complete.
        assert_eq!(artefact.coverage.c.status, CoverageStatus::Partial);
        assert_eq!(artefact.coverage.i.status, CoverageStatus::NotApplicable);
        // No annotations claim lowering that did not happen.
        assert!(
            !artefact.annotations.iter().any(|a| a.code.0.starts_with("C.NATIVE_LOWERING")),
            "blocked lowering must not be reported as realised"
        );
    }

    #[test]
    fn source_recovery_is_byte_exact() {
        let tmp = tempfile::tempdir().unwrap();
        let path = generate_for_invocation(&invocation(), tmp.path()).unwrap();
        let artefact: goya_escene::Artefact =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let original = std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/calculator.gscene"),
        )
        .unwrap();
        assert_eq!(artefact.source.recover().unwrap(), original);
    }
}
