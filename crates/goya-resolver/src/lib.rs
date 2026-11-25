//! GY-RESOLVER/0.1 — headless execution of an admitted `.gscene`.
//!
//! The resolver admits source through the GSSH handoff, publishes D1 actions
//! with the `GY-D1-APP/1` rules, and projects `text-from` displays through
//! `goya-lang`. It does not compile a native executable, execute D2 effects,
//! select a Caprichio, or draw a window.
//!
//! The baseline for this slice is `docs/resolver/GRD-001-baseline.md`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use goya_lang::display_binding::{scene_display_values, SceneDisplayValue};
use goya_lang::handoff::convert_field;
use goya_lang::{Frame, InteractionValue, Lowered, Scene, ScrollOffsets};

mod admission;
mod caret;
mod dependencies;
mod diagnostics;
mod execution;
mod lifecycle;
mod platform;
mod policy;
mod project;
mod reload;
mod representation;

pub use admission::SemanticContract;
pub use caret::{CaretMotion, CaretPlace, Edit};
use diagnostics::located;
pub use diagnostics::Diagnostic;
pub use lifecycle::Phase;
pub use platform::Target;
pub use policy::{Exhaustion, PerformanceProfile, ResourcePolicy};
pub use project::ProjectedNode;

/// Contract implemented by this crate. The embedding surface may grow without
/// changing the GSSH handoff it consumes.
pub const CONTRACT: &str = "GY-RESOLVER/0.1";

/// One element of the admitted scene, addressed by a depth-first component id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Component<'a> {
    pub id: u32,
    pub name: &'a str,
    pub semantic_id: Option<&'a str>,
    pub kind: goya_lang::CellKind,
}

/// One declared state key as currently published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedState {
    pub key: String,
    pub value: InteractionValue,
    pub lifetime: goya_lang::StateLifetime,
}

/// One authored action the resolver can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionInfo {
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub class_name: &'static str,
}

/// Result of one accepted action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchOutcome {
    pub revision: u64,
    pub announcement: Option<String>,
    pub affected_elements: Vec<String>,
    pub sites_evaluated: usize,
}

/// What a millipoint hit did. Cells that are not buttons or inputs miss.
/// Scroll offsets are not applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointOutcome {
    /// The point is outside every button and input.
    Miss,
    /// The point is inside an input. Field text is unchanged.
    Focus { name: String, id: Option<String> },
    /// The point is inside a button and its action ran.
    Activate {
        name: String,
        outcome: DispatchOutcome,
    },
}

/// What a millipoint scroll did. The offset is clamped by `goya_lang`.
/// A scroll does not publish state and does not move focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScrollOutcome {
    /// No scrollable viewport is under the point.
    Miss,
    /// The viewport stored this clamped offset, in millipoints.
    Moved { name: String, x_mp: i64, y_mp: i64 },
}

/// Keys kept, authored, or dropped by one successful [`Instance::reload`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadReport {
    pub preserved_keys: Vec<String>,
    pub initialised_keys: Vec<String>,
    pub dropped_keys: Vec<String>,
    pub preserved_fields: Vec<String>,
}

/// An event the running instance can apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Run the action named by its authored id, its structural name, or its
    /// `button.<name>.activate` trigger.
    Activate { action: String },
}

#[derive(Clone)]
struct FieldState {
    text: String,
    max_chars: Option<i64>,
    range: Option<goya_lang::SourceRange>,
}

/// Input last returned by [`Instance::point`]. Cleared by a miss or by a
/// button activation that publishes.
#[derive(Clone)]
struct FocusedInput {
    name: String,
    id: Option<String>,
    /// UTF-8 byte offset on a grapheme boundary.
    caret: usize,
}

struct ApplicationInner {
    contract: &'static str,
    scene: Scene,
    digest: [u8; 32],
    lowered: Option<Lowered>,
    intern: representation::Interner,
    components: Vec<representation::StoredComponent>,
    displays: dependencies::DisplayIndex,
    prepared: execution::Prepared,
    target: Option<Target>,
}

/// A resolved scene. Cloning shares the resolved representation.
#[derive(Clone)]
pub struct Application {
    inner: Arc<ApplicationInner>,
}

