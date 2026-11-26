//! Small stable facade for applications that author and run `.gscene` files.
//!
//! `SceneProject` validates source, lowers it to the shared GY-IR/2 table,
//! exposes scene geometry, and can create a stateful runtime. Platform hosts
//! still own window creation and presentation.

use goya_ir::Module;
use goya_lang::{
    compile_d1_interactions, compile_d2_interactions, layout as scene_layout, lower,
    CompiledD2Interaction, CompiledInteraction, DrawNode, Lowered, ParseError, RectMp, Scene,
    TransitionError,
};
use goya_layout::Constraints;
use goya_runtime::Runtime;
use goya_state::Store;
use std::fmt;
use std::path::Path;

mod application;
pub use application::{D1Application, D1ApplicationError, D1_APPLICATION_CONTRACT};
mod android_button_preview;
pub use android_button_preview::{
    emit_android_root_button_preview, emit_android_root_button_preview_mapped,
    kotlin_string_literal, AndroidRootButtonPreview, AndroidRootButtonPreviewError,
};
mod android_d1_runtime;
mod android_gvi;
mod android_live;
mod android_scene;
pub use android_scene::{
    compile_android_scene, compile_android_scene_with, AndroidScene, AndroidSceneError,
    AndroidSceneOptions, AndroidSceneReason, ANDROID_SCENE_MAX_CELLS, ANDROID_SCENE_MAX_TEXT_BYTES,
    ANDROID_SCENE_PACKAGE, ANDROID_SCENE_SPEC,
};
mod kotlin;
pub use android_d1_runtime::{
    emit_android_d1_runtime, emit_android_d1_runtime_mapped, AndroidD1Runtime,
    AndroidD1RuntimeError,
};
pub use kotlin::{
    AndroidSourceMap, KotlinFile, SourceMapEntry, SourceMapError, ANDROID_SOURCE_MAP_CONTRACT,
};
mod nevada_android;
mod nevada_project;
pub mod nevada_scene;
pub use nevada_project::{compile_nevada_project, NevadaProject, NevadaProjectError};

/// Refusal reason for the experimental Android static-text preview emitter.
#[derive(Debug)]
pub enum AndroidTextPreviewError {
    StrictParse(goya_lang::StrictParseError),
    Unsupported(&'static str),
    InvalidClassName,
}

/// Kotlin preview source plus the one authored property that produced its text.
/// The offsets are end-exclusive UTF-8 byte ranges in their respective files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidTextPreview {
    pub kotlin: String,
    pub source_digest: [u8; 32],
    pub semantic_id: String,
    pub source_range: goya_lang::SourceRange,
    pub generated_range: goya_lang::SourceRange,
}

/// Maximum UTF-8 payload accepted by the experimental Android text preview.
/// The generated Kotlin source is larger than the source payload because it
/// escapes non-ASCII characters, so keep this deliberately small and stable.
pub const ANDROID_TEXT_PREVIEW_MAX_UTF8_BYTES: usize = 16 * 1024;

impl fmt::Display for AndroidTextPreviewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StrictParse(error) => write!(f, "strict scene admission failed: {error}"),
            Self::Unsupported(feature) => {
                write!(f, "Android text preview does not support {feature}")
            }
            Self::InvalidClassName => write!(f, "invalid Kotlin class name"),
        }
    }
}

impl std::error::Error for AndroidTextPreviewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StrictParse(error) => Some(error),
            Self::Unsupported(_) | Self::InvalidClassName => None,
        }
    }
}

/// Emit an experimental Kotlin `TextView` source file for one strict,
/// source-bound scene whose entire content is a single static `@cell` text.
///
/// Optional text size and fill are preserved. This intentionally accepts no
/// layout, other styling, children, interactions, buttons, or inputs. The
/// output is a preview emitter; it does not establish Android build
/// integration, visual equivalence, or platform qualification.
pub fn emit_android_static_text_preview(source: &str) -> Result<String, AndroidTextPreviewError> {
    emit_android_static_text_preview_named(source, "GoyaStaticTextPreview")
}

