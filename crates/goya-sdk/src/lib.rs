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
        let scene = goya_lang::parse(&source).map_err(SdkError::Parse)?;
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
        let state = D1Snapshot::new(
            0,
            BTreeMap::from([("count".to_owned(), D1Value::Int(0))]),
        )
        .unwrap();
        let request = compiled
            .to_vee_request(&state, &InputSnapshot::default(), &journal, "sdk-test")
            .unwrap();
        assert_eq!(request.language, "python");
        assert!(request.source_code.contains("GOYA_D2_PROGRAM_HASH"));
    }
}
