//! Scene AST and deterministic absolute/proportional layout.

use goya_chromatics::{Risk, Stability, Strategy};
use goya_determinism::EffectKind;
use std::collections::BTreeMap;

/// Contract millipoint scale: 1 point = 1000 mp (matches GY-LAYOUT/1).
pub const MP: f64 = 1000.0;

/// 3D vector of f64 components.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Cell,
    Button,
    /// `@input` — editable text field (placeholder in `text`).
    Input,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

/// A `present-when` test on one declared state key (live profiles).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition {
    pub key: String,
    pub op: ConditionOp,
    pub value: InteractionValue,
}

/// How a `present-when` condition compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConditionOp {
    /// `is`: equal to the literal (any type).
    #[default]
    Is,
    /// `at-least`: an int key greater than or equal to the literal.
    AtLeast,
}

/// A scene element: RUGID-style cell or button.
#[derive(Debug, Clone)]
pub struct SceneCell {
    pub name: String,
    /// Optional authored identity. Strict handoff admission requires this to
    /// be present and unique; display names and tree positions are not IDs.
    pub authored_id: Option<String>,
    pub kind: CellKind,
    /// True for `@input` cells (mirrors [`CellKind::Input`]).
    pub is_input: bool,
    /// Input field properties: placeholder text and max character limit.
    pub placeholder: Option<String>,
    /// GSSH/5 static phrasing for `placeholder`. When set, `placeholder`
    /// stays `None` for the same reason as [`SceneCell::text_phrasing`].
    pub placeholder_phrasing: Option<Phrasing>,
    pub max_length: Option<i64>,
    /// Absolute position in points, parent-origin-relative.
    pub position: Option<Vec3>,
    /// Absolute extent in points.
    pub extent: Option<Vec3>,
    /// Proportional position (fraction of parent extent).
    pub position_p: Option<Vec3>,
    /// Proportional extent (fraction of parent extent).
    pub extent_p: Option<Vec3>,
    /// `text-size-p`: text size as a fraction of the element's own resolved
    /// height, at least 0; above 1 the text is taller than its element and
    /// is clipped like any overflowing text (GSSH/3).
    pub text_size_p: Option<f64>,
    /// `stroke-width-p`: stroke width as a fraction of the shorter side of
    /// the element's resolved rect, clamped to [0, 0.5] (GSSH/3).
    pub stroke_width_p: Option<f64>,
    /// Fill colour as 0xRRGGBB (or 0x00RRGGBB for #rgb expansion).
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    pub stroke_width: Option<f64>,
    pub text: Option<String>,
    /// GSSH/5 static phrasing for `text`. When set, `text` stays `None`:
    /// layout does not choose a class, and the adopting compiler wraps the
    /// sentence [`Phrasing::phrase`] returns for its class.
    pub text_phrasing: Option<Phrasing>,
    pub text_size: Option<f64>,
    pub anchor: Option<Anchor>,
    /// `wrap: <positive integer columns>` — greedy word-wrap width applied
    /// at layout time. Absent = no wrap.
    pub wrap: Option<u32>,
    /// Enable a clipped, user-scrollable text viewport in interactive previews.
    pub scroll: bool,
    /// Element clipboard decisions. `None` means the element authors no policy.
    /// This is not the experimental scope resolver in [`crate::gestures`].
    pub gestures: Option<GesturePolicy>,
    /// `text-from: state.key;` shows the current value of a declared state
    /// key as the element's text (GSSH/3 live admission only).
    pub text_from: Option<String>,
    /// `present-when: state.key is <literal>;` (or `at-least <int>`) makes
    /// the element and its subtree present only while the condition holds
    /// (live profiles only). Nested conditions conjoin.
    pub present_when: Option<Condition>,
    /// `fill-to: #rrggbb;` with `fill-period-s`: the fill breathes between
    /// `fill` and this colour (live profiles only).
    pub fill_to: Option<u32>,
    /// Seconds for one `fill` → `fill-to` transition.
    pub fill_period_s: Option<f64>,
    /// `appear-s: N;` the subtree fades in over N seconds whenever it
    /// becomes present (and at launch if present then).
    pub appear_s: Option<f64>,
    /// `appear-from-p: (fx, fy, 0);` with `appear-s`: it also slides in from
    /// this offset, as fractions of the parent's resolved extent.
    pub appear_from_p: Option<Vec3>,
    /// `@draw graphic { ... }`: the element draws a declared `@graphic` in
    /// the state its rules choose (live profiles only).
    pub draw: Option<crate::graphic::DrawDecl>,
    /// `@iden { ... }`: the element's accessible identity (GSSH/4, decision
    /// D14). Required on every `@input` under GSSH/4.
    pub iden: Option<Iden>,
    pub children: Vec<SceneCell>,
}