/// Emit the static-text preview using a caller-selected Kotlin class name.
///
/// Names are restricted to non-keyword ASCII Kotlin identifiers so the
/// generated declaration is always a single ordinary class name.
pub fn emit_android_static_text_preview_named(
    source: &str,
    class_name: &str,
) -> Result<String, AndroidTextPreviewError> {
    emit_android_static_text_preview_mapped(source, class_name).map(|preview| preview.kotlin)
}

/// Emit the static-text preview and retain its source-to-Kotlin text mapping.
/// The map binds the generated literal to the exact admitted source revision.
pub fn emit_android_static_text_preview_mapped(
    source: &str,
    class_name: &str,
) -> Result<AndroidTextPreview, AndroidTextPreviewError> {
    if !is_kotlin_class_name(class_name) {
        return Err(AndroidTextPreviewError::InvalidClassName);
    }
    let handoff =
        goya_lang::handoff::admit(source).map_err(AndroidTextPreviewError::StrictParse)?;
    let scene = &handoff.parsed.scene;
    if !scene.interactions.is_empty() {
        return Err(AndroidTextPreviewError::Unsupported("interactions"));
    }
    let root = &scene.root;
    if root.kind != goya_lang::CellKind::Cell {
        return Err(AndroidTextPreviewError::Unsupported("buttons or inputs"));
    }
    if !root.children.is_empty() {
        return Err(AndroidTextPreviewError::Unsupported("child elements"));
    }
    if root.position.is_some()
        || root.extent.is_some()
        || root.position_p.is_some()
        || root.extent_p.is_some()
        || root.stroke.is_some()
        || root.stroke_width.is_some()
        || root.anchor.is_some()
        || root.wrap.is_some()
        || root.scroll
    {
        return Err(AndroidTextPreviewError::Unsupported("layout or styling"));
    }
    let text = root
        .text
        .as_deref()
        .ok_or(AndroidTextPreviewError::Unsupported("cells without text"))?;
    if text.len() > ANDROID_TEXT_PREVIEW_MAX_UTF8_BYTES {
        return Err(AndroidTextPreviewError::Unsupported(
            "text above the 16 KiB UTF-8 preview limit",
        ));
    }
    let semantic_id = root
        .authored_id
        .as_ref()
        .ok_or(AndroidTextPreviewError::Unsupported(
            "cells without authored IDs",
        ))?;
    let source_entry = handoff
        .parsed
        .entries
        .iter()
        .find(|entry| {
            entry.kind == "property"
                && entry.name.replace('-', "_") == "text"
                && entry.semantic_id.as_deref() == Some(semantic_id.as_str())
        })
        .ok_or(AndroidTextPreviewError::Unsupported(
            "text without a source origin",
        ))?;
    let literal = kotlin_string_literal(text);
    let mut imports =
        String::from("import android.content.Context\nimport android.widget.TextView\n");
    if root.text_size.is_some() {
        imports.push_str("import android.util.TypedValue\n");
    }
    let prefix = format!(
        "{imports}\n\
         /** Experimental Goya static-text preview. */\n\
         class {class_name}(context: Context) : TextView(context) {{\n\
             init {{\n\
                 text = "
    );
    let generated_start = prefix.len();
    let mut kotlin = format!("{prefix}{literal}\n");
    if let Some(text_size) = root.text_size {
        kotlin.push_str(&format!(
            "                 setTextSize(TypedValue.COMPLEX_UNIT_PT, {:?}f)\n",
            text_size
        ));
    }
    if let Some(fill) = root.fill {
        let red = (fill >> 16) & 0xff;
        let green = (fill >> 8) & 0xff;
        let blue = fill & 0xff;
        kotlin.push_str(&format!(
            "                 setTextColor(android.graphics.Color.rgb({red}, {green}, {blue}))\n"
        ));
    }
    kotlin.push_str("             }\n         }\n");
    Ok(AndroidTextPreview {
        kotlin,
        source_digest: handoff.parsed.source_digest,
        semantic_id: semantic_id.clone(),
        source_range: source_entry.range,
        generated_range: goya_lang::SourceRange {
            start: generated_start,
            end: generated_start + literal.len(),
        },
    })
}