/// One running copy of an [`Application`].
pub struct Instance {
    app: Application,
    owner: execution::Owner,
    rest: BTreeMap<String, InteractionValue>,
    fields: BTreeMap<String, FieldState>,
    phase: Phase,
    dispatches: u64,
    policy: ResourcePolicy,
    viewport: (f64, f64),
    frame: Option<Frame>,
    layout_generations: u64,
    focused: Option<FocusedInput>,
    offsets: ScrollOffsets,
}

/// Builds a resolver that does not draw and does not grant host effects.
pub struct ResolverBuilder {
    viewport: (f64, f64),
    contract: Option<SemanticContract>,
    policy: ResourcePolicy,
    target: Option<Target>,
}

/// Headless resolver. Presentation, Caprichio selection, and D2 effects are
/// outside this contract.
pub struct Resolver {
    viewport: (f64, f64),
    contract: Option<SemanticContract>,
    policy: ResourcePolicy,
    target: Option<Target>,
}

impl ResolverBuilder {
    pub fn viewport(mut self, width_pt: f64, height_pt: f64) -> Self {
        self.viewport = (width_pt, height_pt);
        self
    }

    pub fn contract(mut self, contract: SemanticContract) -> Self {
        self.contract = Some(contract);
        self
    }

    pub fn resource_policy(mut self, policy: ResourcePolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Select a served platform. Exceptions for other platforms stay on the
    /// scene and are not applied. An exception for this platform is refused.
    /// The surface is not opened.
    pub fn target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    pub fn build(self) -> Result<Resolver, Diagnostic> {
        let (width, height) = self.viewport;
        if !width.is_finite()
            || !height.is_finite()
            || width <= 0.0
            || height <= 0.0
            || width > 32_768.0
            || height > 32_768.0
        {
            return Err(Diagnostic::new(
                "GY-RESOLVER-VIEWPORT",
                "viewport must be finite and within 1..=32768 points",
            ));
        }
        Ok(Resolver {
            viewport: self.viewport,
            contract: self.contract,
            policy: self.policy,
            target: self.target,
        })
    }
}

impl Resolver {
    pub fn builder() -> ResolverBuilder {
        ResolverBuilder {
            viewport: (800.0, 600.0),
            contract: None,
            policy: ResourcePolicy::default(),
            target: None,
        }
    }

    pub fn load(&self, source: &str) -> Result<Application, Diagnostic> {
        if source.len() > self.policy.max_source_bytes {
            return Err(Diagnostic::new(
                "GY-RESOLVER-BUDGET",
                format!(
                    "source is {} bytes; the limit is {}",
                    source.len(),
                    self.policy.max_source_bytes
                ),
            ));
        }
        let admitted = admission::admit(source, self.contract, self.target)?;
        let prepared = execution::prepare(
            &admitted.scene,
            &admitted.state,
            &admitted.bindings,
            &admitted.input_keys,
            &admitted.state_uses,
        )?;
        let lowered = match goya_lang::lower(&admitted.scene, self.viewport.0, self.viewport.1) {
            Ok(lowered) => Some(lowered),
            Err(goya_lang::LowerError::UnsupportedGesture) => None,
            Err(error) => {
                return Err(Diagnostic::new("GY-RESOLVER-LOWER", error.to_string()));
            }
        };
        let mut intern = representation::Interner::default();
        let components = representation::index_components(&admitted.scene.root, &mut intern);
        let displays = dependencies::display_index(&admitted.scene.root);
        Ok(Application {
            inner: Arc::new(ApplicationInner {
                contract: admitted.contract,
                scene: admitted.scene,
                digest: admitted.digest,
                lowered,
                intern,
                components,
                displays,
                prepared,
                target: self.target,
            }),
        })
    }

    pub fn load_path(&self, path: &Path) -> Result<Application, Diagnostic> {
        let source = fs::read_to_string(path).map_err(|error| {
            Diagnostic::new(
                "GY-RESOLVER-IO",
                format!("could not read {}: {error}", path.display()),
            )
        })?;
        self.load(&source)
    }

