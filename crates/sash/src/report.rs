//! Evidence files for one linked static-window build.
//!
//! Runtime qualification is recorded as BLOCKED. These files do not claim a
//! clean-machine launch, a signature, or release admission.

use crate::admit::Disposition;
use crate::emit::Generated;
use crate::pe::{self, PeInfo};
use crate::{sha256_hex, DIRECTIVE_ID, PROFILE_PORTABLE};

pub struct SceneEvidence {
    pub presentation_profile: &'static str,
    pub semantic_id: String,
    pub dispositions: Vec<Disposition>,
    pub unit_name: &'static str,
    pub contains: &'static [&'static str],
    pub not_generated: &'static [&'static str],
}

pub struct BuildFacts {
    pub target_triple: String,
    pub exe_sha256: String,
    pub pdb_sha256: String,
    pub manifest_sha256: String,
    pub generated_sha256: String,
    pub source_digest: String,
    pub pe: PeInfo,
    pub rustc: String,
    pub cargo_xwin: String,
    pub llvm_rc: String,
}

pub fn files(
    scene: &SceneEvidence,
    generated: &Generated,
    facts: &BuildFacts,
) -> Vec<(&'static str, Vec<u8>)> {
    let mut imports = Vec::new();
    for name in &facts.pe.imports {
        imports.push(serde_json::json!({
            "name": name,
            "class": "windows-system",
            "delay_load": false,
        }));
    }
    for name in &facts.pe.delay_imports {
        imports.push(serde_json::json!({
            "name": name,
            "class": "windows-system",
            "delay_load": true,
        }));
    }
    let dependency = serde_json::json!({
        "schema_version": "1",
        "directive": DIRECTIVE_ID,
        "profile": PROFILE_PORTABLE,
        "presentation_profile": scene.presentation_profile,
        "target": facts.target_triple,
        "artifacts": [
            {"path": "App.exe", "sha256": facts.exe_sha256, "role": "primary-executable", "required_at_launch": true},
            {"path": "App.pdb", "sha256": facts.pdb_sha256, "role": "private-symbols", "required_at_launch": false}
        ],
        "imported_dlls": imports,
        "sidecars_required": [],
        "redistributables": [],
        "crt": "static requested with target-feature=+crt-static; this is not a claim of zero system imports",
        "import_floor_observation": "PerMonitorV2 helpers are imported from user32. That observation is not a Windows 10 or Windows 11 support claim.",
        "clean_machine_launch": "BLOCKED",
    });
    let build = serde_json::json!({
        "schema_version": "1",
        "directive": DIRECTIVE_ID,
        "compiler": "sash",
        "profile": PROFILE_PORTABLE,
        "presentation_profile": scene.presentation_profile,
        "target": facts.target_triple,
        "build_host": "linux-cross",
        "claim": "unqualified-cross-link",
        "status": "LINKED",
        "runtime_qualification": "BLOCKED",
        "release_admission": "BLOCKED",
        "source_digest": facts.source_digest,
        "exe_sha256": facts.exe_sha256,
        "pdb_sha256": facts.pdb_sha256,
        "manifest_sha256": facts.manifest_sha256,
        "generated_main_sha256": facts.generated_sha256,
        "pe": {
            "machine": format!("{:#06x}", facts.pe.machine),
            "magic": format!("{:#06x}", facts.pe.magic),
            "subsystem": facts.pe.subsystem,
            "subsystem_name": "IMAGE_SUBSYSTEM_WINDOWS_GUI",
            "entry_point": facts.pe.entry,
            "dll_characteristics": format!("{:#06x}", facts.pe.dll_characteristics),
            "defenses": pe::format_flags(facts.pe.dll_characteristics),
            "imports": facts.pe.imports,
            "delay_imports": facts.pe.delay_imports,
        },
        "toolchain": {
            "rustc": facts.rustc,
            "cargo_xwin": facts.cargo_xwin,
            "llvm_rc": facts.llvm_rc,
            "cross_compiler": "clang",
            "sdk": "pinned local windows-msvc-sysroot; this build does not download an SDK",
            "crt_static": true,
            "control_flow_guard": "requested",
        },
        "reproducibility": {
            "r1": "NOT_RUN",
            "r2": "NOT_RUN",
            "r3": "NOT_APPLICABLE",
            "host_pair": "NOT_COMPARED",
            "note": "One link does not compare a second worker. /Brepro, a stable build directory, and a stable symbols.o path are what a later same-host pair can measure.",
        },
        "reason": "This host linked a PE. It did not launch the executable on Windows.",
    });
    let dispositions = serde_json::json!({
        "schema_version": "1",
        "profile": scene.presentation_profile,
        "semantic_id": scene.semantic_id,
        "dispositions": scene.dispositions.iter().map(|item| serde_json::json!({
            "subject": item.subject,
            "disposition": item.disposition,
            "detail": item.detail,
        })).collect::<Vec<_>>(),
    });
    let graph = serde_json::json!({
        "schema_version": "1",
        "units": [{
            "name": scene.unit_name,
            "placement": "linked into App.exe",
            "contains": scene.contains,
            "not_generated": scene.not_generated,
        }],
        "incremental_invalidation": "NOT_RUN",
    });
    let readiness = format!(
        "# Sash release readiness\n\nStatus: BLOCKED\n\nDirective: {DIRECTIVE_ID}\n\nProfile: {PROFILE_PORTABLE}\n\nPresentation: {}\n\n`App.exe` is a release-mode PE produced by a Linux cross link. Clean-machine launch, signing, installed integration, accessibility, and performance gates were not run. This file is not a release admission.\n",
        scene.presentation_profile
    );
    vec![
        ("evidence/dependency-manifest.json", pretty(&dependency)),
        ("evidence/build-report.json", pretty(&build)),
        (
            "evidence/capability-dispositions.json",
            pretty(&dispositions),
        ),
        ("evidence/component-build-graph.json", pretty(&graph)),
        ("evidence/source-map.json", pretty(&generated.source_map)),
        (
            "evidence/generated-main.rs",
            generated.main_rs.as_bytes().to_vec(),
        ),
        (
            "evidence/app.manifest",
            generated.manifest_xml.as_bytes().to_vec(),
        ),
        ("evidence/release-readiness.md", readiness.into_bytes()),
    ]
}