/// Accessible identity of one element (`@iden`, GSSH/4, decision D14).
///
/// Each target maps it to its native accessibility features: `label` is the
/// accessible name, `hint` the accessible description. The struct is
/// non-exhaustive so later profiles can add fields without breaking
/// consumers; keys those profiles will define are already reserved.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Iden {
    /// Accessible name. Required. Empty when [`Self::label_phrasing`] carries
    /// the GSSH/5 sentences; GSSH/4 stores the authored string here.
    pub label: String,
    /// GSSH/5 phrasing of the accessible name.
    pub label_phrasing: Option<Phrasing>,
    /// Accessible description: what the element is for or how to use it.
    pub hint: Option<String>,
    /// GSSH/5 phrasing of the accessible description.
    pub hint_phrasing: Option<Phrasing>,
    /// Reserved keys the author wrote, with their source ranges. Strict
    /// admission refuses them, so nothing reserved is silently dropped.
    pub reserved: Vec<(String, crate::parser::SourceRange)>,
}

impl Iden {
    pub fn new(label: String, hint: Option<String>) -> Self {
        Self {
            label,
            hint,
            label_phrasing: None,
            hint_phrasing: None,
            reserved: Vec::new(),
        }
    }
}

/// Platform class a static sentence is written for (GSSH/5, decision D17).
///
/// These are not `@platform` names. `@web` is a browser page. `@desktop` is
/// a native window. `@mobile` is a touch phone or tablet. `@tv` is a remote,
/// including webOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhrasingClass {
    Desktop,
    Mobile,
    Tv,
    Web,
}

impl PhrasingClass {
    /// Desktop, mobile, television, web. Completeness uses this order.
    pub const ALL: [PhrasingClass; 4] = [
        PhrasingClass::Desktop,
        PhrasingClass::Mobile,
        PhrasingClass::Tv,
        PhrasingClass::Web,
    ];

    /// Source specifier, including the `@`.
    pub fn specifier(self) -> &'static str {
        match self {
            Self::Desktop => "@desktop",
            Self::Mobile => "@mobile",
            Self::Tv => "@tv",
            Self::Web => "@web",
        }
    }
}

/// Largest `text` or `placeholder` phrase, in UTF-8 bytes.
/// Labels and hints keep the `@iden` limits.
pub const MAX_PHRASE_TEXT_BYTES: usize = 16 * 1024;

/// Static sentences for one phrasing site (GSSH/5, decision D17).
///
/// [`Self::phrase`] returns that class's sentence, or `@universal` when the
/// class has none of its own. A compiler shows that sentence unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrasing {
    pub universal: Option<String>,
    pub desktop: Option<String>,
    pub mobile: Option<String>,
    pub tv: Option<String>,
    pub web: Option<String>,
    /// Source range of the phrasing value.
    pub range: crate::parser::SourceRange,
}

impl Phrasing {
    /// The sentence for `class`: its own override, otherwise `@universal`.
    pub fn phrase(&self, class: PhrasingClass) -> Option<&str> {
        let own = match class {
            PhrasingClass::Desktop => &self.desktop,
            PhrasingClass::Mobile => &self.mobile,
            PhrasingClass::Tv => &self.tv,
            PhrasingClass::Web => &self.web,
        };
        own.as_deref().or(self.universal.as_deref())
    }

    /// Every class resolves to a sentence.
    pub fn complete(&self) -> bool {
        self.missing_class().is_none()
    }

    /// The first class, in [`PhrasingClass::ALL`] order, with neither its
    /// own sentence nor `@universal`.
    pub fn missing_class(&self) -> Option<PhrasingClass> {
        PhrasingClass::ALL
            .into_iter()
            .find(|class| self.phrase(*class).is_none())
    }
}

/// One explicit allow or deny. An omitted operation is not a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GestureDecision {
    Allow,
    Deny,
}

/// Element-level interaction gesture decisions.
///
/// Each field is `None` when the block does not mention that operation.
/// Scopes, hard denial, and predicates are not represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GesturePolicy {
    /// Copy operation gesture decision.
    pub copy: Option<GestureDecision>,
    /// Cut operation gesture decision.
    pub cut: Option<GestureDecision>,
    /// Paste operation gesture decision.
    pub paste: Option<GestureDecision>,
}

/// A parsed scene document.
#[derive(Debug, Clone)]
pub struct Scene {
    pub name: String,
    pub root: SceneCell,
    /// Authored, bounded interaction programs. Visual layout remains separate.
    pub interactions: Vec<InteractionDecl>,
    /// Experimental `@intent` declarations (Quitasol phase 0; proposed, not
    /// ratified). Empty unless the source authors them. Validated by
    /// [`crate::intents`], not by layout.
    pub intents: Vec<IntentDecl>,
    /// Draft `@chromatics` declaration (GY-CHROMATICS/0.1 draft). At most
    /// one per scene. Validated by [`crate::chromatics`], not by layout;
    /// refused by every strict admission until ratified.
    pub chromatics: Option<ChromaticsDecl>,
}