    pub fn instantiate(&self, application: &Application) -> Result<Instance, Diagnostic> {
        if let Some(reason) = application.execution_refusal() {
            return Err(Diagnostic::new("GY-RESOLVER-D2", reason.to_owned()));
        }
        let mut fields = BTreeMap::new();
        for seed in &application.inner.prepared.fields {
            fields.insert(
                seed.id.clone(),
                FieldState {
                    text: seed.text.clone(),
                    max_chars: seed.max_chars,
                    range: seed.range,
                },
            );
        }
        Ok(Instance {
            owner: application.inner.prepared.owner.clone(),
            rest: application.inner.prepared.rest.clone(),
            fields,
            phase: Phase::Initialising,
            dispatches: 0,
            policy: self.policy.clone(),
            viewport: self.viewport,
            frame: None,
            layout_generations: 0,
            focused: None,
            offsets: ScrollOffsets::default(),
            app: application.clone(),
        })
    }
}

impl Application {
    pub fn semantic_contract(&self) -> &str {
        self.inner.contract
    }

    /// Platform selected at load. `None` is the headless resolver.
    pub fn target(&self) -> Option<Target> {
        self.inner.target
    }

    pub fn source_digest(&self) -> [u8; 32] {
        self.inner.digest
    }

    pub fn scene(&self) -> &Scene {
        &self.inner.scene
    }

    /// `None` when GY-IR cannot represent a gesture policy that the scene still carries.
    pub fn lowered(&self) -> Option<&Lowered> {
        self.inner.lowered.as_ref()
    }

    pub fn ir_lowered(&self) -> bool {
        self.inner.lowered.is_some()
    }

    pub fn execution_refusal(&self) -> Option<&str> {
        self.inner.prepared.refusal.as_deref()
    }

    pub fn actions(&self) -> Vec<ActionInfo> {
        self.inner
            .prepared
            .actions
            .iter()
            .map(|action| ActionInfo {
                id: action.authored_id.clone(),
                name: action.name.clone(),
                trigger: action.trigger.clone(),
                class_name: action.class_name,
            })
            .collect()
    }

    pub fn components(&self) -> Vec<Component<'_>> {
        self.inner
            .components
            .iter()
            .map(|component| Component {
                id: component.id,
                name: self.inner.intern.get(component.name),
                semantic_id: component.semantic_id.map(|id| self.inner.intern.get(id)),
                kind: component.kind,
            })
            .collect()
    }

    /// Element ids whose `text-from` binding reads `key`, in source order.
    pub fn dependents(&self, key: &str) -> &[String] {
        self.inner.displays.element_ids(key)
    }
}

impl Instance {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn revision(&self) -> u64 {
        self.owner.revision()
    }

    pub fn semantic_contract(&self) -> &str {
        self.app.semantic_contract()
    }

    pub fn target(&self) -> Option<Target> {
        self.app.target()
    }

    pub fn actions(&self) -> Vec<ActionInfo> {
        self.app.actions()
    }

    pub fn execution_refusal(&self) -> Option<&str> {
        self.app.execution_refusal()
    }

    pub fn state(&self) -> Vec<ObservedState> {
        let values = self.observed();
        self.app
            .inner
            .prepared
            .facts
            .iter()
            .filter_map(|fact| {
                values.get(&fact.key).map(|value| ObservedState {
                    key: fact.key.clone(),
                    value: value.clone(),
                    lifetime: fact.lifetime,
                })
            })
            .collect()
    }

    pub fn field(&self, element_id: &str) -> Option<&str> {
        self.fields.get(element_id).map(|field| field.text.as_str())
    }

    pub fn fields(&self) -> Vec<(String, String)> {
        self.fields
            .iter()
            .map(|(id, field)| (id.clone(), field.text.clone()))
            .collect()
    }

    pub fn ir_lowered(&self) -> bool {
        self.app.ir_lowered()
    }

    pub fn set_field(&mut self, element_id: &str, text: &str) -> Result<(), Diagnostic> {
        self.require(&[Phase::Initialising, Phase::Running], "set a field")?;
        let Some(field) = self.fields.get_mut(element_id) else {
            return Err(Diagnostic::new(
                "GY-RESOLVER-FIELD",
                format!("unknown field {element_id}"),
            ));
        };
        if exceeds(field.max_chars, text) {
            return Err(located(
                "GY-RESOLVER-FIELD",
                format!("field {element_id} exceeds its max-length"),
                field.range,
            ));
        }
        field.text = text.to_owned();
        if let Some(focused) = self.focused.as_mut() {
            if focused.id.as_deref() == Some(element_id) {
                focused.caret = text.len();
            }
        }
        Ok(())
    }