fn is_kotlin_class_name(name: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "abstract",
        "annotation",
        "as",
        "by",
        "break",
        "catch",
        "class",
        "companion",
        "const",
        "continue",
        "crossinline",
        "data",
        "do",
        "else",
        "enum",
        "external",
        "final",
        "false",
        "finally",
        "for",
        "fun",
        "get",
        "if",
        "in",
        "interface",
        "is",
        "infix",
        "inline",
        "inner",
        "internal",
        "lateinit",
        "null",
        "noinline",
        "object",
        "open",
        "operator",
        "out",
        "override",
        "package",
        "private",
        "protected",
        "public",
        "reified",
        "return",
        "sealed",
        "super",
        "suspend",
        "tailrec",
        "this",
        "throw",
        "true",
        "try",
        "typealias",
        "typeof",
        "val",
        "var",
        "when",
        "while",
        "where",
        "set",
        "vararg",
    ];
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        && !name.chars().all(|ch| ch == '_')
        && !matches!(name, "Context" | "TextView")
        && !KEYWORDS.contains(&name)
}

pub use goya_executable::{
    CapabilityLabel, D1Outcome, D1Snapshot, D1Value, D2Outcome, EffectJournal, EffectResult,
    InputSnapshot, JournalEntry, JournalLimits,
};
pub use goya_ir::ActionId;
pub use goya_lang::{
    CompiledD2Interaction as D2SceneInteraction, CompiledInteraction as D1SceneInteraction,
};
pub use goya_vee::SubmitRequest as VeeSubmitRequest;

/// SDK contract implemented by this crate.
pub const SDK_CONTRACT: &str = "GY-SDK/1";

/// Logical viewport dimensions in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width_points: f64,
    pub height_points: f64,
}

impl Viewport {
    pub fn new(width_points: f64, height_points: f64) -> Result<Self, SdkError> {
        if !width_points.is_finite()
            || !height_points.is_finite()
            || width_points <= 0.0
            || height_points <= 0.0
            || width_points > 32_768.0
            || height_points > 32_768.0
        {
            return Err(SdkError::InvalidViewport);
        }
        Ok(Self {
            width_points,
            height_points,
        })
    }

    fn layout_constraints(self) -> Constraints {
        Constraints {
            max_width: (self.width_points * 1000.0).round() as i64,
            max_height: (self.height_points * 1000.0).round() as i64,
        }
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width_points: 800.0,
            height_points: 600.0,
        }
    }
}

/// Error returned by SDK scene-loading operations.
#[derive(Debug)]
pub enum SdkError {
    InvalidViewport,
    Parse(ParseError),
    Lower(goya_lang::LowerError),
    Read(std::io::Error),
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidViewport => {
                f.write_str("viewport must be finite and within 1..=32768 points")
            }
            Self::Parse(error) => write!(f, "scene parse failed: {error}"),
            Self::Lower(error) => write!(f, "scene lowering failed: {error}"),
            Self::Read(error) => write!(f, "could not read scene source: {error}"),
        }
    }
}

impl std::error::Error for SdkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Lower(error) => Some(error),
            Self::Read(error) => Some(error),
            Self::InvalidViewport => None,
        }
    }
}

/// A validated scene project and its canonical lowering.
#[derive(Debug, Clone)]
pub struct SceneProject {
    source: String,
    scene: Scene,
    lowered: Lowered,
    viewport: Viewport,
}

/// One `@input` node enumerated from the authored scene.
#[derive(Debug, Clone, PartialEq)]
pub struct InputNodeInfo {
    pub name: String,
    /// Placeholder text (the scene `text:` value); `None` when absent.
    pub placeholder: Option<String>,
    /// Millipoint geometry from the authored layout pass.
    pub rect: RectMp,
}

/// Geometry emitted by the authored scene layout pass.
#[derive(Debug, Clone, Default)]
pub struct SceneFrame {
    pub nodes: Vec<DrawNode>,
}

/// All statically compiled authored interactions in a project.
///
/// Triggers refer to the same depth-first button `ActionId`s as the lowered
/// module. D2 request construction consumes a caller-created journal; it
/// never performs a host effect or submits to VEE automatically.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompiledInteractions {
    d1: Vec<CompiledInteraction>,
    d2: Vec<CompiledD2Interaction>,
}