/// One experimental `@intent` declaration (Quitasol phase 0).
///
/// A voice-addressable action binding: the named interaction to invoke, the
/// natural-language utterances that may address it under the pinned utterance
/// grammar v0, whether invocation requires explicit confirmation, and the
/// required non-voice fallback. See docs/quitasol/intents.md. Not admitted by
/// any strict profile and not ratified; the shape may change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDecl {
    pub name: String,
    /// Authored identity for stable intent references (optional).
    pub authored_id: Option<String>,
    /// The `@interaction` this intent invokes.
    pub action: String,
    /// Utterance patterns with `{slot}` placeholders, grammar v0.
    pub utterances: Vec<String>,
    /// Whether invocation requires explicit authorisation at runtime.
    /// Recorded only; no runtime confirmation is implemented.
    pub confirmation: bool,
    /// Required non-voice equivalent: a remote-navigable target.
    pub fallback_remote: Option<String>,
    /// Required non-voice equivalent: a keyboard-addressable target.
    pub fallback_keyboard: Option<String>,
    /// Optional Media Session transport class (Quitasol Q2 draft surface):
    /// the documented Fire OS voice transport behaviour this intent
    /// represents. `None` means the intent declares no transport lowering.
    /// See docs/quitasol/q0-capability-audit.md — the Media Session path is
    /// the only self-serve Fire OS voice integration.
    pub transport: Option<TransportClass>,
    /// Typed slots (Quitasol Q4 draft surface): every `{slot}` placeholder
    /// used in any utterance must be declared here, and every declared slot
    /// must be used in at least one utterance (validated in
    /// [`crate::intents`], fail-closed). Slot text syntax is unchanged from
    /// grammar v0; typing lives in this declaration.
    pub slots: Vec<TypedSlot>,
    /// Semantic category (Quitasol Q4 draft surface): the closed
    /// classification the Semantic Intent Compiler records for this intent.
    /// Defaults to [`IntentCategory::General`] when not authored.
    pub category: IntentCategory,
    /// Source range of the whole declaration.
    pub range: crate::parser::SourceRange,
}

/// One declared, typed intent slot: `{name}` in utterances, typed with the
/// D1 scalar schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedSlot {
    pub name: String,
    pub value_type: InteractionType,
}

/// Closed semantic classification of an intent (Q4 draft). Mirrors the
/// conventional-to-voice equivalence table in the Quitasol proposal:
/// search, playback, navigation, selection, settings, confirmation, and a
/// default bucket. A future category is a new keyword and a contract
/// revision, never a reinterpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntentCategory {
    Search,
    Playback,
    Navigation,
    Selection,
    Settings,
    Confirm,
    #[default]
    General,
}

impl IntentCategory {
    /// Canonical authored keyword for this category.
    pub fn keyword(self) -> &'static str {
        match self {
            IntentCategory::Search => "search",
            IntentCategory::Playback => "playback",
            IntentCategory::Navigation => "navigation",
            IntentCategory::Selection => "selection",
            IntentCategory::Settings => "settings",
            IntentCategory::Confirm => "confirm",
            IntentCategory::General => "general",
        }
    }

    /// Parse an authored keyword. Returns `None` for unknown keywords;
    /// callers turn that into a parse error (fail-closed).
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "search" => IntentCategory::Search,
            "playback" => IntentCategory::Playback,
            "navigation" => IntentCategory::Navigation,
            "selection" => IntentCategory::Selection,
            "settings" => IntentCategory::Settings,
            "confirm" => IntentCategory::Confirm,
            "general" => IntentCategory::General,
            _ => return None,
        })
    }
}

/// The Media Session transport behaviours Amazon documents for Fire OS
/// voice control (Quitasol Q2 draft). Absolute-time seek is deliberately
/// absent: the Q0 audit records that Fire OS Media Session voice seeks do
/// not support absolute time, only offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportClass {
    /// Play or pause playback.
    PlayPause,
    /// Skip to the next item.
    SkipNext,
    /// Skip to the previous item.
    SkipPrevious,
    /// Seek forward by an offset (voice supplies the offset).
    SeekForward,
    /// Seek backward by an offset (voice supplies the offset).
    SeekBackward,
    /// Restart the current item from the beginning.
    Restart,
}

impl TransportClass {
    /// Canonical authored keyword for this class.
    pub fn keyword(self) -> &'static str {
        match self {
            TransportClass::PlayPause => "play_pause",
            TransportClass::SkipNext => "skip_next",
            TransportClass::SkipPrevious => "skip_previous",
            TransportClass::SeekForward => "seek_forward",
            TransportClass::SeekBackward => "seek_backward",
            TransportClass::Restart => "restart",
        }
    }

    /// Parse an authored keyword. Returns `None` for unknown keywords;
    /// callers turn that into a parse error (fail-closed).
    pub fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "play_pause" => TransportClass::PlayPause,
            "skip_next" => TransportClass::SkipNext,
            "skip_previous" => TransportClass::SkipPrevious,
            "seek_forward" => TransportClass::SeekForward,
            "seek_backward" => TransportClass::SeekBackward,
            "restart" => TransportClass::Restart,
            _ => return None,
        })
    }
}