    pub fn displays(&self) -> Vec<SceneDisplayValue> {
        scene_display_values(&self.app.scene().root, &self.observed())
    }

    /// Geometry and the text a host would draw. Field text and `text-from`
    /// values replace the authored strings. This does not open a surface.
    pub fn project(&mut self) -> Result<Vec<ProjectedNode>, Diagnostic> {
        let mut fields = BTreeMap::new();
        for (id, text) in self.fields() {
            fields.insert(id, text);
        }
        let displays = self.displays();
        let offsets = self.offsets.clone();
        let drawn = self.frame().scrolled_nodes(&offsets);
        let mut nodes = project::nodes(&self.app.scene().root, &drawn, &displays, &fields)?;
        if let Some(focused) = &self.focused {
            if let Some(id) = &focused.id {
                let caret = focused.caret;
                if let Some(node) = nodes
                    .iter_mut()
                    .rev()
                    .find(|node| node.role == "input" && node.id.as_deref() == Some(id.as_str()))
                {
                    node.caret = Some(caret);
                }
            }
        }
        Ok(nodes)
    }

    /// Layout reads authored geometry and the viewport. A D1 revision does not
    /// invalidate the frame.
    pub fn frame(&mut self) -> &Frame {
        if self.frame.is_none() {
            let laid = goya_lang::layout(self.app.scene(), self.viewport.0, self.viewport.1);
            self.offsets.reconcile(&laid);
            self.layout_generations += 1;
            self.frame = Some(laid);
        }
        self.frame.as_ref().expect("layout is stored")
    }

    pub fn layout_generations(&self) -> u64 {
        self.layout_generations
    }

    /// Hit-test in millipoints and either focus an input or activate a button.
    /// Uses `goya_lang::hit_test_scrolled` with this instance's scroll offsets.
    /// A miss leaves the revision unchanged.
    /// Focusing an input records it for [`Self::enter`] and does not edit the field.
    pub fn point(&mut self, x_mp: i64, y_mp: i64) -> Result<PointOutcome, Diagnostic> {
        self.require(&[Phase::Running], "point")?;
        let offsets = self.offsets.clone();
        let hit = {
            let frame = self.frame();
            goya_lang::hit_test_scrolled(frame, &offsets, x_mp, y_mp)
        };
        let Some(name) = hit else {
            self.focused = None;
            return Ok(PointOutcome::Miss);
        };
        let Some(node) = self
            .project()?
            .into_iter()
            .rev()
            .find(|node| node.name == name)
        else {
            return Err(Diagnostic::new(
                "GY-RESOLVER-PRESENT",
                format!("hit {name} is not on the projected surface"),
            ));
        };
        match node.role {
            "button" => {
                let outcome = self.dispatch(&Event::Activate {
                    action: format!("button.{name}.activate"),
                })?;
                self.focused = None;
                Ok(PointOutcome::Activate { name, outcome })
            }
            "input" => {
                let caret = node
                    .id
                    .as_deref()
                    .and_then(|id| self.field(id))
                    .map(str::len)
                    .unwrap_or(0);
                self.focused = Some(FocusedInput {
                    name: name.clone(),
                    id: node.id.clone(),
                    caret,
                });
                Ok(PointOutcome::Focus { name, id: node.id })
            }
            _ => {
                self.focused = None;
                Ok(PointOutcome::Miss)
            }
        }
    }

    /// Replace the text of the input last focused by [`Self::point`].
    /// The caret moves to the end of the replacement. The revision does not change.
    pub fn enter(&mut self, text: &str) -> Result<String, Diagnostic> {
        self.require(&[Phase::Running], "enter text")?;
        let Some(focused) = self.focused.as_ref() else {
            return Err(Diagnostic::new("GY-RESOLVER-FIELD", "no input is focused"));
        };
        let Some(id) = focused.id.clone() else {
            return Err(Diagnostic::new(
                "GY-RESOLVER-FIELD",
                format!("focused input {} has no field id", focused.name),
            ));
        };
        self.set_field(&id, text)?;
        Ok(id)
    }

