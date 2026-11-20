//! Compile GSSH/3 through Nevada's static gate before producing TV sources.

use std::{fmt, fmt::Write};

use goya_nevada::{
    focus::Viewport,
    gate::{self, SceneInteraction},
};

use crate::{
    nevada_android::emit_nevada_android,
    nevada_scene::{lower_nevada_scene, NevadaSceneError, TV_VIEWPORT},
    AndroidSceneError, AndroidSourceMap, KotlinFile,
};

/// Files and static conformance evidence for the locked Aragon Gradle project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NevadaProject {
    pub manifest: &'static str,
    pub banner: &'static [u8],
    pub icon: &'static str,
    pub files: Vec<KotlinFile>,
    pub source_map: AndroidSourceMap,
    pub conformance: String,
}

#[derive(Debug)]
pub enum NevadaProjectError {
    Scene(NevadaSceneError),
    Gate(gate::GateError),
    Android(AndroidSceneError),
    SceneMismatch,
}

impl fmt::Display for NevadaProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scene(error) => write!(f, "scene admission failed at bytes {error}"),
            Self::Gate(error) => error.fmt(f),
            Self::Android(error) => write!(f, "Android TV lowering failed at bytes {error}"),
            Self::SceneMismatch => {
                f.write_str("TV emitter and Nevada gate disagree on scene identity")
            }
        }
    }
}

impl std::error::Error for NevadaProjectError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Scene(error) => Some(error),
            Self::Gate(error) => Some(error),
            Self::Android(error) => Some(error),
            Self::SceneMismatch => None,
        }
    }
}

/// Refuse the project unless the selected profile and exact scene layout can
/// preserve all admitted interactions through the generated remote runtime.
pub fn compile_nevada_project(
    source: &str,
    profile_id: &str,
) -> Result<NevadaProject, NevadaProjectError> {
    let scene = lower_nevada_scene(source).map_err(NevadaProjectError::Scene)?;
    let (width, height) = TV_VIEWPORT;
    let viewport = Viewport {
        x: 0,
        y: 0,
        w: i64::from(width) * 1_000,
        h: i64::from(height) * 1_000,
    };
    let elements: Vec<_> = scene
        .elements
        .iter()
        .copied()
        .filter(|cell| cell.focusable)
        .collect();
    let interactions: Vec<_> = scene
        .interactions
        .iter()
        .map(|entry| SceneInteraction {
            element_id: entry.element_id,
            authored_id: &entry.authored_id,
            kind: entry.kind,
        })
        .collect();
    let verified = gate::verify(profile_id, viewport, &elements, &interactions)
        .map_err(NevadaProjectError::Gate)?;
    let android = emit_nevada_android(source, &verified).map_err(NevadaProjectError::Android)?;
    if scene.cell_ids != android.scene.cell_ids
        || scene.source_digest != android.scene.source_map.source_digest
    {
        return Err(NevadaProjectError::SceneMismatch);
    }

    let profile = verified.profile_id();
    let initial_focus = verified.initial_focus();
    let mut evidence = format!(
        "{} {} PASS\nprofile={profile}\ninitial_focus={initial_focus}\nsource_sha256=",
        gate::ID,
        gate::STAGE
    );
    for byte in scene.source_digest {
        write!(evidence, "{byte:02x}").unwrap();
    }
    evidence.push('\n');
    for (route, authored) in verified.routes().iter().zip(&scene.interactions) {
        writeln!(
            evidence,
            "control={} source={}..{} binding={:?} {:?} -> {:?}",
            route.element_id,
            authored.range.start,
            authored.range.end,
            route.authored_id,
            route.kind,
            route.remote
        )
        .unwrap();
    }
    evidence.push_str("runtime-conformance BLOCKED: Android TV device execution required\n");

    let mut files = android.scene.files;
    files.push(android.activity);
    Ok(NevadaProject {
        manifest: android.manifest,
        banner: android.banner,
        icon: android.icon,
        files,
        source_map: android.scene.source_map,
        conformance: evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = "oem.generic.tv/1.0.0";
    const COUNTER: &str = include_str!("../../../fixtures/nevada/counter.gscene");

    #[test]
    fn tv_project_preserves_authored_routes_and_is_reproducible() {
        let project = compile_nevada_project(COUNTER, PROFILE).unwrap();
        assert_eq!(project, compile_nevada_project(COUNTER, PROFILE).unwrap());
        assert!(project
            .manifest
            .contains("android.intent.category.LEANBACK_LAUNCHER"));
        assert!(project.manifest.contains("@drawable/nevada_banner"));
        assert!(project.manifest.contains("@drawable/nevada_icon"));
        assert!(project
            .conformance
            .contains("action.add\" TouchTap -> FocusAndSelect"));
        assert!(project
            .conformance
            .contains("window.amount\" TextInput -> TvKeyboard"));
        assert!(project.conformance.contains("runtime-conformance BLOCKED"));
        let remote = project
            .files
            .iter()
            .find(|file| file.name == "NevadaRemote.kt")
            .unwrap();
        assert!(remote.text.contains("rows[2] = intArrayOf(-1, -1, -1, 3)"));
        assert!(remote.text.contains("rows[3] = intArrayOf(-1, -1, 2, -1)"));
        project.source_map.verify(COUNTER, &project.files).unwrap();
    }

    #[test]
    fn unknown_profiles_and_unreachable_controls_refuse_the_project() {
        let diagonal = COUNTER.replace("(0.65, 0.22, 0)", "(0.65, 0.5, 0)");
        for (scene, profile, code) in [
            (COUNTER, "oem.generic.tv", "GY-NV-504"),
            (&diagonal, PROFILE, "GY-NV-501"),
        ] {
            let error = compile_nevada_project(scene, profile).unwrap_err();
            assert!(matches!(error, NevadaProjectError::Gate(_)));
            assert!(error.to_string().contains(code));
        }
    }
}