/// D1 scalar schema type for authored interaction contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionType {
    Int,
    Text,
    Bool,
}

impl InteractionType {
    /// Canonical authored keyword for this scalar type.
    pub fn keyword(self) -> &'static str {
        match self {
            InteractionType::Int => "int",
            InteractionType::Text => "text",
            InteractionType::Bool => "bool",
        }
    }
}

/// Authored execution class for a bounded interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractionClass {
    D1,
    D2,
}

/// Result type of a replayed static D2 effect request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectValueType {
    Int64,
    Text,
    Bool,
}

/// Static request bytes and their explicit declared capability label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectRequestDecl {
    pub name: String,
    pub effect_kind: EffectKind,
    pub capability: String,
    pub request_utf8: String,
    pub result_type: EffectValueType,
}

/// One explicitly declared contract key and its type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedKey {
    pub name: String,
    pub value_type: InteractionType,
}

/// Literal available to the deterministic transition subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionValue {
    Int(i64),
    Text(String),
    Bool(bool),
}

/// Supported typed expression AST. Addition is checked integer addition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InteractionExpr {
    Literal(InteractionValue),
    State(String),
    Input(String),
    Effect(String),
    Add(Box<InteractionExpr>, Box<InteractionExpr>),
}

/// One straight-line state assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateAssignment {
    pub key: String,
    pub value: InteractionExpr,
}

/// One draft `@chromatics` declaration (directive GOYA-INT-CHR-001 §12,
/// contract GY-CHROMATICS/0.1 draft).
///
/// Chromatics assigns the scene's scarce physical inputs to the semantic
/// actions most worth one press in this scene. The declaration is semantic:
/// strategy, eligibility and stability policy plus explicit candidates that
/// name declared `@interaction`s. It never names key codes; the target maps
/// abstract slot names (for example `red`) to its own hardware.
///
/// Quarantine surface like `@intent`: parsed and retained on
/// [`Scene::chromatics`], validated by [`crate::chromatics`], and refused by
/// every strict admission until the contract is ratified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromaticsDecl {
    /// Authored block name; the scene's chromatic scope label.
    pub name: String,
    /// Primary allocation strategy (§8).
    pub primary: Strategy,
    /// Fills slots the primary leaves empty; never repeats an entity (§8.1).
    pub fallback: Option<Strategy>,
    /// Keep a still-selected candidate on its previous slot (§10.4).
    pub preserve_slots: bool,
    /// Assign slots strictly in selection order (overrides preservation).
    pub ordered_slots: bool,
    /// Drop candidates a dedicated native input already covers (§3.3).
    /// Turning this off is the explicit accessibility accommodation.
    pub exclude_native: bool,
    /// At most one binding per entity.
    pub exclude_duplicates: bool,
    /// Chromatic Inertia thresholds (§10.3).
    pub stability: Stability,
    /// Explicitly declared candidates (§12.1). Bounded by the validator.
    pub candidates: Vec<ChromaticCandidate>,
    /// Source range of the whole declaration.
    pub range: crate::parser::SourceRange,
}

/// One explicitly declared Chromatic candidate (§7, §12.1).
///
/// All numeric fields are authored integers; allocation stays exact. The
/// runtime statistics fields (`last_used`, `uses`, `expires`) are not
/// authored: they belong to the target's runtime state, not the scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromaticCandidate {
    /// Stable semantic candidate id (§7). Never a position.
    pub name: String,
    /// The declared `@interaction` activation executes (§20: registered
    /// handlers only).
    pub intent: String,
    /// Text label. Required and non-empty: colour alone never identifies a
    /// binding (§21).
    pub label: String,
    /// Semantic symbol or marker shape for non-colour identification
    /// (§21). Optional; the target maps it to its own iconography.
    pub symbol: Option<String>,
    /// Accessible description of what the action is for (§21). Optional;
    /// supplementary to the required label.
    pub description: Option<String>,
    /// Associated entity, if entity-oriented (§7.1).
    pub entity: Option<String>,
    /// Consequence class (§20). Defaults to navigational.
    pub risk: Risk,
    /// The intent enters a confirmation workflow rather than acting.
    /// Required for sensitive, destructive and privileged candidates.
    pub confirmation: bool,
    /// Native-equivalence key a dedicated input may already cover (§3.3).
    pub native: Option<String>,
    /// Application ranking, 0 best.
    pub rank: Option<u32>,
    /// Favourite order, 0 best.
    pub favourite: Option<u32>,
    /// Visible position, 0 first.
    pub position: Option<u32>,
    /// Conventional inputs needed to reach and activate it (≥ 1). Defaults
    /// to 1 (§9 InteractionSavings).
    pub cost: Option<u32>,
    /// Estimated likelihood of need, per mille 0..=1000 (§9).
    pub probability: Option<u32>,
    /// Authored utility terms in milli-inputs (§9).
    pub contextual: Option<i64>,
    pub urgency: Option<i64>,
    pub preference: Option<i64>,
    /// Explicit slot reservation, by abstract slot name (Manual, §8).
    pub pinned: Option<String>,
    /// Source range of the whole candidate declaration.
    pub range: crate::parser::SourceRange,
}