impl CompiledInteractions {
    pub fn d1(&self) -> &[CompiledInteraction] {
        &self.d1
    }

    pub fn d2(&self) -> &[CompiledD2Interaction] {
        &self.d2
    }

    /// Find the compiled interaction bound to a lowered button action.
    pub fn for_trigger(&self, trigger: ActionId) -> Option<SceneInteractionRef<'_>> {
        self.d1
            .iter()
            .find(|interaction| interaction.trigger() == trigger)
            .map(SceneInteractionRef::D1)
            .or_else(|| {
                self.d2
                    .iter()
                    .find(|interaction| interaction.trigger() == trigger)
                    .map(SceneInteractionRef::D2)
            })
    }
}

/// Borrowed interaction selected by its authored button trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneInteractionRef<'a> {
    D1(&'a CompiledInteraction),
    D2(&'a CompiledD2Interaction),
}

impl<'a> SceneInteractionRef<'a> {
    pub fn name(self) -> &'a str {
        match self {
            Self::D1(interaction) => interaction.name(),
            Self::D2(interaction) => interaction.name(),
        }
    }

    pub fn trigger(self) -> ActionId {
        match self {
            Self::D1(interaction) => interaction.trigger(),
            Self::D2(interaction) => interaction.trigger(),
        }
    }
}

impl SceneFrame {
    pub fn find(&self, name: &str) -> Option<&DrawNode> {
        self.nodes.iter().find(|node| node.name == name)
    }
}