fn pretty(value: &serde_json::Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap_or_else(|_| b"{}".to_vec());
    bytes.push(b'\n');
    bytes
}

/// A report that marks R1, R2, R3, or the same-host pair as PASS.
///
/// `verify` does not compare two workers and does not launch Windows, so it
/// cannot confirm those claims.
pub fn reproducibility_claim_is_unconfirmed(report: &serde_json::Value) -> bool {
    let Some(block) = report.get("reproducibility") else {
        return false;
    };
    ["r1", "r2", "r3", "host_pair"]
        .iter()
        .any(|key| block.get(*key).and_then(|value| value.as_str()) == Some("PASS"))
}

pub fn runtime_claim_is_unconfirmed(report: &serde_json::Value) -> bool {
    report
        .get("runtime_qualification")
        .and_then(|value| value.as_str())
        == Some("PASS")
        || report
            .get("release_admission")
            .and_then(|value| value.as_str())
            == Some("PASS")
}

pub fn reported_exe_hash(report: &serde_json::Value) -> Option<&str> {
    report.get("exe_sha256").and_then(|value| value.as_str())
}

pub fn sha256_matches(bytes: &[u8], expected: &str) -> bool {
    sha256_hex(bytes) == expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admit::Disposition;
    use crate::emit::Generated;
    use crate::pe::PeInfo;

    #[test]
    fn build_report_leaves_reproducibility_unclaimed() {
        let generated = Generated {
            main_rs: String::new(),
            cargo_toml: "",
            config_toml: "",
            build_rs: "",
            manifest_xml: String::new(),
            resource_script: String::new(),
            source_map: serde_json::json!({}),
        };
        let scene = SceneEvidence {
            presentation_profile: "sash.win32.static-window/0.0.3",
            semantic_id: "window.root".into(),
            dispositions: vec![Disposition {
                subject: "label".into(),
                disposition: "native",
                detail: "static".into(),
            }],
            unit_name: "static-window",
            contains: &["top-level HWND"],
            not_generated: &["startup behaviour"],
        };
        let facts = BuildFacts {
            target_triple: "x86_64-pc-windows-msvc".into(),
            exe_sha256: "ab".into(),
            pdb_sha256: "cd".into(),
            manifest_sha256: "ef".into(),
            generated_sha256: "01".into(),
            source_digest: "02".into(),
            pe: PeInfo {
                machine: 0x8664,
                magic: 0x020B,
                subsystem: 2,
                entry: 1,
                dll_characteristics: 0xC160,
                characteristics: 0x22,
                imports: vec!["KERNEL32.dll".into()],
                delay_imports: Vec::new(),
                manifest: Vec::new(),
                has_version_resource: true,
            },
            rustc: "rustc test".into(),
            cargo_xwin: "cargo-xwin test".into(),
            llvm_rc: "llvm-rc test".into(),
        };
        let report_bytes = files(&scene, &generated, &facts)
            .into_iter()
            .find(|(name, _)| *name == "evidence/build-report.json")
            .expect("build report")
            .1;
        let report: serde_json::Value = serde_json::from_slice(&report_bytes).unwrap();
        assert_eq!(report["claim"], "unqualified-cross-link");
        assert_eq!(report["runtime_qualification"], "BLOCKED");
        assert_eq!(report["reproducibility"]["r1"], "NOT_RUN");
        assert_eq!(report["reproducibility"]["r2"], "NOT_RUN");
        assert_eq!(report["reproducibility"]["r3"], "NOT_APPLICABLE");
        assert_eq!(report["reproducibility"]["host_pair"], "NOT_COMPARED");
        assert!(!reproducibility_claim_is_unconfirmed(&report));
        let mut forged = report.clone();
        forged["reproducibility"]["r2"] = serde_json::json!("PASS");
        assert!(reproducibility_claim_is_unconfirmed(&forged));
        forged["reproducibility"]["r2"] = serde_json::json!("NOT_RUN");
        forged["reproducibility"]["host_pair"] = serde_json::json!("PASS");
        assert!(reproducibility_claim_is_unconfirmed(&forged));
    }
}