/// A D1 transition explicitly bound to one button activation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionDecl {
    pub name: String,
    /// Optional authored identity for stable action references.
    pub authored_id: Option<String>,
    pub trigger_button: String,
    pub class: InteractionClass,
    pub declared_effects: Vec<EffectKind>,
    pub requests: Vec<EffectRequestDecl>,
    pub state_reads: Vec<TypedKey>,
    pub state_writes: Vec<TypedKey>,
    pub inputs: Vec<TypedKey>,
    pub assignments: Vec<StateAssignment>,
}

impl Scene {
    /// Depth-first search for a cell by name (structural, pre-layout).
    pub fn find_node(&self, name: &str) -> Option<&SceneCell> {
        fn walk<'a>(cell: &'a SceneCell, name: &str) -> Option<&'a SceneCell> {
            if cell.name == name {
                return Some(cell);
            }
            cell.children.iter().find_map(|c| walk(c, name))
        }
        walk(&self.root, name)
    }
}

/// Millipoint rect (matches GY-LAYOUT/1 semantics).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectMp {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

/// One resolved, drawable node.
#[derive(Debug, Clone)]
pub struct DrawNode {
    pub name: String,
    /// Structural parent; used by renderers that need clipping and overflow.
    pub parent: Option<String>,
    pub is_button: bool,
    /// Editable text field (`@input`); focus is host-side state.
    pub is_input: bool,
    /// Input field properties: placeholder text and max character limit.
    pub placeholder: Option<String>,
    pub max_length: Option<i64>,
    pub rect: RectMp,
    /// Effective screen clip from scrollable ancestors after applying offsets.
    pub clip: Option<RectMp>,
    pub fill: Option<u32>,
    pub stroke: Option<u32>,
    pub stroke_width: Option<f64>,
    pub text: Option<String>,
    pub text_size: Option<f64>,
    /// Resolved text size in millipoints: from `text-size-p` and the
    /// resolved height, else from absolute `text-size` points.
    pub text_size_mp: Option<i64>,
    /// Resolved stroke width in millipoints, by the same rule.
    pub stroke_width_mp: Option<i64>,
    pub anchor: Anchor,
    /// Whether the node's text viewport accepts scrolling.
    pub scroll: bool,
}

/// Layout output: draw order is depth-first (parents before children).
#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub nodes: Vec<DrawNode>,
    index: BTreeMap<String, usize>,
    /// Parent node index for each entry in `nodes`. Name lookup is not used
    /// for this walk: a descendant may reuse an ancestor's name.
    parent_index: Vec<Option<usize>>,
}

/// Per-scene scroll offsets in millipoints. Offsets are keyed by stable cell
/// name and are clamped to the content extent each time they are changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScrollOffsets {
    offsets: BTreeMap<String, (i64, i64)>,
}

impl ScrollOffsets {
    pub fn get(&self, name: &str) -> (i64, i64) {
        self.offsets.get(name).copied().unwrap_or((0, 0))
    }

    /// Scroll a viewport by millipoints. Returns false when `name` does not
    /// identify a scrollable node; otherwise stores the clamped new offset.
    pub fn scroll_by(&mut self, frame: &Frame, name: &str, dx: i64, dy: i64) -> bool {
        if !frame.find(name).is_some_and(|node| node.scroll) {
            return false;
        }
        let (max_x, max_y) = frame.scroll_limits(name);
        let (x, y) = self.get(name);
        let x = x.saturating_add(dx).clamp(0, max_x);
        let y = y.saturating_add(dy).clamp(0, max_y);
        self.offsets.insert(name.to_owned(), (x, y));
        true
    }

    /// Re-clamp stored offsets after layout, resize, or scene replacement.
    /// Offsets for viewports removed from the frame are discarded.
    pub fn reconcile(&mut self, frame: &Frame) {
        self.offsets.retain(|name, offset| {
            let Some(node) = frame.find(name).filter(|node| node.scroll) else {
                return false;
            };
            let (max_x, max_y) = frame.scroll_limits(name);
            offset.0 = offset.0.clamp(0, max_x);
            offset.1 = offset.1.clamp(0, max_y);
            // Keep explicit zero offsets so callers can observe that the
            // viewport remains registered after its content shrinks.
            let _ = node;
            true
        });
    }
}

impl Frame {
    pub fn find(&self, name: &str) -> Option<&DrawNode> {
        self.index.get(name).map(|&i| &self.nodes[i])
    }