    /// Edit the focused field at its grapheme caret.
    /// A rejected write leaves the text and the caret in place.
    /// The revision does not change.
    pub fn edit(&mut self, edit: Edit) -> Result<CaretPlace, Diagnostic> {
        self.require(&[Phase::Running], "edit")?;
        let Some(focused) = self.focused.clone() else {
            return Err(Diagnostic::new("GY-RESOLVER-FIELD", "no input is focused"));
        };
        let Some(id) = focused.id.clone() else {
            return Err(Diagnostic::new(
                "GY-RESOLVER-FIELD",
                format!("focused input {} has no field id", focused.name),
            ));
        };
        let text = self.field(&id).unwrap_or("").to_owned();
        let (next, byte) = caret::apply(&text, focused.caret, edit);
        if let Some(next) = next {
            self.set_field(&id, &next)?;
        }
        if let Some(focused) = self.focused.as_mut() {
            focused.caret = byte;
        }
        Ok(CaretPlace { id, byte })
    }

    /// Scroll the viewport under a point by millipoints.
    /// Uses `Frame::scroll_target_at` and `ScrollOffsets::scroll_by`.
    /// The offset is clamped. Focus and the revision stay.
    pub fn scroll(
        &mut self,
        x_mp: i64,
        y_mp: i64,
        dx_mp: i64,
        dy_mp: i64,
    ) -> Result<ScrollOutcome, Diagnostic> {
        self.require(&[Phase::Running], "scroll")?;
        let frame = self.frame().clone();
        let Some(name) = frame.scroll_target_at(&self.offsets, x_mp, y_mp) else {
            return Ok(ScrollOutcome::Miss);
        };
        if !self.offsets.scroll_by(&frame, &name, dx_mp, dy_mp) {
            return Ok(ScrollOutcome::Miss);
        }
        let (x_mp, y_mp) = self.offsets.get(&name);
        Ok(ScrollOutcome::Moved { name, x_mp, y_mp })
    }

    pub fn start(&mut self) -> Result<(), Diagnostic> {
        if self.phase != Phase::Initialising {
            return Err(self.phase_error("start"));
        }
        self.phase = Phase::Running;
        Ok(())
    }

    pub fn dispatch(&mut self, event: &Event) -> Result<DispatchOutcome, Diagnostic> {
        let Event::Activate { action } = event;
        self.activate(action)
    }

    pub fn activate(&mut self, action: &str) -> Result<DispatchOutcome, Diagnostic> {
        self.require(&[Phase::Running], "dispatch")?;
        if self.dispatches >= self.policy.max_dispatches {
            return self.exhaust();
        }
        let (authored_id, writes) = {
            let resolved = self.lookup(action)?;
            (resolved.authored_id.clone(), resolved.writes.clone())
        };
        let inputs = self.read_inputs(&authored_id)?;
        let before = self.observed();
        self.dispatches += 1;
        self.owner.dispatch(&authored_id, inputs)?;
        let after = self.observed();
        let propagated =
            dependencies::propagate(&self.app.inner.displays, &writes, &before, &after);
        Ok(DispatchOutcome {
            revision: self.owner.revision(),
            announcement: propagated.announcement,
            affected_elements: propagated.affected_elements,
            sites_evaluated: propagated.sites_evaluated,
        })
    }