impl SceneProject {
    /// Parse and lower a source string using the given viewport.
    pub fn from_source(source: impl Into<String>, viewport: Viewport) -> Result<Self, SdkError> {
        // Revalidate public fields in case a caller constructed `Viewport`
        // without `Viewport::new`.
        let viewport = Viewport::new(viewport.width_points, viewport.height_points)?;
        let source = source.into();
        let parsed = goya_lang::parse_with_source_map(&source).map_err(SdkError::Parse)?;
        if let Some(rule) = parsed.declarations.platform_exceptions.first() {
            let line = source[..rule.range.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1;
            return Err(SdkError::Parse(ParseError {
                line,
                range: rule.range,
            }));
        }
        let scene = parsed.scene;
        let lowered = lower(&scene, viewport.width_points, viewport.height_points)
            .map_err(SdkError::Lower)?;
        Ok(Self {
            source,
            scene,
            lowered,
            viewport,
        })
    }

    /// Read, parse, and lower a scene file.
    pub fn from_path(path: impl AsRef<Path>, viewport: Viewport) -> Result<Self, SdkError> {
        let source = std::fs::read_to_string(path).map_err(SdkError::Read)?;
        Self::from_source(source, viewport)
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn lowered(&self) -> &Lowered {
        &self.lowered
    }

    pub fn module(&self) -> &Module {
        &self.lowered.module
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Geometry computed by the scene frontend, preserving absolute and
    /// proportional `.gscene` placement.
    pub fn frame(&self) -> SceneFrame {
        let frame = scene_layout(
            &self.scene,
            self.viewport.width_points,
            self.viewport.height_points,
        );
        SceneFrame { nodes: frame.nodes }
    }

    /// Enumerate `@input` nodes with their placeholder and geometry,
    /// in draw order.
    pub fn input_nodes(&self) -> Vec<InputNodeInfo> {
        self.frame()
            .nodes
            .iter()
            .filter(|node| node.is_input)
            .map(|node| InputNodeInfo {
                name: node.name.clone(),
                placeholder: node.text.clone(),
                rect: node.rect,
            })
            .collect()
    }

    /// Compile the approved D1 and D2 authored interaction subsets. This
    /// validates all bindings across both classes and returns only verified
    /// wrappers; D2 effects remain caller-supplied journal entries.
    pub fn compile_interactions(&self) -> Result<CompiledInteractions, TransitionError> {
        Ok(CompiledInteractions {
            d1: compile_d1_interactions(&self.scene)?,
            d2: compile_d2_interactions(&self.scene)?,
        })
    }

    /// Create an event runtime over the canonical IR. Its flow layout is the
    /// GY-LAYOUT engine; use [`SceneProject::frame`] for authored scene geometry.
    pub fn runtime(&self, state: Store) -> Runtime {
        Runtime::new(
            self.lowered.module.clone(),
            state,
            self.viewport.layout_constraints(),
        )
    }
}

/// Validate a scene source without retaining its project.
pub fn validate(source: &str, viewport: Viewport) -> Result<(), SdkError> {
    SceneProject::from_source(source, viewport).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_runtime::Event;
    use std::collections::BTreeMap;

    #[test]
    fn target_neutral_scene_project_refuses_unresolved_platform_rules() {
        let source =
            "@ontology app { @cell root { id: \"root\"; } @platform android { \"media\": cede; } }";
        let start = source.find("\"media\"").unwrap();
        let end = source[start..].find(';').unwrap() + start + 1;
        let Err(SdkError::Parse(error)) = SceneProject::from_source(source, Viewport::default())
        else {
            panic!("expected unresolved platform exception refusal");
        };
        assert_eq!(error.range, goya_lang::SourceRange { start, end });
    }

    #[test]
    fn legacy_android_preview_stays_gssh1_and_refuses_platform_rules() {
        let source = "@ontology app { @cell root { id: \"root\"; text: \"x\"; } @platform android { \"media\": cede; } }";
        let AndroidTextPreviewError::StrictParse(error) =
            emit_android_static_text_preview(source).unwrap_err()
        else {
            panic!("expected GSSH/1 admission refusal");
        };
        assert_eq!(error.code, "GY-E1015");
    }

    #[test]
    fn android_preview_emits_only_strict_static_text_and_escapes_kotlin_literals() {
        let source = "@ontology demo { @cell label { id: \"label\"; text: \"Hello\" } }";
        let output = emit_android_static_text_preview(source).unwrap();
        assert!(
            output.contains("class GoyaStaticTextPreview(context: Context) : TextView(context)")
        );
        assert!(output.contains("text = \"Hello\""));
        assert_eq!(
            kotlin_string_literal("$user\n\"\\é"),
            "\"\\$user\\n\\\"\\\\\\u00e9\""
        );
        assert!(!output.contains("@ontology"));
    }

    #[test]
    fn android_preview_preserves_supported_label_style() {
        let source = "@ontology demo { @cell label { id: \"label\"; text: \"Hello\"; text-size: 18; fill: #123456 } }";
        let output = emit_android_static_text_preview(source).unwrap();
        assert!(output.contains("import android.util.TypedValue"));
        assert!(output.contains("setTextSize(TypedValue.COMPLEX_UNIT_PT, 18.0f)"));
        assert!(output.contains("setTextColor(android.graphics.Color.rgb(18, 52, 86))"));
    }

    #[test]
    fn android_preview_accepts_safe_custom_class_names_only() {
        let source = "@ontology demo { @cell label { id: \"label\"; text: \"Hello\" } }";
        let output = emit_android_static_text_preview_named(source, "WelcomePreview").unwrap();
        assert!(output.contains("class WelcomePreview(context: Context) : TextView(context)"));
        for invalid in [
            "",
            "class",
            "by",
            "catch",
            "finally",
            "get",
            "set",
            "where",
            "Context",
            "TextView",
            "_",
            "___",
            "9Preview",
            "Preview.Name",
            "Preview`\nclass Evil",
        ] {
            assert!(matches!(
                emit_android_static_text_preview_named(source, invalid),
                Err(AndroidTextPreviewError::InvalidClassName)
            ));
        }
    }

    #[test]
    fn android_preview_map_binds_generated_literal_to_unicode_source_property() {
        let source = "@ontology demo { @cell label { id: \"label\"; text: \"Hi é\" } }";
        let preview = emit_android_static_text_preview_mapped(source, "WelcomePreview").unwrap();
        assert_eq!(preview.semantic_id, "label");
        assert_eq!(
            &source[preview.source_range.start..preview.source_range.end],
            "text: \"Hi é\""
        );
        assert_eq!(
            &preview.kotlin[preview.generated_range.start..preview.generated_range.end],
            "\"Hi \\u00e9\""
        );
        assert_eq!(
            preview.source_digest,
            goya_lang::parse_with_source_map(source)
                .unwrap()
                .source_digest
        );
    }

    #[test]
    fn android_preview_emission_is_deterministic_and_digest_tracks_source_revision() {
        let source = "@ontology demo { @cell label { id: \"label\"; text: \"Hi $user é\" } }";
        let repeated = emit_android_static_text_preview_mapped(source, "WelcomePreview").unwrap();
        let again = emit_android_static_text_preview_mapped(source, "WelcomePreview").unwrap();
        assert_eq!(repeated, again);

        // Whitespace is not reflected in the preview's Kotlin output, but the
        // source digest must still distinguish the exact admitted revision.
        let reformatted =
            "@ontology demo {\n    @cell label { id: \"label\"; text: \"Hi $user é\" }\n}";
        let revised =
            emit_android_static_text_preview_mapped(reformatted, "WelcomePreview").unwrap();
        assert_eq!(repeated.kotlin, revised.kotlin);
        assert_ne!(repeated.source_digest, revised.source_digest);

        assert_eq!(
            &reformatted[revised.source_range.start..revised.source_range.end],
            "text: \"Hi $user é\""
        );
        assert_eq!(
            &revised.kotlin[revised.generated_range.start..revised.generated_range.end],
            "\"Hi \\$user \\u00e9\""
        );
    }

    #[test]
    fn android_preview_rejects_features_outside_its_static_text_subset() {
        for source in [
            "@ontology demo { @cell root { id: \"root\"; } }",
            "@ontology demo { @cell root { id: \"root\"; text: \"x\"; stroke: #ffffff } }",
            "@ontology demo { @cell root { id: \"root\"; text: \"x\"; @cell child { id: \"child\"; } } }",
            "@ontology demo { @button root { id: \"root\"; text: \"x\"; } }",
            "@ontology demo { @input root { id: \"root\"; text: \"x\"; } }",
        ] {
            assert!(matches!(
                emit_android_static_text_preview(source),
                Err(AndroidTextPreviewError::Unsupported(_))
            ));
        }

        let invalid = "@ontology demo { @cell root { text: \"x\"; } }";
        assert!(matches!(
            emit_android_static_text_preview(invalid),
            Err(AndroidTextPreviewError::StrictParse(_))
        ));
    }

    #[test]
    fn android_preview_bounds_text_payload_by_utf8_bytes() {
        let at_limit = format!(
            "@ontology demo {{ @cell label {{ id: \"label\"; text: \"{}\" }} }}",
            "é".repeat(ANDROID_TEXT_PREVIEW_MAX_UTF8_BYTES / 2)
        );
        assert!(emit_android_static_text_preview(&at_limit).is_ok());

        let over_limit = format!(
            "@ontology demo {{ @cell label {{ id: \"label\"; text: \"{}\" }} }}",
            "é".repeat(ANDROID_TEXT_PREVIEW_MAX_UTF8_BYTES / 2 + 1)
        );
        assert!(matches!(
            emit_android_static_text_preview(&over_limit),
            Err(AndroidTextPreviewError::Unsupported(
                "text above the 16 KiB UTF-8 preview limit"
            ))
        ));
    }

    const SCENE: &str =
        "@ontology sdk { @button go { position: (0, 0, 0) extent: (120, 40, 0) text: \"Go\" } }";

    #[test]
    fn project_parses_lowers_and_exposes_scene_geometry() {
        let project =
            SceneProject::from_source(SCENE, Viewport::new(240.0, 120.0).unwrap()).unwrap();
        assert_eq!(project.scene().name, "sdk");
        assert!(project.module().root.is_some());
        assert_eq!(
            project.lowered().table.get("go").unwrap().role,
            goya_lang::Role::Button
        );
        assert_eq!(project.frame().find("go").unwrap().rect.width, 120_000);
    }

    #[test]
    fn runtime_routes_canonical_button_actions() {
        let project = SceneProject::from_source(SCENE, Viewport::default()).unwrap();
        let mut runtime = project.runtime(Store::new());
        let action = runtime.hit_test(500, 1_000).unwrap();
        assert!(runtime.handle(Event::Activate(action)));
        assert!(!runtime.render_list().is_empty());
    }

    #[test]
    fn input_nodes_exposes_name_placeholder_and_geometry() {
        const WITH_INPUT: &str = "@ontology sdk { @cell root { position: (0, 0, 0) extent: (800, 600, 0) @button go { position: (0, 0, 0) extent: (120, 40, 0) text: \"Go\" } @input composer { position: (0, 100, 0) extent: (400, 40, 0) text: \"Compose\" } } }";
        let project =
            SceneProject::from_source(WITH_INPUT, Viewport::new(800.0, 600.0).unwrap()).unwrap();
        let inputs = project.input_nodes();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].name, "composer");
        assert_eq!(inputs[0].placeholder.as_deref(), Some("Compose"));
        assert_eq!(inputs[0].rect.width, 400_000);
        // Cells and buttons are not reported as inputs.
        let plain = SceneProject::from_source(SCENE, Viewport::default()).unwrap();
        assert!(plain.input_nodes().is_empty());
    }

    #[test]
    fn invalid_viewport_and_scene_fail_closed() {
        assert!(Viewport::new(f64::NAN, 20.0).is_err());
        assert!(SceneProject::from_source("not a scene", Viewport::default()).is_err());
    }

    #[test]
    fn d1_facade_maps_button_trigger_and_executes_typed_transition() {
        let source = "@ontology app { @button plus {} @interaction increment { trigger: button.plus.activate; class: D1; state { read count: int; write count: int; } input { amount: int; } set count = state.count + input.amount; } }";
        let project = SceneProject::from_source(source, Viewport::default()).unwrap();
        let interactions = project.compile_interactions().unwrap();
        assert_eq!(interactions.d1().len(), 1);
        assert!(interactions.d2().is_empty());
        let interaction = interactions.for_trigger(ActionId(0)).unwrap();
        assert_eq!(interaction.name(), "increment");
        assert_eq!(interaction.trigger(), ActionId(0));
        let SceneInteractionRef::D1(compiled) = interaction else {
            panic!("D1 trigger resolved to a different interaction class");
        };
        let state =
            D1Snapshot::new(7, BTreeMap::from([("count".to_owned(), D1Value::Int(4))])).unwrap();
        let inputs =
            InputSnapshot::new(BTreeMap::from([("amount".to_owned(), D1Value::Int(3))])).unwrap();
        let outcome = compiled.execute(&state, &inputs).unwrap();
        assert_eq!(outcome.next_state["count"], D1Value::Int(7));
    }

    #[test]
    fn d2_facade_builds_request_from_explicit_journal_only() {
        let source = "@ontology app { @button go {} @interaction fetch { trigger: button.go.activate; class: D2; effects { network; } requests { lookup: effect network capability \"network\" request \"static request\" result int64; } state { write count: int; } input { } set count = effect.lookup; } }";
        let project = SceneProject::from_source(source, Viewport::default()).unwrap();
        let interactions = project.compile_interactions().unwrap();
        assert!(interactions.d1().is_empty());
        let SceneInteractionRef::D2(compiled) = interactions.for_trigger(ActionId(0)).unwrap()
        else {
            panic!("D2 trigger resolved to a different interaction class");
        };
        assert_eq!(compiled.name(), "fetch");

        let mut journal = EffectJournal::new_d2(
            compiled.declaration(),
            compiled.proof(),
            compiled.capability_labels().iter().cloned(),
            JournalLimits::default(),
        )
        .unwrap();
        let label = CapabilityLabel::new("network").unwrap();
        let result = EffectResult::Int64(41).encode().unwrap();
        journal.record(&label, b"static request", &result).unwrap();
        let state =
            D1Snapshot::new(0, BTreeMap::from([("count".to_owned(), D1Value::Int(0))])).unwrap();
        let request = compiled
            .to_vee_request(&state, &InputSnapshot::default(), &journal, "sdk-test")
            .unwrap();
        assert_eq!(request.language, "python");
        assert!(request.source_code.contains("GOYA_D2_PROGRAM_HASH"));
    }
}
