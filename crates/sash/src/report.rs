//! Evidence files for one linked static-window build.
//!
//! Runtime qualification is recorded as BLOCKED. These files do not claim a
//! clean-machine launch, a signature, or release admission.

use crate::admit::{AdmittedWindow, PROFILE};
use crate::emit::Generated;
use crate::pe::{self, PeInfo};
use crate::{sha256_hex, DIRECTIVE_ID, PROFILE_PORTABLE, TARGET_MSVC_X64};

pub struct BuildFacts {
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
    window: &AdmittedWindow,
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
        "presentation_profile": PROFILE,
        "target": TARGET_MSVC_X64,
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
        "presentation_profile": PROFILE,
        "target": TARGET_MSVC_X64,
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
            "crt_static": true,
            "control_flow_guard": "requested",
        },
        "reproducibility": {"r1": "NOT_RUN", "r2": "NOT_RUN", "r3": "NOT_APPLICABLE"},
        "reason": "This host linked a PE. It did not launch the executable on Windows.",
    });
    let dispositions = serde_json::json!({
        "schema_version": "1",
        "profile": PROFILE,
        "semantic_id": window.semantic_id,
        "dispositions": window.dispositions.iter().map(|item| serde_json::json!({
            "subject": item.subject,
            "disposition": item.disposition,
            "detail": item.detail,
        })).collect::<Vec<_>>(),
    });
    let graph = serde_json::json!({
        "schema_version": "1",
        "units": [{
            "name": "static-window",
            "placement": "linked into App.exe",
            "contains": ["top-level HWND", "one STATIC label"],
            "not_generated": ["banner communications", "taskbar behaviour", "startup behaviour", "actions", "retention", "nested layout"],
        }],
        "incremental_invalidation": "NOT_RUN",
    });
    let readiness = format!(
        "# Sash release readiness\n\nStatus: BLOCKED\n\nDirective: {DIRECTIVE_ID}\n\nProfile: {PROFILE_PORTABLE}\n\nPresentation: {PROFILE}\n\n`App.exe` is a release-mode PE produced by a Linux cross link. Clean-machine launch, signing, installed integration, accessibility, and performance gates were not run. This file is not a release admission.\n"
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