    pub fn suspend(&mut self) -> Result<(), Diagnostic> {
        if self.phase != Phase::Running {
            return Err(self.phase_error("suspend"));
        }
        self.phase = Phase::Suspended;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<(), Diagnostic> {
        if self.phase != Phase::Suspended {
            return Err(self.phase_error("resume"));
        }
        self.phase = Phase::Running;
        Ok(())
    }

    pub fn terminate(&mut self) -> Result<(), Diagnostic> {
        match self.phase {
            Phase::Initialising | Phase::Running | Phase::Suspended | Phase::Failed => {
                self.phase = Phase::Terminated;
                Ok(())
            }
            Phase::Terminated => Err(self.phase_error("terminate")),
        }
    }

    /// Replace the admitted scene. A failure leaves this instance unchanged.
    pub fn reload(&mut self, source: &str) -> Result<ReloadReport, Diagnostic> {
        self.require(
            &[Phase::Initialising, Phase::Running, Phase::Suspended],
            "reload",
        )?;
        if source.len() > self.policy.max_source_bytes {
            return Err(Diagnostic::new(
                "GY-RESOLVER-BUDGET",
                format!(
                    "source is {} bytes; the limit is {}",
                    source.len(),
                    self.policy.max_source_bytes
                ),
            ));
        }
        let Some(contract) = SemanticContract::parse(self.app.semantic_contract()) else {
            return Err(Diagnostic::new(
                "GY-RESOLVER-RELOAD",
                "running contract cannot be selected again",
            ));
        };
        let admitted = admission::admit(source, Some(contract), self.app.target())?;
        let prepared = execution::prepare(
            &admitted.scene,
            &admitted.state,
            &admitted.bindings,
            &admitted.input_keys,
            &admitted.state_uses,
        )?;
        if let Some(reason) = prepared.refusal.as_deref() {
            return Err(Diagnostic::new("GY-RESOLVER-D2", reason.to_owned()));
        }
        let lowered = match goya_lang::lower(&admitted.scene, self.viewport.0, self.viewport.1) {
            Ok(lowered) => Some(lowered),
            Err(goya_lang::LowerError::UnsupportedGesture) => None,
            Err(error) => {
                return Err(Diagnostic::new("GY-RESOLVER-LOWER", error.to_string()));
            }
        };
        let mut next = prepared.rest.clone();
        for (key, value) in prepared.owner.values() {
            next.insert(key.clone(), execution::from_d1(value));
        }
        let previous = self.observed();
        let mut spans = BTreeMap::new();
        for decl in &admitted.state {
            spans.insert(decl.key.clone(), decl.range);
        }
        let class = reload::classify(&previous, &next, &spans)?;
        let (fields, preserved_fields) = reload_fields(&prepared.fields, &self.fields)?;
        let mut owner = prepared.owner.clone();
        let mut d1 = BTreeMap::new();
        let mut rest = BTreeMap::new();
        for (key, value) in class.values {
            if prepared.owner.values().contains_key(&key) {
                d1.insert(key, execution::to_d1(&value));
            } else {
                rest.insert(key, value);
            }
        }
        owner.restore(d1, self.owner.revision())?;
        let mut intern = representation::Interner::default();
        let components = representation::index_components(&admitted.scene.root, &mut intern);
        let displays = dependencies::display_index(&admitted.scene.root);
        let target = self.app.target();
        self.app = Application {
            inner: Arc::new(ApplicationInner {
                contract: admitted.contract,
                scene: admitted.scene,
                digest: admitted.digest,
                lowered,
                intern,
                components,
                displays,
                prepared,
                target,
            }),
        };
        self.owner = owner;
        self.rest = rest;
        self.fields = fields;
        let focused_survives = self
            .focused
            .as_ref()
            .and_then(|focused| focused.id.as_deref())
            .is_some_and(|id| self.fields.contains_key(id));
        if !focused_survives {
            self.focused = None;
        } else if let Some(focused) = self.focused.as_mut() {
            if let Some(id) = focused.id.as_deref() {
                if let Some(field) = self.fields.get(id) {
                    focused.caret = caret::clamp(&field.text, focused.caret);
                }
            }
        }
        self.frame = None;
        Ok(ReloadReport {
            preserved_keys: sorted_keys(class.preserved_keys),
            initialised_keys: sorted_keys(class.initialised_keys),
            dropped_keys: sorted_keys(class.dropped_keys),
            preserved_fields,
        })
    }

    fn lookup(&self, token: &str) -> Result<&execution::ActionDesc, Diagnostic> {
        let actions = &self.app.inner.prepared.actions;
        if let Some(found) = actions.iter().find(|action| action.authored_id == token) {
            return Ok(found);
        }
        // The authored trigger is `button.<name>.activate`. The scene stores
        // only `<name>`, which is the button's structural name.
        let button = token
            .strip_prefix("button.")
            .and_then(|rest| rest.strip_suffix(".activate"))
            .filter(|name| !name.is_empty());
        if let Some(found) = unique(
            actions
                .iter()
                .filter(|action| Some(action.trigger.as_str()) == button),
        ) {
            return Ok(found);
        }
        if let Some(found) = unique(actions.iter().filter(|action| action.name == token)) {
            return Ok(found);
        }
        if let Some(found) = unique(actions.iter().filter(|action| action.trigger == token)) {
            return Ok(found);
        }
        Err(Diagnostic::new(
            "GY-RESOLVER-ACTION",
            format!("unknown action {token}"),
        ))
    }

    fn read_inputs(
        &self,
        action_id: &str,
    ) -> Result<BTreeMap<String, goya_executable::D1Value>, Diagnostic> {
        let action = self
            .app
            .inner
            .prepared
            .actions
            .iter()
            .find(|action| action.authored_id == action_id)
            .ok_or_else(|| {
                Diagnostic::new("GY-RESOLVER-ACTION", format!("unknown action {action_id}"))
            })?;
        let mut inputs = BTreeMap::new();
        for input in &action.inputs {
            let field = self.fields.get(&input.field_id).ok_or_else(|| {
                located(
                    "GY-RESOLVER-FIELD",
                    format!("missing field {}", input.field_id),
                    input.range,
                )
            })?;
            let range = field.range.or(input.range);
            if exceeds(field.max_chars, &field.text) {
                return Err(located(
                    "GY-RESOLVER-FIELD",
                    format!("field {} exceeds its max-length", input.field_id),
                    range,
                ));
            }
            let value = convert_field(input.value_type, &field.text).map_err(|error| {
                located(
                    "GY-RESOLVER-FIELD",
                    format!("field {}: {error}", input.field_id),
                    range,
                )
            })?;
            inputs.insert(input.name.clone(), execution::to_d1(&value));
        }
        Ok(inputs)
    }

    fn observed(&self) -> BTreeMap<String, InteractionValue> {
        let mut values = self.rest.clone();
        for (key, value) in self.owner.values() {
            values.insert(key.clone(), execution::from_d1(value));
        }
        values
    }

    fn exhaust(&mut self) -> Result<DispatchOutcome, Diagnostic> {
        let diagnostic = Diagnostic::new(
            "GY-RESOLVER-BUDGET",
            format!(
                "dispatch budget of {} is exhausted",
                self.policy.max_dispatches
            ),
        );
        match self.policy.on_dispatch_exhaustion {
            Exhaustion::Refuse => {}
            Exhaustion::Suspend => self.phase = Phase::Suspended,
            Exhaustion::Terminate => self.phase = Phase::Terminated,
            Exhaustion::Fail => self.phase = Phase::Failed,
        }
        Err(diagnostic)
    }

    fn require(&self, allowed: &[Phase], action: &str) -> Result<(), Diagnostic> {
        if allowed.contains(&self.phase) {
            Ok(())
        } else {
            Err(self.phase_error(action))
        }
    }

    fn phase_error(&self, action: &str) -> Diagnostic {
        Diagnostic::new(
            "GY-RESOLVER-PHASE",
            format!("cannot {action} while {}", self.phase.as_str()),
        )
    }
}

fn unique<'a>(
    mut matches: impl Iterator<Item = &'a execution::ActionDesc>,
) -> Option<&'a execution::ActionDesc> {
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn exceeds(max_chars: Option<i64>, text: &str) -> bool {
    match max_chars {
        Some(limit) if limit < 0 => true,
        Some(limit) => text.chars().count() as i64 > limit,
        None => false,
    }
}

fn reload_fields(
    seeds: &[execution::FieldSeed],
    current: &BTreeMap<String, FieldState>,
) -> Result<(BTreeMap<String, FieldState>, Vec<String>), Diagnostic> {
    let mut fields = BTreeMap::new();
    let mut preserved_fields = Vec::new();
    for seed in seeds {
        let text = if let Some(existing) = current.get(&seed.id) {
            if exceeds(seed.max_chars, &existing.text) {
                return Err(located(
                    "GY-RESOLVER-FIELD",
                    format!("field {} exceeds its max-length", seed.id),
                    seed.range,
                ));
            }
            preserved_fields.push(seed.id.clone());
            existing.text.clone()
        } else {
            seed.text.clone()
        };
        fields.insert(
            seed.id.clone(),
            FieldState {
                text,
                max_chars: seed.max_chars,
                range: seed.range,
            },
        );
    }
    preserved_fields.sort();
    Ok((fields, preserved_fields))
}

fn sorted_keys(mut keys: Vec<String>) -> Vec<String> {
    keys.sort();
    keys
}