    /// Maximum horizontal and vertical offsets for a scroll viewport,
    /// measured from all descendants in the same scene coordinate space.
    pub fn scroll_limits(&self, name: &str) -> (i64, i64) {
        let Some(viewport_index) = self.index.get(name).copied() else {
            return (0, 0);
        };
        let viewport = self.nodes[viewport_index].rect;
        if !self.nodes[viewport_index].scroll {
            return (0, 0);
        }
        let mut content_right = viewport.x.saturating_add(viewport.width);
        let mut content_bottom = viewport.y.saturating_add(viewport.height);
        for (index, node) in self.nodes.iter().enumerate() {
            if self.ancestor_indexes(index).contains(&viewport_index) {
                content_right = content_right.max(node.rect.x.saturating_add(node.rect.width));
                content_bottom = content_bottom.max(node.rect.y.saturating_add(node.rect.height));
            }
        }
        (
            content_right.saturating_sub(viewport.x.saturating_add(viewport.width)),
            content_bottom.saturating_sub(viewport.y.saturating_add(viewport.height)),
        )
    }

    fn ancestor_indexes(&self, mut index: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        while let Some(parent) = self.parent_index.get(index).copied().flatten() {
            if chain.contains(&parent) {
                break;
            }
            chain.push(parent);
            index = parent;
        }
        chain
    }

    /// Return draw nodes translated by all ancestor scroll offsets and
    /// clipped to the intersection of their scrollable ancestors.
    pub fn scrolled_nodes(&self, offsets: &ScrollOffsets) -> Vec<DrawNode> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let mut translated = node.clone();
                let mut shift_x = 0i64;
                let mut shift_y = 0i64;
                let mut clips: Vec<RectMp> = Vec::new();
                let mut chain = self.ancestor_indexes(index);
                chain.reverse();
                for ancestor_index in chain {
                    let ancestor = &self.nodes[ancestor_index];
                    let visual_rect = RectMp {
                        x: ancestor.rect.x - shift_x,
                        y: ancestor.rect.y - shift_y,
                        width: ancestor.rect.width,
                        height: ancestor.rect.height,
                    };
                    if ancestor.scroll {
                        clips.push(visual_rect);
                        let (dx, dy) = offsets.get(&ancestor.name);
                        shift_x = shift_x.saturating_add(dx);
                        shift_y = shift_y.saturating_add(dy);
                    }
                }
                translated.rect.x = translated.rect.x.saturating_sub(shift_x);
                translated.rect.y = translated.rect.y.saturating_sub(shift_y);
                translated.clip = clips.into_iter().reduce(intersect_rects);
                translated
            })
            .collect()
    }

    /// Find the topmost visible scroll viewport under a screen point.
    pub fn scroll_target_at(&self, offsets: &ScrollOffsets, x: i64, y: i64) -> Option<String> {
        self.scrolled_nodes(offsets)
            .into_iter()
            .rev()
            .find_map(|node| {
                (node.scroll
                    && contains_point(node.rect, x, y)
                    && node.clip.is_none_or(|clip| contains_point(clip, x, y)))
                .then_some(node.name)
            })
    }

    fn insert(&mut self, node: DrawNode, parent: Option<usize>) {
        self.index.insert(node.name.clone(), self.nodes.len());
        self.parent_index.push(parent);
        self.nodes.push(node);
    }
}

/// Resolve the scene against a viewport of `width_pt × height_pt` points.
/// The root cell's extent defaults to the full viewport; absolute child
/// geometry is parent-origin-relative; proportional geometry is a fraction
/// of the parent extent. Deterministic and total.
pub fn layout(scene: &Scene, width_pt: f64, height_pt: f64) -> Frame {
    let mut frame = Frame::default();
    let vp = RectMp {
        x: 0,
        y: 0,
        width: (width_pt * MP) as i64,
        height: (height_pt * MP) as i64,
    };
    resolve_cell(&scene.root, vp, &mut frame, None, None, false);
    frame
}

fn resolve_cell(
    cell: &SceneCell,
    parent_rect: RectMp,
    frame: &mut Frame,
    parent_name: Option<&str>,
    parent_index: Option<usize>,
    allow_overflow: bool,
) {
    let rect = resolve_rect(cell, parent_rect, allow_overflow);
    let text_size_mp = cell
        .text_size_p
        .map(|f| scale_mp(f.max(0.0) * rect.height as f64))
        .or(cell.text_size.map(|points| scale_mp(points * MP)));
    let stroke_width_mp = cell
        .stroke_width_p
        .map(|f| scale_mp(f.clamp(0.0, 0.5) * rect.width.min(rect.height) as f64))
        .or(cell.stroke_width.map(|points| scale_mp(points * MP)));
    // Wrap is a pure textual transform: greedy word wrap at the declared
    // column count, emitted as '\n'-joined text. The wave-2 goya-wayland
    // raster already splits '\n' text at glyph_px*1.4 line height (rect
    // clipped), so wrapped text reaches the screen with no renderer change.
    let text = match (&cell.wrap, &cell.text) {
        (Some(width), Some(t)) => Some(wrap_text(t, *width as usize).join("\n")),
        _ => cell.text.clone(),
    };
    let index = frame.nodes.len();
    frame.insert(
        DrawNode {
            name: cell.name.clone(),
            parent: parent_name.map(str::to_owned),
            is_button: cell.kind == CellKind::Button,
            is_input: cell.is_input,
            placeholder: cell.placeholder.clone(),
            max_length: cell.max_length,
            rect,
            clip: None,
            fill: cell.fill,
            stroke: cell.stroke,
            stroke_width: cell.stroke_width,
            text,
            text_size: cell.text_size,
            text_size_mp,
            stroke_width_mp,
            anchor: cell.anchor.unwrap_or(Anchor::Start),
            scroll: cell.scroll,
        },
        parent_index,
    );
    for child in &cell.children {
        resolve_cell(
            child,
            rect,
            frame,
            Some(&cell.name),
            Some(index),
            cell.scroll,
        );
    }
}

fn contains_point(rect: RectMp, x: i64, y: i64) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn intersect_rects(a: RectMp, b: RectMp) -> RectMp {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.x.saturating_add(a.width).min(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .min(b.y.saturating_add(b.height));
    RectMp {
        x,
        y,
        width: right.saturating_sub(x).max(0),
        height: bottom.saturating_sub(y).max(0),
    }
}

/// Greedy word wrap at `width` columns (same algorithm class as the
/// monitor's host-side `wrap_line`): words fill lines until they would
/// overflow; a token longer than `width` is hard-split. Multi-space runs
/// collapse to single separators between words on a line. Deterministic
/// and measurement-free.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;
    // Width 0 cannot come from the parser (it rejects 0), but stay total.
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut current = String::new();
        let mut current_len = 0usize;
        // Empty tokens between consecutive spaces would accumulate as
        // separators; skip them so runs collapse to single spaces.
        for word in line.split(' ').filter(|w| !w.is_empty()) {
            let mut word = word;
            loop {
                let word_len = word.graphemes(true).count();
                let sep = usize::from(!current.is_empty());
                if current_len + sep + word_len <= width {
                    if sep == 1 {
                        current.push(' ');
                    }
                    current.push_str(word);
                    current_len += sep + word_len;
                    break;
                }
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                    current_len = 0;
                    continue;
                }
                // The word alone exceeds the width: hard-split it.
                let split_at = word
                    .grapheme_indices(true)
                    .nth(width)
                    .map(|(index, _)| index)
                    .unwrap_or(word.len());
                let head = &word[..split_at];
                word = &word[split_at..];
                out.push(head.to_string());
                if word.is_empty() {
                    break;
                }
            }
        }
        // Preserve authored line breaks exactly, including empty lines.
        out.push(current);
    }
    out
}

fn resolve_rect(cell: &SceneCell, parent: RectMp, allow_overflow: bool) -> RectMp {
    // Fractions are clamped to [0, 1] of the parent, except inside a scroll
    // viewport, where content may be larger than the viewport (>= 0 only).
    let fraction = |value: f64| {
        if allow_overflow {
            value.max(0.0)
        } else {
            value.clamp(0.0, 1.0)
        }
    };
    // Extent first: proportional fraction of parent, else absolute points,
    // else a sensible default (half the parent, never negative).
    let (w, h) = if let Some(e) = cell.extent_p {
        (
            scale_mp(fraction(e.x) * parent.width as f64),
            scale_mp(fraction(e.y) * parent.height as f64),
        )
    } else if let Some(e) = cell.extent {
        (scale_mp(e.x * MP), scale_mp(e.y * MP))
    } else {
        // Undeclared extent fills the parent — cells are surfaces by default.
        (parent.width, parent.height)
    };
    let (w, h) = (w.max(0), h.max(0));
    // Position: proportional fraction of parent extent, else absolute points
    // from the parent origin. Both clamp into the parent.
    let (x, y) = if let Some(p) = cell.position_p {
        (
            parent
                .x
                .saturating_add(scale_mp(fraction(p.x) * parent.width as f64)),
            parent
                .y
                .saturating_add(scale_mp(fraction(p.y) * parent.height as f64)),
        )
    } else if let Some(p) = cell.position {
        (
            parent.x.saturating_add(scale_mp(p.x * MP)),
            parent.y.saturating_add(scale_mp(p.y * MP)),
        )
    } else {
        (parent.x, parent.y)
    };
    let parent_right = parent.x.saturating_add(parent.width);
    let parent_bottom = parent.y.saturating_add(parent.height);
    // Clamp inside the parent (non-negative offsets only).
    let x = if allow_overflow {
        x.max(parent.x)
    } else {
        x.max(parent.x).min(parent_right)
    };
    let y = if allow_overflow {
        y.max(parent.y)
    } else {
        y.max(parent.y).min(parent_bottom)
    };
    RectMp {
        x,
        y,
        width: if allow_overflow {
            w
        } else {
            w.min(parent_right.saturating_sub(x)).max(0)
        },
        height: if allow_overflow {
            h
        } else {
            h.min(parent_bottom.saturating_sub(y)).max(0)
        },
    }
}

fn scale_mp(value: f64) -> i64 {
    if !value.is_finite() {
        return if value.is_sign_negative() {
            i64::MIN
        } else {
            i64::MAX
        };
    }
    // Out-of-range finite values saturate; this cast does not panic.
    value as i64
}

/// Topmost interactive element under a point (millipoints), searching
/// depth-first with later-drawn (deeper) nodes winning.
pub fn hit_test(frame: &Frame, x: i64, y: i64) -> Option<String> {
    hit_test_scrolled(frame, &ScrollOffsets::default(), x, y)
}

/// Hit test a scene after applying scroll offsets and ancestor clipping.
pub fn hit_test_scrolled(frame: &Frame, offsets: &ScrollOffsets, x: i64, y: i64) -> Option<String> {
    for node in frame.scrolled_nodes(offsets).iter().rev() {
        // Interactive: buttons activate; inputs are focus targets (host-side
        // focus state, Wave 2 composer).
        if (node.is_button || node.is_input)
            && contains_point(node.rect, x, y)
            && node.clip.is_none_or(|clip| contains_point(clip, x, y))
        {
            return Some(node.name.clone());
        }
    }
    None
}

#[cfg(test)]
mod proportional_tests {
    use super::layout;
    use crate::parse;

    #[test]
    fn fractions_clamp_to_the_parent_except_inside_scroll_viewports() {
        let scene = parse("@ontology a { @cell root { @cell plain { id: \"p\"; extent_p: (2, -1, 0); position_p: (1.5, 0.5, 0); } @cell view { extent_p: (0.5, 0.5, 0); scroll: true; @cell content { extent_p: (1, 3.5, 0); position_p: (0, 1.25, 0); } } } }").unwrap();
        let frame = layout(&scene, 400.0, 200.0);
        let plain = frame.find("plain").unwrap().rect;
        // Clamped: x at the right edge, width 0; y at half, height 0.
        assert_eq!(
            (plain.x, plain.y, plain.width, plain.height),
            (400_000, 100_000, 0, 0)
        );
        let content = frame.find("content").unwrap().rect;
        assert_eq!(
            (content.x, content.y, content.width, content.height),
            (0, 125_000, 200_000, 350_000)
        );
        assert_eq!(frame.scroll_limits("view"), (0, 375_000));
    }

    #[test]
    fn text_size_and_stroke_width_resolve_against_the_element_rect() {
        let scene = parse("@ontology a { @cell root { @cell t { extent_p: (0.5, 0.25, 0); text: \"x\"; text-size-p: 0.6; stroke: #000; stroke-width-p: 0.75; } @cell abs { text: \"y\"; text-size: 12; } } }").unwrap();
        let frame = layout(&scene, 400.0, 200.0);
        let t = frame.find("t").unwrap();
        // 0.6 of a 50 pt height; stroke clamped to 0.5 of the 50 pt short side.
        assert_eq!(t.text_size_mp, Some(30_000));
        assert_eq!(t.stroke_width_mp, Some(25_000));
        assert_eq!(frame.find("abs").unwrap().text_size_mp, Some(12_000));
    }
}

#[cfg(test)]
mod scrolling_tests {
    use super::{layout, ScrollOffsets};
    use crate::parse;

    #[test]
    fn reconcile_clamps_stale_offsets_and_removes_missing_viewports() {
        let first = parse("@ontology a { @cell root { extent: (400, 300, 0) @cell viewport { extent: (100, 80, 0) scroll: true @cell content { position: (0, 200, 0) extent: (20, 20, 0) } } } }").unwrap();
        let first_frame = layout(&first, 400.0, 300.0);
        let mut offsets = ScrollOffsets::default();
        assert!(offsets.scroll_by(&first_frame, "viewport", 0, 500_000));
        assert_eq!(offsets.get("viewport"), (0, 140_000));

        let smaller = parse("@ontology a { @cell root { extent: (400, 300, 0) @cell viewport { extent: (100, 80, 0) scroll: true @cell content { position: (0, 100, 0) extent: (20, 20, 0) } } } }").unwrap();
        offsets.reconcile(&layout(&smaller, 400.0, 300.0));
        assert_eq!(offsets.get("viewport"), (0, 40_000));

        let removed = parse("@ontology a { @cell root { extent: (400, 300, 0) } }").unwrap();
        offsets.reconcile(&layout(&removed, 400.0, 300.0));
        assert_eq!(offsets.get("viewport"), (0, 0));
    }
}
