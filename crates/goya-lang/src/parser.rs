//! Recursive-descent parser: tokens → Scene. Total on all inputs; every
//! failure is a `ParseError` with a line and an exact UTF-8 byte range.

use crate::desktop::{self, ChromeChoice, DesktopFault, DesktopPosition, DesktopState};
use crate::lexer::{lex, Token, TokenKind};
use crate::scene::{
    Anchor, CellKind, ChromaticCandidate, ChromaticsDecl, Condition, ConditionOp,
    EffectRequestDecl, EffectValueType, GestureDecision, GesturePolicy, Iden, IntentCategory,
    IntentDecl, InteractionClass, InteractionDecl, InteractionExpr, InteractionType,
    InteractionValue, Phrasing, Scene, SceneCell, StateAssignment, TransportClass, TypedKey,
    TypedSlot, Vec3, MAX_PHRASE_TEXT_BYTES,
};
use goya_chromatics::{Risk, Stability, Strategy};
use goya_determinism::EffectKind;
use std::collections::BTreeMap;
use std::fmt;

/// Bound recursive scene parsing and the matching tree walks used by strict
/// admission. Ordinary scenes are shallow; this prevents adversarial nesting
/// from exhausting the host stack.
const MAX_CELL_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    /// End-exclusive UTF-8 byte range from the start of the construct being
    /// parsed through the offending token. At end of input the range ends at
    /// the source length.
    pub range: SourceRange,
}

#[cfg(test)]
mod platform_policy_tests {
    use super::*;

    const SCENE: &str = r#"@ontology demo {
        @persistence { cache_targets: null; }
        @cell root { id: "root"; }
        @platform android_tv {
            "media": only-use("native.player");
            "input": choice-use("remote", "keyboard");
            "decode": fallback("software");
            "network": fallbacks("ethernet", "wifi");
            "audio": must-use("mixer");
            "focus": invade("custom.focus");
            "gestures": cede;
        }
    }"#;

    #[test]
    fn exceptions_are_explicit_and_old_profiles_refuse_them() {
        let any = parse_strict_gssh4(
            "@ontology demo { @persistence { cache_targets: null; } @cell root { id: \"root\"; } }",
        )
        .unwrap();
        assert!(any.declarations.platform_exceptions.is_empty());
        let parsed = parse_strict_gssh4(SCENE).unwrap();
        assert_eq!(parsed.declarations.platform_exceptions.len(), 7);
        assert_eq!(
            parsed.declarations.platform_exceptions[1].options,
            ["remote", "keyboard"]
        );
        assert_eq!(
            parsed.declarations.platform_exceptions[6].use_kind,
            PlatformUse::Cede
        );
        assert!(parse(SCENE).is_err());
        assert_eq!(parse_strict_gssh3(SCENE).unwrap_err().code, "GY-E1015");
        assert_eq!(parse_strict_gssh3_live(SCENE).unwrap_err().code, "GY-E1015");
        let neutral = SCENE.replace("@platform android_tv {", "@platform webos {");
        assert_eq!(
            parse_strict_gssh4(&neutral)
                .unwrap()
                .declarations
                .platform_exceptions[0]
                .platform,
            "webos"
        );
        let live = "@ontology demo { @persistence { cache_targets: null; } @state { label: text = \"x\" session; } @cell root { id: \"root\"; text-from: state.label; } }";
        assert_eq!(parse_strict_gssh4(live).unwrap_err().code, "GY-E1012");
        // GSSH/4-live admits both (decision D12); GSSH/3-live still refuses
        // the GSSH/4 declaration.
        let both = parse_strict_gssh4_live(live).unwrap();
        assert_eq!(both.scene.root.text_from.as_deref(), Some("label"));
        assert_eq!(both.declarations.persistence_cache_targets, Some(None));
        assert_eq!(parse_strict_gssh3_live(live).unwrap_err().code, "GY-E1015");
        assert_eq!(
            parse_strict_gssh4_live(SCENE)
                .unwrap()
                .declarations
                .platform_exceptions
                .len(),
            7
        );
        let unbound = live.replace("state.label", "state.missing");
        assert_eq!(
            parse_strict_gssh4_live(&unbound).unwrap_err().code,
            "GY-E1013"
        );
    }

    #[test]
    fn exceptions_require_distinct_options_and_unique_scope() {
        for bad in [
            SCENE.replace(
                "choice-use(\"remote\", \"keyboard\")",
                "choice-use(\"remote\")",
            ),
            SCENE.replace(
                "fallbacks(\"ethernet\", \"wifi\")",
                "fallbacks(\"ethernet\")",
            ),
            SCENE.replace(
                "choice-use(\"remote\", \"keyboard\")",
                "choice-use(\"remote\", \"remote\")",
            ),
            SCENE.replace("\"gestures\": cede;", "\"media\": cede;"),
        ] {
            assert_eq!(parse_strict_gssh4(&bad).unwrap_err().code, "GY-E1000");
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error at line {}", self.line)
    }
}

impl std::error::Error for ParseError {}

/// An end-exclusive UTF-8 byte range in the original scene source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRange {
    pub start: usize,
    pub end: usize,
}

/// A declaration or property encountered by the canonical parser.
/// Entries are sorted by source position; duplicate names remain distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEntry {
    pub kind: String,
    pub name: String,
    pub range: SourceRange,
    /// Digest binding this origin to the exact source revision.
    pub source_digest: [u8; 32],
    /// Child indexes from the parsed scene root. Not a persistent semantic ID.
    pub scene_path: Option<Vec<usize>>,
    /// Stable authored element identity, or the owning element identity for a
    /// property entry. `scene_path` remains a within-revision lookup aid.
    pub semantic_id: Option<String>,
}

/// Scene and source records produced by the same parse.
#[derive(Debug, Clone)]
pub struct ParsedScene {
    pub scene: Scene,
    pub entries: Vec<SourceEntry>,
    /// SHA-256 of the exact UTF-8 source bytes parsed for this snapshot.
    /// This identifies a revision; authored element IDs provide identity.
    pub source_digest: [u8; 32],
    /// GSSH/2 declarations. Empty unless the source authors them; GSSH/1
    /// strict admission rejects any that are present.
    pub declarations: Declarations,
}

/// How long a declared state value lives (GSSH/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateLifetime {
    /// Lives for one application process; never written to storage.
    Session,
    /// Restored across process death by a platform storage adapter.
    Durable,
}

/// One `@state` entry: `key: type = initial lifetime;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    pub key: String,
    pub value_type: InteractionType,
    pub initial: InteractionValue,
    pub lifetime: StateLifetime,
    pub range: SourceRange,
}

/// One interaction input bound to an authored `@input` element:
/// `key: type from "element.id"` (GSSH/2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputBinding {
    /// Structural name of the owning interaction.
    pub interaction: String,
    pub input: String,
    pub value_type: InteractionType,
    /// Authored ID of the bound `@input` element.
    pub field_id: String,
    pub range: SourceRange,
}

/// Where an interaction declares one state or input key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyUse {
    /// Structural name of the owning interaction.
    pub interaction: String,
    pub key: String,
    /// `read count: int`, `write count: int` or `amount: int` (with any
    /// `from "..."` clause).
    pub range: SourceRange,
}

/// What happens to a record kept in another store of the same platform when
/// the scene's store has none (GSSH/4 `@persistence`, decision D13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Migration {
    /// Copy the record into the scene's store and keep the old one (default).
    Copy,
    /// Copy the record, then remove it from the old store.
    Move,
    /// Ignore other stores: start from the authored values.
    None,
}

/// An authored `migration:` ruling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationRule {
    /// `migration: copy|move|none;` for every platform.
    All(Migration),
    /// `migration: { <platform>: copy|move|none; ... };`; unlisted
    /// platforms keep the default, `copy`.
    PerPlatform(BTreeMap<String, Migration>),
}

impl MigrationRule {
    pub fn for_platform(&self, platform: &str) -> Migration {
        match self {
            Self::All(rule) => *rule,
            Self::PerPlatform(rules) => rules.get(platform).copied().unwrap_or(Migration::Copy),
        }
    }
}

/// Platform keys `@persistence` accepts.
const PERSISTENCE_PLATFORMS: &[&str] = &[
    "android", "ios", "macos", "windows", "linux", "browser", "webos",
];

/// `@iden` keys later profiles may define (decision D14). Accepted by the
/// parser with a text value and refused by strict admission (`GY-E1017`).
pub const RESERVED_IDEN_KEYS: &[&str] = &[
    "role",
    "lang",
    "live",
    "shortcut",
    "value_text",
    "state_text",
    "native",
];

/// Largest `@iden` label and hint, in UTF-8 bytes.
pub const MAX_IDEN_LABEL_BYTES: usize = 512;
pub const MAX_IDEN_HINT_BYTES: usize = 2048;

/// An explicit exception to the ontology's implicit `any` platform scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformUse {
    OnlyUse,
    ChoiceUse,
    Fallback,
    Fallbacks,
    MustUse,
    Invade,
    Cede,
}

/// One `@desktop` block (decision D16). Absence on [`Declarations`] means the
/// author omitted it; the resolved desktop is still fullscreen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopDecl {
    pub state: Option<DesktopState>,
    pub position: Option<DesktopPosition>,
    pub extent_dip: Option<(i32, i32)>,
    pub controls: Option<ChromeChoice>,
    pub range: SourceRange,
}

/// A target compiler must resolve this rule for its own platform or refuse it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformException {
    pub platform: String,
    pub feature: String,
    pub use_kind: PlatformUse,
    pub options: Vec<String>,
    pub range: SourceRange,
}

/// GSSH/2 declarations retained beside the GY-LANG/1 scene, plus the
/// interaction key ranges used to report them precisely.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declarations {
    /// The `@state` block's entries and the block's source range, if authored.
    pub state: Option<(Vec<StateDecl>, SourceRange)>,
    pub bindings: Vec<InputBinding>,
    /// Every `read`/`write` key of every interaction, in source order.
    pub state_uses: Vec<KeyUse>,
    /// Every input key of every interaction, in source order.
    pub input_keys: Vec<KeyUse>,
    /// GSSH/4 `@persistence` cache targets. `None` means the author omitted
    /// the declaration (each target's default local store); `Some(None)`
    /// means the author disabled persistence with `cache_targets: null`.
    pub persistence_cache_targets: Option<Option<BTreeMap<String, String>>>,
    /// GSSH/4 `migration:` ruling; `None` means the default, `copy`.
    pub persistence_migration: Option<MigrationRule>,
    /// Absence means platform `any`; exceptions never change other targets.
    pub platform_exceptions: Vec<PlatformException>,
    /// GSSH/4 `@desktop`. `None` is the enforced default: fullscreen.
    pub desktop: Option<DesktopDecl>,
    /// `@graphic` declarations (live profiles only).
    pub graphics: Vec<crate::graphic::GraphicDecl>,
}

impl Declarations {
    /// Initial desktop after GSSH/4 admission. Omission resolves to fullscreen
    /// with plain chrome. A loose parse can hold a combination strict
    /// admission rejects; this returns that fault instead of guessing.
    pub fn resolved_desktop(&self) -> Result<desktop::ResolvedDesktop, DesktopFault> {
        match &self.desktop {
            None => desktop::resolve(false, None, None, None, None),
            Some(decl) => desktop::resolve(
                true,
                decl.state,
                decl.position,
                decl.extent_dip,
                decl.controls.as_ref(),
            ),
        }
    }
}

/// A construct accepted for legacy compatibility but rejected by strict
/// semantic admission because this frontend does not preserve its meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrictParseError {
    pub code: &'static str,
    pub name: String,
    pub range: SourceRange,
}

impl fmt::Display for StrictParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self.code {
            "GY-E1000" => "invalid Goya syntax",
            "GY-E1001" => "unknown property",
            "GY-E1002" => "unsupported template construct",
            "GY-E1003" => "duplicate property",
            "GY-E1004" => "invalid interaction",
            "GY-E1005" => "missing or duplicate authored ID",
            "GY-E1006" => "declaration requires GSSH/2 admission",
            "GY-E1007" => "invalid input binding",
            "GY-E1008" => "invalid state declaration",
            "GY-E1009" => "absolute length in a proportional profile",
            "GY-E1010" => "intent declaration requires intent admission",
            "GY-E1011" => "invalid intent declaration",
            "GY-E1012" => {
                "live binding requires a live profile (GSSH/3-live, GSSH/4-live or GSSH/5-live)"
            }
            "GY-E1013" => "invalid live binding",
            "GY-E1014" => "invalid persistence cache targets",
            "GY-E1015" => "declaration requires GSSH/4 admission",
            "GY-E1017" => "invalid accessible identity",
            "GY-E1018" => "invalid desktop declaration",
            "GY-E1019" => "invalid static phrasing",
            "GY-E1020" => "chromatics declaration requires chromatics admission",
            "GY-E1021" => "invalid chromatics declaration",
            _ => "strict parsing failed",
        };
        write!(f, "{}: {} '{}'", self.code, description, self.name)
    }
}

impl std::error::Error for StrictParseError {}

/// Parse a scene document.
pub fn parse(source: &str) -> Result<Scene, ParseError> {
    let parsed = parse_with_source_map(source)?;
    // The compatibility API returns only Scene and would discard policy.
    // Refuse instead of silently dropping a platform exception, a desktop,
    // a graphic the layout pass does not paint, or a phrase layout cannot
    // select a class for.
    let dropped = parsed
        .declarations
        .platform_exceptions
        .first()
        .map(|rule| rule.range)
        .or_else(|| {
            parsed
                .declarations
                .desktop
                .as_ref()
                .map(|desktop| desktop.range)
        })
        .or_else(|| first_extended_live(&parsed))
        .or_else(|| first_phrasing_range(&parsed.scene.root));
    if let Some(range) = dropped {
        return Err(ParseError {
            line: source[..range.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count()
                + 1,
            range,
        });
    }
    Ok(parsed.scene)
}

/// Parse a scene and retain the locations of authored declarations and properties.
pub fn parse_with_source_map(source: &str) -> Result<ParsedScene, ParseError> {
    let tokens = match lex(source) {
        Ok(t) => t,
        Err(e) => {
            return Err(ParseError {
                line: e.line,
                range: SourceRange {
                    start: e.byte_start,
                    end: e.byte_end,
                },
            })
        }
    };
    let mut p = P {
        tokens,
        i: 0,
        interactions: Vec::new(),
        intents: Vec::new(),
        chromatics: None,
        entries: Vec::new(),
        source_len: source.len(),
        anchor: 0,
        depth: 0,
        state: None,
        persistence_cache_targets: None,
        persistence_migration: None,
        platform_exceptions: Vec::new(),
        desktop: None,
        graphics: Vec::new(),
        bindings: Vec::new(),
        state_uses: Vec::new(),
        input_keys: Vec::new(),
    };
    p.expect_at_word("ontology")?;
    let ontology_start = p.tokens[p.i - 1].byte_start;
    let name = p.expect_ident()?;
    p.expect(TokenKind::LBrace)?;
    let mut root = p.parse_cell_body()?;
    let ontology_end = p.tokens[p.i - 1].byte_end;
    // A document is exactly one ontology block. Do not silently accept
    // trailing tokens, which can hide a malformed second root or typo.
    if let Some(token) = p.peek() {
        p.anchor = token.byte_start;
        return Err(p.err());
    }
    // Keep an origin for the implicit ontology body. It is also the only
    // declaration-like source span available when the body itself becomes
    // the scene root (for example, when it contains root properties).
    p.entries.push(SourceEntry {
        kind: "ontology".into(),
        name: name.clone(),
        range: SourceRange {
            start: ontology_start,
            end: ontology_end,
        },
        source_digest: [0; 32],
        scene_path: None,
        semantic_id: None,
    });
    // Collapse the anonymous ontology wrapper when it holds exactly one
    // named cell and no own properties — the wrapper is syntax, not scene.
    // Otherwise the wrapper keeps the ontology name so lowering has a root.
    let collapse_root = root.children.len() == 1
        && root.position.is_none()
        && root.extent.is_none()
        && root.position_p.is_none()
        && root.extent_p.is_none()
        && root.fill.is_none()
        && root.stroke.is_none()
        && root.stroke_width.is_none()
        && root.text.is_none()
        && root.text_size.is_none()
        && root.text_size_p.is_none()
        && root.stroke_width_p.is_none()
        && root.anchor.is_none()
        && root.authored_id.is_none()
        && root.wrap.is_none()
        && root.gestures.is_none()
        && root.text_from.is_none()
        && root.fill_to.is_none()
        && root.present_when.is_none()
        && root.appear_s.is_none()
        && root.appear_from_p.is_none()
        && root.draw.is_none()
        && root.text_phrasing.is_none()
        && root.placeholder_phrasing.is_none()
        && !root.scroll
        && !root.is_input;
    if collapse_root {
        root = root.children.remove(0);
    } else if root.name.is_empty() {
        root.name = name.clone();
    }
    p.entries
        .sort_by_key(|entry| (entry.range.start, entry.range.end));
    let source_digest: [u8; 32] = {
        use sha2::{Digest, Sha256};
        Sha256::digest(source.as_bytes()).into()
    };
    // Scene declarations are nested in source order. Count only scene children;
    // properties and interaction declarations do not occupy child indexes.
    let mut stack: Vec<(usize, Vec<usize>, usize)> = Vec::new();
    let mut root_children = 0usize;
    for entry in &mut p.entries {
        while stack
            .last()
            .is_some_and(|(end, _, _)| *end <= entry.range.start)
        {
            stack.pop();
        }
        if matches!(entry.kind.as_str(), "cell" | "button" | "input") {
            let mut path = if let Some((_, parent, next)) = stack.last_mut() {
                let mut path = parent.clone();
                path.push(*next);
                *next += 1;
                path
            } else {
                let path = vec![root_children];
                root_children += 1;
                path
            };
            stack.push((entry.range.end, path.clone(), 0));
            if collapse_root {
                path.remove(0);
            }
            entry.scene_path = Some(path);
        } else if matches!(
            entry.kind.as_str(),
            "property" | "gestures" | "iden" | "draw"
        ) {
            let mut path = stack
                .last()
                .map(|(_, path, _)| path.clone())
                .unwrap_or_default();
            if collapse_root && !path.is_empty() {
                path.remove(0);
            }
            entry.scene_path = Some(path);
        }
        if let Some(path) = &entry.scene_path {
            if let Some(cell) = cell_at_path(&root, path) {
                entry.semantic_id = cell.authored_id.clone();
            }
        }
        if entry.kind == "binding" {
            entry.semantic_id = p
                .bindings
                .iter()
                .find(|binding| binding.range == entry.range)
                .and_then(|binding| {
                    p.interactions
                        .iter()
                        .find(|interaction| interaction.name == binding.interaction)
                })
                .and_then(|interaction| interaction.authored_id.clone());
        }
        if entry.kind == "interaction" {
            entry.semantic_id = p
                .interactions
                .iter()
                .find(|interaction| interaction.name == entry.name)
                .and_then(|interaction| interaction.authored_id.clone());
        }
        entry.source_digest = source_digest;
    }
    Ok(ParsedScene {
        scene: Scene {
            name,
            root,
            interactions: p.interactions,
            intents: p.intents,
            chromatics: p.chromatics,
        },
        entries: p.entries,
        source_digest,
        declarations: Declarations {
            state: p.state,
            bindings: p.bindings,
            state_uses: p.state_uses,
            input_keys: p.input_keys,
            persistence_cache_targets: p.persistence_cache_targets,
            persistence_migration: p.persistence_migration,
            platform_exceptions: p.platform_exceptions,
            desktop: p.desktop,
            graphics: p.graphics,
        },
    })
}

fn cell_at_path<'a>(root: &'a SceneCell, path: &[usize]) -> Option<&'a SceneCell> {
    let mut cell = root;
    for index in path {
        cell = cell.children.get(*index)?;
    }
    Some(cell)
}

/// Parse using the canonical GY-LANG grammar, then reject compatibility
/// constructs whose semantics the current AST discards. The returned range
/// is a half-open UTF-8 byte range in `source`.
///
/// This is the GSSH/1 profile: `@state` blocks and `from` input bindings are
/// rejected with `GY-E1006`, because GSSH/1 consumers do not implement them.
pub fn parse_strict(source: &str) -> Result<ParsedScene, StrictParseError> {
    strict(source, Profile::Gssh1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Profile {
    Gssh1,
    Gssh2,
    Gssh3,
    /// GSSH/3 plus live bindings. Proposed; admitted only by consumers that
    /// render state. This branch carries `text-from`, `present-when`,
    /// `fill-to` and `@graphic` / `@draw`. `image-from`, `host` state and
    /// `@link` are not here yet and stay unknown.
    Gssh3Live,
    /// GSSH/3 plus `@platform`, `@persistence`, `@iden` and `@desktop`
    /// (decisions D10, D11, D14, D16). Live bindings stay gated by the live profiles.
    Gssh4,
    /// GSSH/4 plus live bindings (decision D12): `text-from` together with
    /// the GSSH/4 declarations.
    Gssh4Live,
    /// GSSH/4 plus enforced static phrasing (decision D17). Candidate.
    Gssh5,
    /// GSSH/5 plus the live bindings of GSSH/4-live.
    Gssh5Live,
}

impl Profile {
    /// Admits live bindings (`text-from`).
    fn live(self) -> bool {
        matches!(self, Self::Gssh3Live | Self::Gssh4Live | Self::Gssh5Live)
    }
    /// Admits GSSH/4 declarations (`@platform`, `@persistence`, `@iden`, `@desktop`).
    /// GSSH/5 includes those declarations.
    fn gssh4(self) -> bool {
        matches!(
            self,
            Self::Gssh4 | Self::Gssh4Live | Self::Gssh5 | Self::Gssh5Live
        )
    }
    /// Admits static phrasing (`@universal` and the class specifiers).
    fn gssh5(self) -> bool {
        matches!(self, Self::Gssh5 | Self::Gssh5Live)
    }
}

/// Properties only GSSH/3 live admission accepts.
const LIVE_PROPERTIES: &[&str] = &[
    "text_from",
    "present_when",
    "fill_to",
    "fill_period_s",
    "appear_s",
    "appear_from_p",
];

/// Shortest fill cross-fade, in seconds.
pub const MIN_ANIMATION_PERIOD_S: f64 = 1.0;
/// Longest fill cross-fade, in seconds.
pub const MAX_ANIMATION_PERIOD_S: f64 = 3600.0;
/// Bounds of an `appear-s` duration, in seconds. A single fade-in is not a
/// repeating flash, so it may be shorter than an ambient period.
pub const MIN_APPEAR_S: f64 = 0.1;
pub const MAX_APPEAR_S: f64 = 2.0;
/// Largest `appear-from-p` offset component, as a fraction of the parent.
pub const MAX_APPEAR_FRACTION: f64 = 0.5;

/// Length properties GSSH/3 requires to be proportional, by canonical name,
/// with their proportional replacement.
const ABSOLUTE_LENGTHS: &[(&str, &str)] = &[
    ("position", "position_p"),
    ("extent", "extent_p"),
    ("text_size", "text-size-p"),
    ("stroke_width", "stroke-width-p"),
];

/// Strict GSSH/3 admission: everything [`parse_strict_gssh2`] admits and
/// checks, plus the proportional `text-size-p` and `stroke-width-p`, and
/// with every absolute length rejected (`GY-E1009`, range: the property).
/// All lengths in a GSSH/3 scene are fractions of a resolved rect.
pub fn parse_strict_gssh3(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh3)?;
    validate_declarations(&parsed)?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
}

/// Strict GSSH/3 live admission (proposed): everything
/// [`parse_strict_gssh3`] admits and checks, plus live bindings.
///
/// * `text-from: state.key;` shows a declared key's value as text.
///
/// A declared key counts as used when an interaction or a binding uses it.
/// Errors: `GY-E1013` for an invalid binding, `GY-E1008` for state misuse.
pub fn parse_strict_gssh3_live(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh3Live)?;
    let bound_keys = validate_live(&parsed)?;
    validate_declarations_with(&parsed, Some(&bound_keys))?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
}

/// Admit GSSH/3 scenes with explicit, typed per-platform exceptions.
/// Live bindings remain gated under GSSH/3-live; platform rules do not
/// silently widen a target's semantic profile.
pub fn parse_strict_gssh4(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh4)?;
    validate_idens(&parsed)?;
    validate_desktop(&parsed)?;
    validate_declarations(&parsed)?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
}

/// Strict GSSH/4 live admission (decision D12): everything
/// [`parse_strict_gssh4`] admits and checks, plus the live bindings of
/// [`parse_strict_gssh3_live`], with the same rules and codes.
pub fn parse_strict_gssh4_live(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh4Live)?;
    validate_idens(&parsed)?;
    validate_desktop(&parsed)?;
    let bound_keys = validate_live(&parsed)?;
    validate_declarations_with(&parsed, Some(&bound_keys))?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
}

/// Strict GSSH/5 admission (decision D17, Candidate): everything
/// [`parse_strict_gssh4`] admits and checks, plus enforced static phrasing.
/// A bare string on `text`, `placeholder`, or an `@iden` label or hint is
/// `GY-E1019`. Live bindings stay on [`parse_strict_gssh5_live`].
pub fn parse_strict_gssh5(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh5)?;
    validate_phrasing(&parsed)?;
    validate_idens(&parsed)?;
    validate_desktop(&parsed)?;
    validate_declarations(&parsed)?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
}

/// Strict GSSH/5 live admission: GSSH/5 plus the live bindings of
/// [`parse_strict_gssh4_live`]. `text-from` is data, not a phrasing site.
pub fn parse_strict_gssh5_live(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh5Live)?;
    validate_phrasing(&parsed)?;
    validate_idens(&parsed)?;
    validate_desktop(&parsed)?;
    let bound_keys = validate_live(&parsed)?;
    validate_declarations_with(&parsed, Some(&bound_keys))?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
}

/// GSSH/4 accessible identity (decision D14): every `@input` has an `@iden`;
/// labels and hints are non-blank, free of control characters and within
/// their limits; reserved keys and an `@iden` on the ontology body itself
/// are refused. Errors are `GY-E1017` at the narrowest range.
fn validate_idens(parsed: &ParsedScene) -> Result<(), StrictParseError> {
    fn walk(
        parsed: &ParsedScene,
        cell: &SceneCell,
        path: &mut Vec<usize>,
        virtual_root: bool,
    ) -> Result<(), StrictParseError> {
        let entry = |kinds: &[&str]| {
            parsed
                .entries
                .iter()
                .find(|entry| {
                    kinds.contains(&entry.kind.as_str())
                        && entry.scene_path.as_deref() == Some(path.as_slice())
                })
                .map(|entry| entry.range)
        };
        let element = entry(&["cell", "button", "input"]);
        let error = |message: String, range: Option<SourceRange>| {
            declaration_error(
                "GY-E1017",
                message,
                range
                    .or(element)
                    .unwrap_or(SourceRange { start: 0, end: 0 }),
            )
        };
        let name = cell
            .authored_id
            .clone()
            .unwrap_or_else(|| cell.name.clone());
        match &cell.iden {
            None if cell.kind == CellKind::Input => {
                return Err(error(
                    format!("@input '{name}' has no @iden {{ label: \"...\"; }}"),
                    element,
                ))
            }
            None => {}
            Some(iden) => {
                let at = entry(&["iden"]);
                if virtual_root {
                    return Err(error(
                        "@iden directly in the ontology body names no element".into(),
                        at,
                    ));
                }
                if let Some((key, range)) = iden.reserved.first() {
                    return Err(error(
                        format!("@iden key '{key}' is reserved for a later profile"),
                        Some(*range),
                    ));
                }
                for (what, text, phrased, limit) in [
                    (
                        "label",
                        Some(&iden.label),
                        iden.label_phrasing.is_some(),
                        MAX_IDEN_LABEL_BYTES,
                    ),
                    (
                        "hint",
                        iden.hint.as_ref(),
                        iden.hint_phrasing.is_some(),
                        MAX_IDEN_HINT_BYTES,
                    ),
                ] {
                    // GSSH/5 stores the sentences on the phrasing fields and
                    // leaves the shared string empty. Those sentences are
                    // checked by `validate_phrasing`.
                    if phrased {
                        continue;
                    }
                    let Some(text) = text else { continue };
                    if text.trim().is_empty() {
                        return Err(error(format!("@iden {what} of '{name}' is blank"), at));
                    }
                    if text.chars().any(char::is_control) {
                        return Err(error(
                            format!("@iden {what} of '{name}' contains a control character"),
                            at,
                        ));
                    }
                    if text.len() > limit {
                        return Err(error(
                            format!("@iden {what} of '{name}' is above {limit} bytes of UTF-8"),
                            at,
                        ));
                    }
                }
            }
        }
        for (index, child) in cell.children.iter().enumerate() {
            path.push(index);
            walk(parsed, child, path, false)?;
            path.pop();
        }
        Ok(())
    }
    let root = &parsed.scene.root;
    if root.authored_id.is_none() && root.kind == CellKind::Cell {
        // The ontology body is a virtual root: its children are the
        // top-level elements, addressed without the root index.
        walk(parsed, root, &mut Vec::new(), true)
    } else {
        walk(parsed, root, &mut Vec::new(), false)
    }
}

fn reject_absolute_lengths(parsed: &ParsedScene) -> Result<(), StrictParseError> {
    if let Some((entry, replacement)) = parsed.entries.iter().find_map(|entry| {
        let canonical = entry.name.replace('-', "_");
        (entry.kind == "property")
            .then(|| ABSOLUTE_LENGTHS.iter().find(|(name, _)| *name == canonical))
            .flatten()
            .map(|(_, replacement)| (entry, *replacement))
    }) {
        return Err(StrictParseError {
            code: "GY-E1009",
            name: format!("{}; use {replacement}", entry.name),
            range: entry.range,
        });
    }
    Ok(())
}

/// Every live binding names a declared key.
fn validate_live(
    parsed: &ParsedScene,
) -> Result<std::collections::BTreeSet<String>, StrictParseError> {
    let empty = Vec::new();
    let decls = parsed
        .declarations
        .state
        .as_ref()
        .map_or(&empty, |(decls, _)| decls);
    let property_range = |path: &[usize], name: &str| {
        parsed
            .entries
            .iter()
            .find(|e| {
                e.kind == "property"
                    && e.name.replace('-', "_") == name
                    && e.scene_path.as_deref() == Some(path)
            })
            .map_or(SourceRange { start: 0, end: 0 }, |e| e.range)
    };
    type Visit<'v> = dyn FnMut(&SceneCell, &[usize]) -> Result<(), StrictParseError> + 'v;
    fn walk(
        cell: &SceneCell,
        path: &mut Vec<usize>,
        f: &mut Visit<'_>,
    ) -> Result<(), StrictParseError> {
        f(cell, path)?;
        for (i, child) in cell.children.iter().enumerate() {
            path.push(i);
            walk(child, path, f)?;
            path.pop();
        }
        Ok(())
    }
    let bad = |message: String, range: SourceRange| StrictParseError {
        code: "GY-E1013",
        name: message,
        range,
    };
    let mut bound_keys = std::collections::BTreeSet::new();
    let mut check = |cell: &SceneCell, path: &[usize]| -> Result<(), StrictParseError> {
        if let Some(key) = &cell.text_from {
            // Inputs own their displayed text: it is the editable value, not
            // a label to append a state value to. Reject this at shared
            // admission so every GSSH/3-live consumer agrees, rather than
            // letting one target accept it and a later target refuse it.
            if cell.kind == CellKind::Input {
                return Err(bad(
                    "text-from cannot be used on an @input".to_owned(),
                    property_range(path, "text_from"),
                ));
            }
            if !decls.iter().any(|d| &d.key == key) {
                return Err(bad(
                    format!("text-from names undeclared state '{key}'"),
                    property_range(path, "text_from"),
                ));
            }
            bound_keys.insert(key.clone());
        }
        let find = |key: &str| decls.iter().find(|d| d.key == key);
        let period_ok = |p: f64| {
            p.is_finite() && (MIN_ANIMATION_PERIOD_S..=MAX_ANIMATION_PERIOD_S).contains(&p)
        };
        match (cell.fill_to, cell.fill_period_s) {
            (None, None) => {}
            (Some(_), Some(p)) if cell.fill.is_some() && period_ok(p) => {}
            _ => {
                return Err(bad(
                    format!(
                        "fill-to needs fill and fill-period-s between {MIN_ANIMATION_PERIOD_S} and {MAX_ANIMATION_PERIOD_S} seconds, and the reverse"
                    ),
                    property_range(
                        path,
                        if cell.fill_to.is_some() {
                            "fill_to"
                        } else {
                            "fill_period_s"
                        },
                    ),
                ))
            }
        }
        if let Some(condition) = &cell.present_when {
            let literal_type = match condition.value {
                InteractionValue::Int(_) => InteractionType::Int,
                InteractionValue::Text(_) => InteractionType::Text,
                InteractionValue::Bool(_) => InteractionType::Bool,
            };
            if condition.op == ConditionOp::AtLeast && literal_type != InteractionType::Int {
                return Err(bad(
                    format!(
                        "present-when {} at-least needs an int literal",
                        condition.key
                    ),
                    property_range(path, "present_when"),
                ));
            }
            match find(&condition.key) {
                Some(d) if d.value_type == literal_type => {}
                _ => {
                    return Err(bad(
                        format!(
                            "present-when compares '{}' with a literal of another type or an undeclared key",
                            condition.key
                        ),
                        property_range(path, "present_when"),
                    ))
                }
            }
            bound_keys.insert(condition.key.clone());
        }
        let appear_ok = |s: f64| s.is_finite() && (MIN_APPEAR_S..=MAX_APPEAR_S).contains(&s);
        match (cell.appear_s, cell.appear_from_p) {
            (None, None) => {}
            (Some(s), None) if appear_ok(s) => {}
            (Some(s), Some(v))
                if appear_ok(s)
                    && [v.x, v.y]
                        .iter()
                        .all(|f| f.is_finite() && f.abs() <= MAX_APPEAR_FRACTION)
                    && v.z == 0.0 => {}
            _ => {
                return Err(bad(
                    format!(
                        "appear-s must lie in [{MIN_APPEAR_S}, {MAX_APPEAR_S}] seconds; appear-from-p needs appear-s, fractions within ±{MAX_APPEAR_FRACTION} and z = 0"
                    ),
                    property_range(
                        path,
                        if cell.appear_from_p.is_some() {
                            "appear_from_p"
                        } else {
                            "appear_s"
                        },
                    ),
                ))
            }
        }
        Ok(())
    };
    walk(&parsed.scene.root, &mut Vec::new(), &mut check)?;
    let condition_ok = |c: &Condition| {
        let literal_type = match c.value {
            InteractionValue::Int(_) => InteractionType::Int,
            InteractionValue::Text(_) => InteractionType::Text,
            InteractionValue::Bool(_) => InteractionType::Bool,
        };
        (c.op == ConditionOp::Is || literal_type == InteractionType::Int)
            && decls
                .iter()
                .any(|d| d.key == c.key && d.value_type == literal_type)
    };
    let draw_keys = crate::graphic::validate(
        &parsed.declarations.graphics,
        &parsed.scene.root,
        &condition_ok,
    )
    .map_err(|(message, range)| bad(message, range))?;
    bound_keys.extend(draw_keys);
    Ok(bound_keys)
}

/// The first `@graphic`, `@draw`, `present-when` or `fill-to` in source
/// order. Live profiles admit these; every other consumer refuses them
/// rather than painting a scene that ignores them.
pub fn first_extended_live(parsed: &ParsedScene) -> Option<SourceRange> {
    parsed.entries.iter().find_map(|entry| {
        let hit = match entry.kind.as_str() {
            "graphic" | "draw" => true,
            "property" => matches!(
                entry.name.replace('-', "_").as_str(),
                "present_when" | "fill_to" | "fill_period_s" | "appear_s" | "appear_from_p"
            ),
            _ => false,
        };
        hit.then_some(entry.range)
    })
}

/// Strict GSSH/2 admission: everything [`parse_strict`] admits and checks,
/// plus `@state` declarations and `from` input bindings. When a scene has
/// interactions, every state key they use must be declared exactly once with
/// the same type, every declared key must be used, and every interaction
/// input must be bound to one authored `@input` element. Bool inputs cannot
/// be bound to a text field.
pub fn parse_strict_gssh2(source: &str) -> Result<ParsedScene, StrictParseError> {
    let parsed = strict(source, Profile::Gssh2)?;
    validate_declarations(&parsed)?;
    Ok(parsed)
}

fn strict(source: &str, profile: Profile) -> Result<ParsedScene, StrictParseError> {
    let parsed = parse_with_source_map(source).map_err(|error| StrictParseError {
        code: "GY-E1000",
        name: format!("syntax at line {}", error.line),
        range: error.range,
    })?;
    let mut properties = std::collections::BTreeSet::new();
    for entry in &parsed.entries {
        let unsupported = match entry.kind.as_str() {
            // `placeholder` is a GSSH/5 phrasing site. Earlier strict
            // profiles keep the compatibility-only refusal.
            "property" if entry.name == "placeholder" && !profile.gssh5() => Some("GY-E1001"),
            "property" if !is_known_property(&entry.name) => Some("GY-E1001"),
            "template" => Some("GY-E1002"),
            "state" | "binding" if profile == Profile::Gssh1 => Some("GY-E1006"),
            "property"
                if !profile.live()
                    && LIVE_PROPERTIES.contains(&entry.name.replace('-', "_").as_str()) =>
            {
                Some("GY-E1012")
            }
            "graphic" | "draw" if !profile.live() => Some("GY-E1012"),
            // Intent declarations are Quitasol phase-0 experimental surface.
            // No strict GSSH consumer implements them, so none may admit one.
            "intent" => Some("GY-E1010"),
            // Chromatics declarations are GY-CHROMATICS/0.1 draft surface.
            // No strict GSSH consumer implements adaptive input allocation,
            // so none may admit one until the contract is ratified.
            "chromatics" => Some("GY-E1020"),
            // GSSH/4 declarations: earlier profiles refuse them rather than
            // drop them (decision D11 keeps approved profiles unchanged).
            "platform" | "persistence" | "iden" | "desktop" if !profile.gssh4() => Some("GY-E1015"),
            // Proportional text and stroke lengths are GSSH/3 only, so older
            // consumers never receive a length they would ignore.
            "property"
                if profile < Profile::Gssh3
                    && matches!(
                        entry.name.replace('-', "_").as_str(),
                        "text_size_p" | "stroke_width_p"
                    ) =>
            {
                Some("GY-E1006")
            }
            _ => None,
        };
        if let Some(code) = unsupported {
            return Err(StrictParseError {
                code,
                name: entry.name.clone(),
                range: entry.range,
            });
        }
        if entry.kind == "property" || entry.kind == "gestures" {
            let canonical = entry.name.replace('-', "_");
            let key = (entry.scene_path.clone(), canonical);
            if !properties.insert(key) {
                return Err(StrictParseError {
                    code: "GY-E1003",
                    name: entry.name.clone(),
                    range: entry.range,
                });
            }
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    let root = &parsed.scene.root;
    let virtual_root = root.authored_id.is_none()
        && root.name == parsed.scene.name
        && root.position.is_none()
        && root.extent.is_none()
        && root.position_p.is_none()
        && root.extent_p.is_none()
        && root.fill.is_none()
        && root.stroke.is_none()
        && root.stroke_width.is_none()
        && root.text.is_none()
        && root.text_size.is_none()
        && root.text_size_p.is_none()
        && root.stroke_width_p.is_none()
        && root.anchor.is_none()
        && root.wrap.is_none()
        && root.gestures.is_none()
        && root.text_from.is_none()
        && root.fill_to.is_none()
        && root.present_when.is_none()
        && root.appear_s.is_none()
        && root.appear_from_p.is_none()
        && root.draw.is_none()
        && root.text_phrasing.is_none()
        && root.placeholder_phrasing.is_none()
        && !root.scroll
        && !root.is_input
        && root.children.len() > 1;
    let id_error = if virtual_root {
        root.children
            .iter()
            .enumerate()
            .find_map(|(index, child)| validate_authored_ids(child, &mut ids, vec![index]))
    } else {
        validate_authored_ids(root, &mut ids, Vec::new())
    };
    if let Some(error) = id_error {
        let path = match &error {
            AuthoredIdError::Missing { path, .. } | AuthoredIdError::Duplicate { path, .. } => path,
        };
        let entry = parsed.entries.iter().find(|entry| {
            matches!(entry.kind.as_str(), "cell" | "button" | "input")
                && entry.scene_path.as_deref() == Some(path.as_slice())
        });
        return Err(StrictParseError {
            code: "GY-E1005",
            name: error.to_string(),
            range: entry
                .map(|entry| entry.range)
                .or_else(|| {
                    // An ontology body with authored properties is a real
                    // scene root, but has no @cell declaration of its own.
                    // Point its missing-ID diagnostic at that body span.
                    (path.is_empty())
                        .then(|| parsed.entries.iter().find(|entry| entry.kind == "ontology"))
                        .flatten()
                        .map(|entry| entry.range)
                })
                .unwrap_or(SourceRange { start: 0, end: 0 }),
        });
    }
    for interaction in &parsed.scene.interactions {
        let Some(id) = interaction.authored_id.as_ref() else {
            let entry = parsed
                .entries
                .iter()
                .find(|entry| entry.kind == "interaction" && entry.name == interaction.name);
            return Err(StrictParseError {
                code: "GY-E1005",
                name: format!("missing ID for interaction {}", interaction.name),
                range: entry
                    .map(|entry| entry.range)
                    .unwrap_or(SourceRange { start: 0, end: 0 }),
            });
        };
        if !ids.insert(id.clone()) {
            let entry = parsed
                .entries
                .iter()
                .find(|entry| entry.kind == "interaction" && entry.name == interaction.name);
            return Err(StrictParseError {
                code: "GY-E1005",
                name: id.clone(),
                range: entry
                    .map(|entry| entry.range)
                    .unwrap_or(SourceRange { start: 0, end: 0 }),
            });
        }
    }
    if let Err((failed_index, error)) =
        crate::transition::validate_interactions_with_origin(&parsed.scene)
    {
        let entry = parsed
            .entries
            .iter()
            .filter(|entry| entry.kind == "interaction")
            .nth(failed_index);
        return Err(StrictParseError {
            code: "GY-E1004",
            name: error.to_string(),
            range: entry
                .map(|entry| entry.range)
                .unwrap_or(SourceRange { start: 0, end: 0 }),
        });
    }
    if !profile.gssh5() {
        if let Some(range) = first_phrasing_range(&parsed.scene.root) {
            return Err(StrictParseError {
                code: "GY-E1019",
                name: "phrasing requires GSSH/5".into(),
                range,
            });
        }
    }
    Ok(parsed)
}

/// Earliest static phrasing value in the tree. Layout has no platform
/// class, so a compatibility scene must not carry one silently.
fn first_phrasing_range(cell: &SceneCell) -> Option<SourceRange> {
    let mut best: Option<SourceRange> = None;
    fn consider(best: &mut Option<SourceRange>, phrasing: &Phrasing) {
        if best
            .map(|range| phrasing.range.start < range.start)
            .unwrap_or(true)
        {
            *best = Some(phrasing.range);
        }
    }
    fn walk(cell: &SceneCell, best: &mut Option<SourceRange>) {
        if let Some(phrasing) = &cell.text_phrasing {
            consider(best, phrasing);
        }
        if let Some(phrasing) = &cell.placeholder_phrasing {
            consider(best, phrasing);
        }
        if let Some(iden) = &cell.iden {
            if let Some(phrasing) = &iden.label_phrasing {
                consider(best, phrasing);
            }
            if let Some(phrasing) = &iden.hint_phrasing {
                consider(best, phrasing);
            }
        }
        for child in &cell.children {
            walk(child, best);
        }
    }
    walk(cell, &mut best);
    best
}

/// GSSH/5 (decision D17): every static sentence names its classes.
fn validate_phrasing(parsed: &ParsedScene) -> Result<(), StrictParseError> {
    fn property_range(parsed: &ParsedScene, path: &[usize], name: &str) -> SourceRange {
        parsed
            .entries
            .iter()
            .find(|entry| {
                entry.kind == "property"
                    && entry.name.replace('-', "_") == name
                    && entry.scene_path.as_deref() == Some(path)
            })
            .map_or(SourceRange { start: 0, end: 0 }, |entry| entry.range)
    }
    fn phrase_fault(phrasing: &Phrasing, limit: usize) -> Option<String> {
        for (specifier, text) in [
            ("@universal", phrasing.universal.as_deref()),
            ("@desktop", phrasing.desktop.as_deref()),
            ("@mobile", phrasing.mobile.as_deref()),
            ("@tv", phrasing.tv.as_deref()),
            ("@web", phrasing.web.as_deref()),
        ] {
            let Some(text) = text else { continue };
            if text.trim().is_empty() {
                return Some(format!("{specifier} phrase is blank"));
            }
            if text.chars().any(char::is_control) {
                return Some(format!("{specifier} phrase contains a control character"));
            }
            if text.len() > limit {
                return Some(format!(
                    "{specifier} phrase is above {limit} bytes of UTF-8"
                ));
            }
        }
        phrasing
            .missing_class()
            .map(|class| format!("has no phrase for {}", class.specifier()))
    }
    fn require(
        what: &str,
        name: &str,
        bare: bool,
        phrasing: Option<&Phrasing>,
        limit: usize,
        bare_range: SourceRange,
    ) -> Result<(), StrictParseError> {
        match phrasing {
            None if bare => Err(declaration_error(
                "GY-E1019",
                format!("{what} of '{name}' is not phrased"),
                bare_range,
            )),
            None => Ok(()),
            Some(phrasing) => match phrase_fault(phrasing, limit) {
                Some(fault) => Err(declaration_error(
                    "GY-E1019",
                    format!("{what} of '{name}' {fault}"),
                    phrasing.range,
                )),
                None => Ok(()),
            },
        }
    }
    fn walk(
        parsed: &ParsedScene,
        cell: &SceneCell,
        path: &mut Vec<usize>,
    ) -> Result<(), StrictParseError> {
        let name = cell
            .authored_id
            .clone()
            .unwrap_or_else(|| cell.name.clone());
        require(
            "text",
            &name,
            cell.text.is_some(),
            cell.text_phrasing.as_ref(),
            MAX_PHRASE_TEXT_BYTES,
            property_range(parsed, path, "text"),
        )?;
        require(
            "placeholder",
            &name,
            cell.placeholder.is_some(),
            cell.placeholder_phrasing.as_ref(),
            MAX_PHRASE_TEXT_BYTES,
            property_range(parsed, path, "placeholder"),
        )?;
        if let Some(iden) = &cell.iden {
            let iden_range = parsed
                .entries
                .iter()
                .find(|entry| {
                    entry.kind == "iden" && entry.scene_path.as_deref() == Some(path.as_slice())
                })
                .map_or(SourceRange { start: 0, end: 0 }, |entry| entry.range);
            require(
                "@iden label",
                &name,
                iden.label_phrasing.is_none(),
                iden.label_phrasing.as_ref(),
                MAX_IDEN_LABEL_BYTES,
                iden_range,
            )?;
            require(
                "@iden hint",
                &name,
                iden.hint.is_some(),
                iden.hint_phrasing.as_ref(),
                MAX_IDEN_HINT_BYTES,
                iden_range,
            )?;
        }
        for (index, child) in cell.children.iter().enumerate() {
            path.push(index);
            walk(parsed, child, path)?;
            path.pop();
        }
        Ok(())
    }
    // The scene root is path `[]`. A multi-cell ontology addresses its
    // children from that same root, which matches the stored entry paths.
    walk(parsed, &parsed.scene.root, &mut Vec::new())
}

/// `0`, or an optional `-` and a digit string without a leading zero.
/// `-0`, `+1`, `01`, `1_000` and fractions are not canonical.
pub(crate) fn is_canonical_decimal(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && text != "-0"
}

fn declaration_error(code: &'static str, name: String, range: SourceRange) -> StrictParseError {
    StrictParseError { code, name, range }
}

/// GSSH/4 desktop combinations (decision D16). Syntax stays `GY-E1000`;
/// these rules are `GY-E1018` and are not applied by the loose parser.
fn validate_desktop(parsed: &ParsedScene) -> Result<(), StrictParseError> {
    let Some(decl) = &parsed.declarations.desktop else {
        return Ok(());
    };
    parsed
        .declarations
        .resolved_desktop()
        .map(|_| ())
        .map_err(|fault| declaration_error("GY-E1018", fault.message(), decl.range))
}

fn validate_declarations(parsed: &ParsedScene) -> Result<(), StrictParseError> {
    validate_declarations_with(parsed, None)
}

fn validate_declarations_with(
    parsed: &ParsedScene,
    live_keys: Option<&std::collections::BTreeSet<String>>,
) -> Result<(), StrictParseError> {
    let scene = &parsed.scene;
    let declared = &parsed.declarations;
    // `@persistence` is optional (GSSH/4 only, decision D11): when it is
    // absent every target keeps durable state in its default local store.
    // `cache_targets: null` disables persistence, so durable state with it
    // is a contradiction.
    let has_durable_state = declared.state.as_ref().is_some_and(|(items, _)| {
        items
            .iter()
            .any(|item| item.lifetime == StateLifetime::Durable)
    });
    let persistence_range = || {
        parsed
            .entries
            .iter()
            .find(|entry| entry.kind == "persistence")
            .map_or(SourceRange { start: 0, end: 0 }, |entry| entry.range)
    };
    if has_durable_state && matches!(declared.persistence_cache_targets, Some(None)) {
        return Err(declaration_error(
            "GY-E1014",
            "durable state with `cache_targets: null` (persistence is disabled)".into(),
            persistence_range(),
        ));
    }
    if declared.persistence_migration.is_some()
        && matches!(declared.persistence_cache_targets, Some(None))
    {
        return Err(declaration_error(
            "GY-E1014",
            "`migration` with `cache_targets: null` (persistence is disabled)".into(),
            persistence_range(),
        ));
    }
    let key_range = |uses: &[KeyUse], interaction: &str, key: &str| {
        uses.iter()
            .find(|item| item.interaction == interaction && item.key == key)
            .map(|item| item.range)
            .unwrap_or(SourceRange { start: 0, end: 0 })
    };
    let mut inputs_by_id = BTreeMap::new();
    collect_inputs(&scene.root, &mut inputs_by_id);
    let bindings = &parsed.declarations.bindings;
    for binding in bindings {
        match inputs_by_id.get(binding.field_id.as_str()) {
            None => {
                return Err(declaration_error(
                    "GY-E1007",
                    format!("no element has id '{}'", binding.field_id),
                    binding.range,
                ))
            }
            Some(false) => {
                return Err(declaration_error(
                    "GY-E1007",
                    format!("element '{}' is not an @input", binding.field_id),
                    binding.range,
                ))
            }
            Some(true) => {}
        }
        if binding.value_type == InteractionType::Bool {
            return Err(declaration_error(
                "GY-E1007",
                format!(
                    "bool input '{}' cannot be bound to a text field",
                    binding.input
                ),
                binding.range,
            ));
        }
        if bindings.iter().any(|other| {
            other.interaction == binding.interaction
                && other.field_id == binding.field_id
                && other.range.start < binding.range.start
        }) {
            return Err(declaration_error(
                "GY-E1007",
                format!(
                    "field '{}' is bound twice in interaction '{}'",
                    binding.field_id, binding.interaction
                ),
                binding.range,
            ));
        }
    }
    for interaction in &scene.interactions {
        for input in &interaction.inputs {
            let bound = bindings.iter().any(|binding| {
                binding.interaction == interaction.name && binding.input == input.name
            });
            if !bound {
                return Err(declaration_error(
                    "GY-E1007",
                    format!(
                        "input '{}' of interaction '{}' has no field binding",
                        input.name, interaction.name
                    ),
                    key_range(&declared.input_keys, &interaction.name, &input.name),
                ));
            }
        }
    }

    // State coverage: every used key declared with the same type; every
    // declared key used. Interactions are checked in source order.
    let mut used: BTreeMap<&str, (InteractionType, &str)> = BTreeMap::new();
    for interaction in &scene.interactions {
        for key in interaction
            .state_reads
            .iter()
            .chain(&interaction.state_writes)
        {
            used.entry(key.name.as_str())
                .or_insert((key.value_type, interaction.name.as_str()));
        }
    }
    let empty = Vec::new();
    let (decls, block_range) = match &parsed.declarations.state {
        Some((decls, range)) => (decls, Some(*range)),
        None => (&empty, None),
    };
    for interaction in &scene.interactions {
        for key in interaction
            .state_reads
            .iter()
            .chain(&interaction.state_writes)
        {
            match decls.iter().find(|decl| decl.key == key.name) {
                None => {
                    return Err(declaration_error(
                        "GY-E1008",
                        format!(
                            "state '{}' used by interaction '{}' is not declared in @state",
                            key.name, interaction.name
                        ),
                        key_range(&declared.state_uses, &interaction.name, &key.name),
                    ))
                }
                Some(decl) if decl.value_type != key.value_type => {
                    return Err(declaration_error(
                        "GY-E1008",
                        format!(
                        "state '{}' is declared with a different type than interaction '{}' uses",
                        key.name, interaction.name
                    ),
                        decl.range,
                    ))
                }
                Some(_) => {}
            }
        }
    }
    if let Some(decl) = decls.iter().find(|decl| {
        !used.contains_key(decl.key.as_str())
            && !live_keys.is_some_and(|keys| keys.contains(&decl.key))
    }) {
        return Err(declaration_error(
            "GY-E1008",
            format!(
                "state '{}' is declared but no interaction uses it",
                decl.key
            ),
            decl.range,
        ));
    }
    if decls.is_empty() {
        if let Some(range) = block_range {
            return Err(declaration_error(
                "GY-E1008",
                "empty @state block".into(),
                range,
            ));
        }
    }
    Ok(())
}

fn collect_inputs<'a>(cell: &'a SceneCell, out: &mut BTreeMap<&'a str, bool>) {
    if let Some(id) = cell.authored_id.as_deref() {
        out.insert(id, cell.kind == CellKind::Input);
    }
    for child in &cell.children {
        collect_inputs(child, out);
    }
}

fn is_known_property(name: &str) -> bool {
    matches!(
        name,
        "position"
            | "extent"
            | "position_p"
            | "extent_p"
            | "fill"
            | "stroke"
            | "stroke-width"
            | "stroke_width"
            | "text"
            | "text-size"
            | "text_size"
            | "text-size-p"
            | "text_size_p"
            | "stroke-width-p"
            | "stroke_width_p"
            | "wrap"
            | "scroll"
            | "text-anchor"
            | "text_anchor"
            | "id"
            | "placeholder"
            | "text-from"
            | "text_from"
            | "present-when"
            | "present_when"
            | "fill-to"
            | "fill_to"
            | "fill-period-s"
            | "fill_period_s"
            | "appear-s"
            | "appear_s"
            | "appear-from-p"
            | "appear_from_p"
    )
}

#[derive(Debug)]
enum AuthoredIdError {
    Missing { name: String, path: Vec<usize> },
    Duplicate { id: String, path: Vec<usize> },
}

impl fmt::Display for AuthoredIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { name, .. } => write!(f, "missing ID for {name}"),
            Self::Duplicate { id, .. } => write!(f, "{id}"),
        }
    }
}

fn validate_authored_ids(
    cell: &SceneCell,
    seen: &mut std::collections::BTreeSet<String>,
    path: Vec<usize>,
) -> Option<AuthoredIdError> {
    let Some(id) = cell.authored_id.as_ref() else {
        return Some(AuthoredIdError::Missing {
            name: cell.name.clone(),
            path,
        });
    };
    if id.is_empty() {
        return Some(AuthoredIdError::Missing {
            name: cell.name.clone(),
            path,
        });
    }
    if !seen.insert(id.clone()) {
        return Some(AuthoredIdError::Duplicate {
            id: id.clone(),
            path,
        });
    }
    for (index, child) in cell.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(index);
        if let Some(error) = validate_authored_ids(child, seen, child_path) {
            return Some(error);
        }
    }
    None
}

struct DesktopBody {
    state: Option<DesktopState>,
    position: Option<DesktopPosition>,
    extent_dip: Option<(i32, i32)>,
    controls: Option<ChromeChoice>,
}

/// Canonical DIP integer text. `signed` allows a leading minus, never `+` or `-0`.
fn canonical_dip(text: &str, signed: bool) -> bool {
    let digits = if let Some(rest) = text.strip_prefix('-') {
        if !signed {
            return false;
        }
        rest
    } else {
        text
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    if digits.len() > 1 && digits.starts_with('0') {
        return false;
    }
    if text.starts_with('-') && digits == "0" {
        return false;
    }
    true
}

struct P {
    tokens: Vec<Token>,
    i: usize,
    interactions: Vec<InteractionDecl>,
    intents: Vec<IntentDecl>,
    chromatics: Option<ChromaticsDecl>,
    entries: Vec<SourceEntry>,
    source_len: usize,
    /// Start of the construct currently being parsed; diagnostics span from
    /// here through the offending token.
    anchor: usize,
    /// Cell-body nesting depth. The ontology body is depth 1.
    depth: usize,
    state: Option<(Vec<StateDecl>, SourceRange)>,
    persistence_cache_targets: Option<Option<BTreeMap<String, String>>>,
    persistence_migration: Option<MigrationRule>,
    platform_exceptions: Vec<PlatformException>,
    desktop: Option<DesktopDecl>,
    graphics: Vec<crate::graphic::GraphicDecl>,
    bindings: Vec<InputBinding>,
    state_uses: Vec<KeyUse>,
    input_keys: Vec<KeyUse>,
}

impl P {
    fn error_at(&self, line: usize, start: usize, end: usize) -> ParseError {
        ParseError {
            line,
            range: SourceRange {
                start: self.anchor.min(start),
                end,
            },
        }
    }

    /// Error at the next unconsumed token, or at end of input.
    fn err(&self) -> ParseError {
        match self.peek() {
            Some(token) => self.error_at(token.line, token.byte_start, token.byte_end),
            None => self.error_at(self.line(), self.source_len, self.source_len),
        }
    }

    /// Error at the token just consumed, for validation after reading it.
    fn err_prev(&self) -> ParseError {
        match self
            .i
            .checked_sub(1)
            .and_then(|index| self.tokens.get(index))
        {
            Some(token) => self.error_at(token.line, token.byte_start, token.byte_end),
            None => self.err(),
        }
    }

    fn mark(&mut self) {
        if let Some(token) = self.peek() {
            self.anchor = token.byte_start;
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.i)
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.i).cloned();
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn line(&self) -> usize {
        self.peek()
            .map(|t| t.line)
            .or_else(|| self.tokens.last().map(|t| t.line))
            .unwrap_or(1)
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, ParseError> {
        match self.peek() {
            Some(t) if t.kind == kind => match self.bump() {
                Some(t) => Ok(t),
                None => Err(self.err()),
            },
            _ => Err(self.err()),
        }
    }

    fn expect_at_word(&mut self, word: &str) -> Result<(), ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::AtWord && t.text == word => {
                self.bump();
                Ok(())
            }
            _ => Err(self.err()),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Ident)) {
            match self.bump() {
                Some(t) => Ok(t.text),
                None => Err(self.err()),
            }
        } else {
            Err(self.err())
        }
    }

    /// Parse cells/buttons until the closing brace (consumed).
    fn parse_cell_body(&mut self) -> Result<SceneCell, ParseError> {
        // Header may be `@cell name {` or `@button name {`; the caller for
        // the ontology root uses a virtual cell.
        let mut cell = SceneCell {
            name: String::new(),
            authored_id: None,
            kind: CellKind::Cell,
            is_input: false,
            position: None,
            extent: None,
            position_p: None,
            extent_p: None,
            text_size_p: None,
            stroke_width_p: None,
            fill: None,
            stroke: None,
            stroke_width: None,
            text: None,
            text_phrasing: None,
            text_size: None,
            anchor: None,
            wrap: None,
            scroll: false,
            gestures: None,
            text_from: None,
            present_when: None,
            fill_to: None,
            fill_period_s: None,
            appear_s: None,
            appear_from_p: None,
            draw: None,
            iden: None,
            children: Vec::new(),
            placeholder: None,
            placeholder_phrasing: None,
            max_length: None,
        };
        if self.depth >= MAX_CELL_DEPTH {
            return Err(self.err());
        }
        self.depth += 1;
        loop {
            self.mark();
            match self.peek().map(|t| t.kind) {
                None => return Err(self.err()),
                Some(TokenKind::RBrace) => {
                    self.bump();
                    self.depth -= 1;
                    return Ok(cell);
                }
                Some(TokenKind::AtWord) => {
                    // The match guard guarantees a token exists; still
                    // explicit rather than unwrap.
                    let Some(tok) = self.bump() else {
                        return Err(self.err());
                    };
                    let word = tok.text;
                    match word.as_str() {
                        "cell" | "button" | "input" => {
                            let kind = match word.as_str() {
                                "button" => CellKind::Button,
                                "input" => CellKind::Input,
                                _ => CellKind::Cell,
                            };
                            let name = self.expect_ident()?;
                            self.expect(TokenKind::LBrace)?;
                            let mut child = self.parse_cell_body()?;
                            child.name = name;
                            child.kind = kind;
                            child.is_input = kind == CellKind::Input;
                            self.entries.push(SourceEntry {
                                kind: word.clone(),
                                name: child.name.clone(),
                                scene_path: None,
                                semantic_id: None,
                                range: SourceRange {
                                    start: tok.byte_start,
                                    end: self.tokens[self.i - 1].byte_end,
                                },
                                source_digest: [0; 32],
                            });
                            cell.children.push(child);
                        }
                        "interaction" => {
                            let name = self.expect_ident()?;
                            let interaction = self.parse_interaction(name)?;
                            self.entries.push(SourceEntry {
                                kind: word.clone(),
                                name: interaction.name.clone(),
                                scene_path: None,
                                semantic_id: None,
                                range: SourceRange {
                                    start: tok.byte_start,
                                    end: self.tokens[self.i - 1].byte_end,
                                },
                                source_digest: [0; 32],
                            });
                            self.interactions.push(interaction);
                        }
                        "intent" => {
                            let name = self.expect_ident()?;
                            let intent = self.parse_intent(name)?;
                            self.entries.push(SourceEntry {
                                kind: word.clone(),
                                name: intent.name.clone(),
                                scene_path: None,
                                semantic_id: None,
                                range: SourceRange {
                                    start: tok.byte_start,
                                    end: self.tokens[self.i - 1].byte_end,
                                },
                                source_digest: [0; 32],
                            });
                            self.intents.push(intent);
                        }
                        "chromatics" => {
                            // Draft GY-CHROMATICS/0.1 surface: ontology body
                            // only, at most once per scene. Semantic input
                            // allocation is scene-level until views exist.
                            if self.depth != 1 || self.chromatics.is_some() {
                                return Err(self.err_prev());
                            }
                            let name = self.expect_ident()?;
                            let decl = self.parse_chromatics(name)?;
                            self.entries.push(SourceEntry {
                                kind: word.clone(),
                                name: decl.name.clone(),
                                scene_path: None,
                                semantic_id: None,
                                range: SourceRange {
                                    start: tok.byte_start,
                                    end: self.tokens[self.i - 1].byte_end,
                                },
                                source_digest: [0; 32],
                            });
                            self.chromatics = Some(decl);
                        }
                        "state" => {
                            // GSSH/2 state declarations live only in the
                            // ontology body, at most once.
                            if self.depth != 1 || self.state.is_some() {
                                return Err(self.err_prev());
                            }
                            let decls = self.parse_state_block()?;
                            let range = SourceRange {
                                start: tok.byte_start,
                                end: self.tokens[self.i - 1].byte_end,
                            };
                            self.entries.push(SourceEntry {
                                kind: "state".into(),
                                name: "state".into(),
                                scene_path: None,
                                semantic_id: None,
                                range,
                                source_digest: [0; 32],
                            });
                            self.state = Some((decls, range));
                        }
                        "persistence" => {
                            if self.depth != 1 || self.persistence_cache_targets.is_some() {
                                return Err(self.err_prev());
                            }
                            let (targets, migration) = self.parse_persistence_block()?;
                            self.persistence_migration = migration;
                            let range = SourceRange {
                                start: tok.byte_start,
                                end: self.tokens[self.i - 1].byte_end,
                            };
                            self.entries.push(SourceEntry {
                                kind: "persistence".into(),
                                name: "persistence".into(),
                                scene_path: None,
                                semantic_id: None,
                                range,
                                source_digest: [0; 32],
                            });
                            self.persistence_cache_targets = Some(targets);
                        }
                        "desktop" => {
                            if self.depth != 1 || self.desktop.is_some() {
                                return Err(self.err_prev());
                            }
                            let body = self.parse_desktop_block()?;
                            let range = SourceRange {
                                start: tok.byte_start,
                                end: self.tokens[self.i - 1].byte_end,
                            };
                            self.entries.push(SourceEntry {
                                kind: "desktop".into(),
                                name: "desktop".into(),
                                scene_path: None,
                                semantic_id: None,
                                range,
                                source_digest: [0; 32],
                            });
                            self.desktop = Some(DesktopDecl {
                                state: body.state,
                                position: body.position,
                                extent_dip: body.extent_dip,
                                controls: body.controls,
                                range,
                            });
                        }
                        "graphic" => {
                            if self.depth != 1 {
                                return Err(self.err_prev());
                            }
                            let name = self.expect_ident()?;
                            let mut g = self.parse_graphic(name)?;
                            let range = SourceRange {
                                start: tok.byte_start,
                                end: self.tokens[self.i - 1].byte_end,
                            };
                            g.range = range;
                            self.entries.push(SourceEntry {
                                kind: "graphic".into(),
                                name: g.name.clone(),
                                scene_path: None,
                                semantic_id: g.authored_id.clone(),
                                range,
                                source_digest: [0; 32],
                            });
                            self.graphics.push(g);
                        }
                        "draw" => {
                            if self.depth == 1 || cell.draw.is_some() {
                                return Err(self.err_prev());
                            }
                            let graphic = self.expect_ident()?;
                            let mut d = self.parse_draw(graphic)?;
                            let range = SourceRange {
                                start: tok.byte_start,
                                end: self.tokens[self.i - 1].byte_end,
                            };
                            d.range = range;
                            self.entries.push(SourceEntry {
                                kind: "draw".into(),
                                name: d.graphic.clone(),
                                scene_path: None,
                                semantic_id: None,
                                range,
                                source_digest: [0; 32],
                            });
                            cell.draw = Some(d);
                        }
                        "platform" => {
                            if self.depth != 1 {
                                return Err(self.err_prev());
                            }
                            let platform = self.expect_ident()?;
                            if !matches!(
                                platform.as_str(),
                                "android"
                                    | "ios"
                                    | "macos"
                                    | "windows"
                                    | "linux"
                                    | "browser"
                                    | "webos"
                                    | "android_tv"
                                    | "tvos"
                            ) {
                                return Err(self.err_prev());
                            }
                            let exceptions = self.parse_platform_block(&platform)?;
                            for exception in exceptions {
                                if self.platform_exceptions.iter().any(|previous| {
                                    previous.platform == exception.platform
                                        && previous.feature == exception.feature
                                }) {
                                    return Err(self.err_prev());
                                }
                                self.entries.push(SourceEntry {
                                    kind: "platform".into(),
                                    name: format!("{}:{}", exception.platform, exception.feature),
                                    scene_path: None,
                                    semantic_id: None,
                                    range: exception.range,
                                    source_digest: [0; 32],
                                });
                                self.platform_exceptions.push(exception);
                            }
                        }
                        "template" => {
                            // Recognised and skipped with its block: visual
                            // state and templates are Wave-3 expression work.
                            let name = self.expect_ident()?;
                            self.skip_block()?;
                            self.entries.push(SourceEntry {
                                kind: word.clone(),
                                name,
                                scene_path: None,
                                semantic_id: None,
                                range: SourceRange {
                                    start: tok.byte_start,
                                    end: self.tokens[self.i - 1].byte_end,
                                },
                                source_digest: [0; 32],
                            });
                        }
                        "iden" => {
                            if cell.iden.is_some() {
                                return Err(self.err_prev());
                            }
                            cell.iden = Some(self.parse_iden_block()?);
                            self.entries.push(SourceEntry {
                                kind: "iden".into(),
                                name: "iden".into(),
                                scene_path: None,
                                semantic_id: None,
                                range: SourceRange {
                                    start: tok.byte_start,
                                    end: self.tokens[self.i - 1].byte_end,
                                },
                                source_digest: [0; 32],
                            });
                        }
                        _ => return Err(self.err_prev()),
                    }
                }
                Some(TokenKind::Ident) => {
                    let token = self.peek().cloned().ok_or_else(|| self.err())?;
                    if token.text == "gestures"
                        && self
                            .tokens
                            .get(self.i + 1)
                            .is_some_and(|next| next.kind == TokenKind::LBrace)
                    {
                        self.parse_gestures(&mut cell)?;
                        self.entries.push(SourceEntry {
                            kind: "gestures".into(),
                            name: "gestures".into(),
                            scene_path: None,
                            semantic_id: None,
                            range: SourceRange {
                                start: token.byte_start,
                                end: self.tokens[self.i - 1].byte_end,
                            },
                            source_digest: [0; 32],
                        });
                        continue;
                    }
                    self.parse_property(&mut cell)?;
                    self.entries.push(SourceEntry {
                        kind: "property".into(),
                        name: token.text,
                        scene_path: None,
                        semantic_id: None,
                        range: SourceRange {
                            start: token.byte_start,
                            end: self.tokens[self.i - 1].byte_end,
                        },
                        source_digest: [0; 32],
                    });
                }
                Some(_) => return Err(self.err()),
            }
        }
    }

    fn parse_interaction(&mut self, name: String) -> Result<InteractionDecl, ParseError> {
        let interaction_start = self.anchor;
        self.expect(TokenKind::LBrace)?;
        let authored_id = if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Ident && token.text == "id")
        {
            self.bump();
            self.expect(TokenKind::Colon)?;
            let value = self.expect(TokenKind::Str)?.text;
            self.optional_semi();
            Some(value)
        } else {
            None
        };
        self.expect_ident_value("trigger")?;
        self.expect(TokenKind::Colon)?;
        self.expect_ident_value("button")?;
        self.expect(TokenKind::Dot)?;
        let trigger_button = self.expect_ident()?;
        self.expect(TokenKind::Dot)?;
        self.expect_ident_value("activate")?;
        self.optional_semi();
        self.expect_ident_value("class")?;
        self.expect(TokenKind::Colon)?;
        let class = match self.expect_ident()?.as_str() {
            "D1" => InteractionClass::D1,
            "D2" => InteractionClass::D2,
            _ => return Err(self.err_prev()),
        };
        self.optional_semi();

        let mut declared_effects = Vec::new();
        let mut requests = Vec::new();
        if class == InteractionClass::D2 {
            self.expect_ident_value("effects")?;
            self.expect(TokenKind::LBrace)?;
            while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                declared_effects.push(self.parse_effect_kind()?);
                self.optional_semi();
            }
            self.expect(TokenKind::RBrace)?;

            self.expect_ident_value("requests")?;
            self.expect(TokenKind::LBrace)?;
            while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                let request_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                self.expect_ident_value("effect")?;
                let effect_kind = self.parse_effect_kind()?;
                self.expect_ident_value("capability")?;
                let capability = self.expect(TokenKind::Str)?.text;
                self.expect_ident_value("request")?;
                let request_utf8 = self.expect(TokenKind::Str)?.text;
                self.expect_ident_value("result")?;
                let result_type = match self.expect_ident()?.as_str() {
                    "int64" => EffectValueType::Int64,
                    "text" => EffectValueType::Text,
                    "bool" => EffectValueType::Bool,
                    _ => return Err(self.err_prev()),
                };
                requests.push(EffectRequestDecl {
                    name: request_name,
                    effect_kind,
                    capability,
                    request_utf8,
                    result_type,
                });
                self.expect(TokenKind::Semicolon)?;
            }
            self.expect(TokenKind::RBrace)?;
        }

        self.expect_ident_value("state")?;
        self.expect(TokenKind::LBrace)?;
        let mut state_reads = Vec::new();
        let mut state_writes = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let use_start = self.peek().map(|t| t.byte_start).unwrap_or(self.source_len);
            let mode = self.expect_ident()?;
            let target = match mode.as_str() {
                "read" => &mut state_reads,
                "write" => &mut state_writes,
                _ => return Err(self.err_prev()),
            };
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value_type = self.parse_interaction_type()?;
            self.state_uses.push(KeyUse {
                interaction: name.clone(),
                key: key.clone(),
                range: SourceRange {
                    start: use_start,
                    end: self.tokens[self.i - 1].byte_end,
                },
            });
            target.push(TypedKey {
                name: key,
                value_type,
            });
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;

        self.expect_ident_value("input")?;
        self.expect(TokenKind::LBrace)?;
        let mut inputs = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            self.mark();
            let key_start = self.peek().map(|t| t.byte_start).unwrap_or(self.source_len);
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value_type = self.parse_interaction_type()?;
            // GSSH/2: `key: type from "<input element id>"` binds the input
            // to the current text of one authored @input element.
            if self
                .peek()
                .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "from")
            {
                self.bump();
                let field = self.expect(TokenKind::Str)?;
                let range = SourceRange {
                    start: key_start,
                    end: field.byte_end,
                };
                self.entries.push(SourceEntry {
                    kind: "binding".into(),
                    name: key.clone(),
                    scene_path: None,
                    semantic_id: None,
                    range,
                    source_digest: [0; 32],
                });
                self.bindings.push(InputBinding {
                    interaction: name.clone(),
                    input: key.clone(),
                    value_type,
                    field_id: field.text,
                    range,
                });
            }
            self.input_keys.push(KeyUse {
                interaction: name.clone(),
                key: key.clone(),
                range: SourceRange {
                    start: key_start,
                    end: self.tokens[self.i - 1].byte_end,
                },
            });
            inputs.push(TypedKey {
                name: key,
                value_type,
            });
            self.optional_semi();
        }
        self.anchor = interaction_start;
        self.expect(TokenKind::RBrace)?;

        let mut assignments = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            self.expect_ident_value("set")?;
            let key = self.expect_ident()?;
            self.expect(TokenKind::Equal)?;
            let value = self.parse_interaction_expr()?;
            assignments.push(StateAssignment { key, value });
            self.expect(TokenKind::Semicolon)?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(InteractionDecl {
            name,
            authored_id,
            trigger_button,
            class,
            declared_effects,
            requests,
            state_reads,
            state_writes,
            inputs,
            assignments,
        })
    }

    /// Experimental `@intent Name { action: ...; voice { ... } fallback { ... } }`
    /// (Quitasol phase 0; proposed, not ratified). Body keyword order is
    /// fixed like `@interaction`; unknown keys and duplicate keys are errors.
    fn parse_intent(&mut self, name: String) -> Result<IntentDecl, ParseError> {
        let intent_start = self.anchor;
        self.expect(TokenKind::LBrace)?;
        let authored_id = if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Ident && token.text == "id")
        {
            self.bump();
            self.expect(TokenKind::Colon)?;
            let value = self.expect(TokenKind::Str)?.text;
            self.optional_semi();
            Some(value)
        } else {
            None
        };
        self.expect_ident_value("action")?;
        self.expect(TokenKind::Colon)?;
        let action = self.expect_ident()?;
        self.optional_semi();
        // Optional Q2 draft key: `transport: <class>;` declares the Media
        // Session transport behaviour this intent represents. Absent means
        // no transport lowering is declared; unknown keywords and duplicate
        // keys are parse errors (fail-closed).
        let transport = if self
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "transport")
        {
            self.bump();
            self.expect(TokenKind::Colon)?;
            let keyword = self.expect_ident()?;
            let parsed = TransportClass::from_keyword(&keyword);
            if parsed.is_none() {
                return Err(self.err_prev());
            }
            self.optional_semi();
            parsed
        } else {
            None
        };
        // Optional Q4 draft key: `category: <keyword>;` classifies the
        // intent in the closed Q4 set; default is `general`.
        let category = if self
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "category")
        {
            self.bump();
            self.expect(TokenKind::Colon)?;
            let keyword = self.expect_ident()?;
            let parsed = IntentCategory::from_keyword(&keyword);
            if parsed.is_none() {
                return Err(self.err_prev());
            }
            self.optional_semi();
            parsed.unwrap()
        } else {
            IntentCategory::default()
        };
        // Optional Q4 draft block: `slots { <name>: <type>; ... }` declares
        // the typed slots utterances may reference. Duplicates are errors;
        // usage symmetry is validated in `crate::intents` (GY-E1011).
        let slots = if self
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "slots")
        {
            self.bump();
            self.expect(TokenKind::LBrace)?;
            let mut slots: Vec<TypedSlot> = Vec::new();
            while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                let name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let value_type = self.parse_interaction_type()?;
                if slots.iter().any(|slot| slot.name == name) {
                    return Err(self.err_prev());
                }
                slots.push(TypedSlot { name, value_type });
                self.optional_semi();
            }
            self.expect(TokenKind::RBrace)?;
            slots
        } else {
            Vec::new()
        };
        self.expect_ident_value("voice")?;
        self.expect(TokenKind::LBrace)?;
        let mut utterances = Vec::new();
        let mut confirmation: Option<bool> = None;
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            match key.as_str() {
                "utterance" => {
                    let text = self.expect(TokenKind::Str)?.text;
                    if utterances.contains(&text) {
                        return Err(self.err_prev());
                    }
                    utterances.push(text);
                }
                "confirmation" => {
                    let value = self.expect_ident()?;
                    let parsed = match value.as_str() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(self.err_prev()),
                    };
                    if confirmation.is_some() {
                        return Err(self.err_prev());
                    }
                    confirmation = Some(parsed);
                }
                _ => return Err(self.err_prev()),
            }
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;
        self.expect_ident_value("fallback")?;
        self.expect(TokenKind::LBrace)?;
        let mut fallback_remote: Option<String> = None;
        let mut fallback_keyboard: Option<String> = None;
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let target = self.expect_ident()?;
            let slot = match key.as_str() {
                "remote" => &mut fallback_remote,
                "keyboard" => &mut fallback_keyboard,
                _ => return Err(self.err_prev()),
            };
            if slot.is_some() {
                return Err(self.err_prev());
            }
            *slot = Some(target);
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;
        self.anchor = intent_start;
        self.expect(TokenKind::RBrace)?;
        Ok(IntentDecl {
            name,
            authored_id,
            action,
            utterances,
            confirmation: confirmation.unwrap_or(false),
            fallback_remote,
            fallback_keyboard,
            transport,
            slots,
            category,
            range: SourceRange {
                start: intent_start,
                end: self.tokens[self.i - 1].byte_end,
            },
        })
    }

    /// `{ strategy … candidate Name { … } }` after `@chromatics`. The schema
    /// is closed: an unknown key, a repeated key or an unknown keyword is a
    /// parse error, never a defaulted field (§12: fail-closed declarations).
    fn parse_chromatics(&mut self, name: String) -> Result<ChromaticsDecl, ParseError> {
        let start = self.anchor;
        self.expect(TokenKind::LBrace)?;
        let mut primary: Option<Strategy> = None;
        let mut fallback: Option<Strategy> = None;
        let mut preserve_slots: Option<bool> = None;
        let mut ordered_slots: Option<bool> = None;
        let mut exclude_native: Option<bool> = None;
        let mut exclude_duplicates: Option<bool> = None;
        let mut stability = Stability::default();
        let mut stability_seen: [bool; 3] = [false; 3];
        let mut candidates: Vec<ChromaticCandidate> = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            self.mark();
            let key = self.expect_ident()?;
            match self.peek().map(|t| t.kind) {
                Some(TokenKind::Colon) => {
                    self.bump();
                    match key.as_str() {
                        "strategy" | "fallback" => {
                            let parsed = Strategy::parse(&self.expect_ident()?)
                                .ok_or_else(|| self.err_prev())?;
                            let slot = if key == "strategy" {
                                &mut primary
                            } else {
                                &mut fallback
                            };
                            if slot.is_some() {
                                return Err(self.err_prev());
                            }
                            *slot = Some(parsed);
                        }
                        "preserve-slots" | "ordered-slots"
                        | "exclude-native-equivalents" | "exclude-duplicates" => {
                            let parsed = self.parse_bool()?;
                            let slot = match key.as_str() {
                                "preserve-slots" => &mut preserve_slots,
                                "ordered-slots" => &mut ordered_slots,
                                "exclude-native-equivalents" => &mut exclude_native,
                                _ => &mut exclude_duplicates,
                            };
                            if slot.is_some() {
                                return Err(self.err_prev());
                            }
                            *slot = Some(parsed);
                        }
                        _ => return Err(self.err_prev()),
                    }
                }
                // `stability { … }` and `candidate Name { … }` sub-blocks.
                Some(TokenKind::LBrace) if key == "stability" => {
                    self.bump();
                    let mut index = 0usize;
                    while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                        let threshold = self.expect_ident()?;
                        self.expect(TokenKind::Colon)?;
                        let which = match threshold.as_str() {
                            "min-improvement" => {
                                stability.min_improvement = self.parse_i64()?;
                                0
                            }
                            "min-residency" => {
                                stability.min_residency = self.parse_u64()?;
                                1
                            }
                            "max-deferral" => {
                                stability.max_deferral = self.parse_u64()?;
                                2
                            }
                            _ => return Err(self.err_prev()),
                        };
                        if stability_seen[which] {
                            return Err(self.err_prev());
                        }
                        stability_seen[which] = true;
                        index += 1;
                        self.optional_semi();
                    }
                    self.expect(TokenKind::RBrace)?;
                    // An empty stability block declares nothing; require at
                    // least one threshold so a typo cannot silently keep the
                    // defaults.
                    if index == 0 {
                        return Err(self.err());
                    }
                }
                Some(TokenKind::Ident) if key == "candidate" => {
                    let candidate_name = self.expect_ident()?;
                    let candidate = self.parse_chromatic_candidate(candidate_name)?;
                    if candidates.iter().any(|c| c.name == candidate.name) {
                        return Err(self.err_prev());
                    }
                    candidates.push(candidate);
                }
                _ => return Err(self.err_prev()),
            }
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;
        let Some(primary) = primary else {
            return Err(self.err_prev());
        };
        self.anchor = start;
        Ok(ChromaticsDecl {
            name,
            primary,
            fallback,
            preserve_slots: preserve_slots.unwrap_or(true),
            ordered_slots: ordered_slots.unwrap_or(false),
            exclude_native: exclude_native.unwrap_or(true),
            exclude_duplicates: exclude_duplicates.unwrap_or(true),
            stability,
            candidates,
            range: SourceRange {
                start,
                end: self.tokens[self.i - 1].byte_end,
            },
        })
    }

    /// Body of one `candidate Name { … }` declaration (§12.1).
    fn parse_chromatic_candidate(&mut self, name: String) -> Result<ChromaticCandidate, ParseError> {
        let start = self.anchor;
        self.expect(TokenKind::LBrace)?;
        let mut intent: Option<String> = None;
        let mut label: Option<String> = None;
        let mut symbol: Option<String> = None;
        let mut description: Option<String> = None;
        let mut entity: Option<String> = None;
        let mut risk = Risk::default();
        let mut risk_seen = false;
        let mut confirmation = false;
        let mut confirm_seen = false;
        let mut native: Option<String> = None;
        let mut pinned: Option<String> = None;
        let mut rank: Option<u32> = None;
        let mut favourite: Option<u32> = None;
        let mut position: Option<u32> = None;
        let mut cost: Option<u32> = None;
        let mut probability: Option<u32> = None;
        let mut contextual: Option<i64> = None;
        let mut urgency: Option<i64> = None;
        let mut preference: Option<i64> = None;
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            macro_rules! once {
                ($slot:expr) => {
                    if $slot.is_some() {
                        return Err(self.err_prev());
                    }
                };
            }
            match key.as_str() {
                "intent" => {
                    once!(intent);
                    intent = Some(self.expect_ident()?);
                }
                "label" => {
                    once!(label);
                    label = Some(self.expect(TokenKind::Str)?.text);
                }
                "symbol" => {
                    once!(symbol);
                    symbol = Some(self.expect_ident()?);
                }
                "describe" => {
                    once!(description);
                    let text = self.expect(TokenKind::Str)?.text;
                    // An empty description is a typo, not a decision (§21:
                    // supplementary to the label, never a placeholder).
                    if text.is_empty() {
                        return Err(self.err_prev());
                    }
                    description = Some(text);
                }
                "entity" | "native" => {
                    let slot = if key == "entity" { &mut entity } else { &mut native };
                    once!(slot);
                    *slot = Some(self.expect(TokenKind::Str)?.text);
                }
                "risk" => {
                    if risk_seen {
                        return Err(self.err_prev());
                    }
                    risk_seen = true;
                    let parsed =
                        Risk::parse(&self.expect_ident()?).ok_or_else(|| self.err_prev())?;
                    risk = parsed;
                }
                "confirm" => {
                    if confirm_seen {
                        return Err(self.err_prev());
                    }
                    confirm_seen = true;
                    confirmation = self.parse_bool()?;
                }
                "pin" => {
                    once!(pinned);
                    pinned = Some(self.expect_ident()?);
                }
                "rank" | "favourite" | "position" | "cost" | "probability" => {
                    let slot = match key.as_str() {
                        "rank" => &mut rank,
                        "favourite" => &mut favourite,
                        "position" => &mut position,
                        "cost" => &mut cost,
                        _ => &mut probability,
                    };
                    once!(slot);
                    *slot = Some(self.parse_u32()?);
                }
                "contextual" | "urgency" | "preference" => {
                    let slot = match key.as_str() {
                        "contextual" => &mut contextual,
                        "urgency" => &mut urgency,
                        _ => &mut preference,
                    };
                    once!(slot);
                    *slot = Some(self.parse_i64()?);
                }
                _ => return Err(self.err_prev()),
            }
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;
        // `intent` and `label` are the declaration's substance (§7, §21);
        // everything else has an engine default. Missing either is a syntax
        // error at the candidate, not a runtime surprise.
        let Some(intent) = intent else {
            return Err(self.err_prev());
        };
        let Some(label) = label.filter(|l| !l.is_empty()) else {
            return Err(self.err_prev());
        };
        self.anchor = start;
        Ok(ChromaticCandidate {
            name,
            intent,
            label,
            symbol,
            description,
            entity,
            risk,
            confirmation,
            native,
            rank,
            favourite,
            position,
            cost,
            probability,
            contextual,
            urgency,
            preference,
            pinned,
            range: SourceRange {
                start,
                end: self.tokens[self.i - 1].byte_end,
            },
        })
    }

    /// `{ key: type = literal session|durable; ... }` after `@state`.
    fn parse_state_block(&mut self) -> Result<Vec<StateDecl>, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut decls: Vec<StateDecl> = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            self.mark();
            let start = self.peek().map(|t| t.byte_start).unwrap_or(self.source_len);
            let key = self.expect_ident()?;
            if decls.iter().any(|decl| decl.key == key) {
                return Err(self.err_prev());
            }
            self.expect(TokenKind::Colon)?;
            let value_type = self.parse_interaction_type()?;
            self.expect(TokenKind::Equal)?;
            let initial = match (value_type, self.peek().map(|t| t.kind)) {
                (InteractionType::Int, Some(TokenKind::Number)) => {
                    let text = self.bump().ok_or_else(|| self.err())?.text;
                    // Canonical decimal only: no `_`, `+`, fraction or `-0`.
                    if !is_canonical_decimal(&text) {
                        return Err(self.err_prev());
                    }
                    InteractionValue::Int(text.parse().map_err(|_| self.err_prev())?)
                }
                (InteractionType::Text, Some(TokenKind::Str)) => {
                    InteractionValue::Text(self.bump().ok_or_else(|| self.err())?.text)
                }
                (InteractionType::Bool, Some(TokenKind::Ident)) => {
                    match self.bump().ok_or_else(|| self.err())?.text.as_str() {
                        "true" => InteractionValue::Bool(true),
                        "false" => InteractionValue::Bool(false),
                        _ => return Err(self.err_prev()),
                    }
                }
                _ => return Err(self.err()),
            };
            let lifetime = match self.expect_ident()?.as_str() {
                "session" => StateLifetime::Session,
                "durable" => StateLifetime::Durable,
                _ => return Err(self.err_prev()),
            };
            let end = self.expect(TokenKind::Semicolon)?.byte_end;
            let range = SourceRange { start, end };
            self.entries.push(SourceEntry {
                kind: "state-key".into(),
                name: key.clone(),
                scene_path: None,
                semantic_id: None,
                range,
                source_digest: [0; 32],
            });
            decls.push(StateDecl {
                key,
                value_type,
                initial,
                lifetime,
                range,
            });
        }
        self.expect(TokenKind::RBrace)?;
        Ok(decls)
    }

    /// `@platform android { "media.playback": choice-use("native", "web"); }`.
    /// Every entry is scoped to one named platform and one feature. The
    /// ontology otherwise remains platform `any`.
    fn parse_platform_block(
        &mut self,
        platform: &str,
    ) -> Result<Vec<PlatformException>, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut exceptions = Vec::new();
        while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
            let start = self.peek().ok_or_else(|| self.err())?.byte_start;
            let feature = match self.peek().map(|token| token.kind) {
                Some(TokenKind::Ident) => self.expect_ident()?,
                Some(TokenKind::Str) => self.expect(TokenKind::Str)?.text,
                _ => return Err(self.err()),
            };
            if feature.is_empty()
                || exceptions
                    .iter()
                    .any(|rule: &PlatformException| rule.feature == feature)
            {
                return Err(self.err_prev());
            }
            self.expect(TokenKind::Colon)?;
            let use_kind = match self.expect_ident()?.as_str() {
                "only-use" => PlatformUse::OnlyUse,
                "choice-use" => PlatformUse::ChoiceUse,
                "fallback" => PlatformUse::Fallback,
                "fallbacks" => PlatformUse::Fallbacks,
                "must-use" => PlatformUse::MustUse,
                "invade" => PlatformUse::Invade,
                "cede" => PlatformUse::Cede,
                _ => return Err(self.err_prev()),
            };
            let mut options = Vec::new();
            if use_kind != PlatformUse::Cede {
                self.expect(TokenKind::LParen)?;
                loop {
                    let option = self.expect(TokenKind::Str)?.text;
                    if option.is_empty() || options.contains(&option) {
                        return Err(self.err_prev());
                    }
                    options.push(option);
                    if !matches!(self.peek().map(|token| token.kind), Some(TokenKind::Comma)) {
                        break;
                    }
                    self.bump();
                }
                self.expect(TokenKind::RParen)?;
            }
            let valid_count = match use_kind {
                PlatformUse::ChoiceUse | PlatformUse::Fallbacks => options.len() >= 2,
                PlatformUse::Cede => options.is_empty(),
                _ => options.len() == 1,
            };
            if !valid_count {
                return Err(self.err_prev());
            }
            let end = self.expect(TokenKind::Semicolon)?.byte_end;
            exceptions.push(PlatformException {
                platform: platform.to_owned(),
                feature,
                use_kind,
                options,
                range: SourceRange { start, end },
            });
        }
        if exceptions.is_empty() {
            return Err(self.err());
        }
        self.expect(TokenKind::RBrace)?;
        Ok(exceptions)
    }

    /// `@desktop { ... }` (decision D16). Keys are optional. Semicolons are
    /// required. Combination rules are checked later by [`validate_desktop`].
    fn parse_desktop_block(&mut self) -> Result<DesktopBody, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut state = None;
        let mut position = None;
        let mut extent_dip = None;
        let mut controls = None;
        while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?.replace('-', "_");
            match key.as_str() {
                "state" => {
                    if state.is_some() {
                        return Err(self.err_prev());
                    }
                    self.expect(TokenKind::Colon)?;
                    state = Some(match self.expect_ident()?.as_str() {
                        "fullscreen" => DesktopState::Fullscreen,
                        "maximized" => DesktopState::Maximized,
                        "windowed" => DesktopState::Windowed,
                        _ => return Err(self.err_prev()),
                    });
                }
                "position" => {
                    if position.is_some() {
                        return Err(self.err_prev());
                    }
                    self.expect(TokenKind::Colon)?;
                    position = Some(
                        if self.peek().is_some_and(|token| {
                            token.kind == TokenKind::Ident && token.text == "center"
                        }) {
                            self.bump();
                            DesktopPosition::Center
                        } else {
                            let (x, y) = self.parse_dip_pair(true)?;
                            DesktopPosition::Dip { x, y }
                        },
                    );
                }
                "extent_dip" => {
                    if extent_dip.is_some() {
                        return Err(self.err_prev());
                    }
                    self.expect(TokenKind::Colon)?;
                    let (width, height) = self.parse_dip_pair(false)?;
                    if !(1..=desktop::MAX_EXTENT_DIP).contains(&width)
                        || !(1..=desktop::MAX_EXTENT_DIP).contains(&height)
                    {
                        return Err(self.err_prev());
                    }
                    extent_dip = Some((width, height));
                }
                "controls" => {
                    if controls.is_some() {
                        return Err(self.err_prev());
                    }
                    self.expect(TokenKind::Colon)?;
                    controls = Some(self.parse_desktop_controls()?);
                }
                _ => return Err(self.err_prev()),
            }
            self.expect(TokenKind::Semicolon)?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(DesktopBody {
            state,
            position,
            extent_dip,
            controls,
        })
    }

    fn parse_desktop_controls(&mut self) -> Result<ChromeChoice, ParseError> {
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Ident && token.text == "library")
        {
            self.bump();
            let name = self.expect(TokenKind::Str)?.text;
            if name.is_empty() {
                return Err(self.err_prev());
            }
            return Ok(ChromeChoice::Library(name));
        }
        self.expect(TokenKind::LBrace)?;
        let mut draft = desktop::ChromeDraft::default();
        while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?.replace('-', "_");
            self.expect(TokenKind::Colon)?;
            let colour = self.parse_hex()?;
            let slot = match key.as_str() {
                "bar" => &mut draft.bar,
                "glyph" => &mut draft.glyph,
                "close" => &mut draft.close,
                "close_glyph" => &mut draft.close_glyph,
                "desk" => &mut draft.desk,
                _ => return Err(self.err_prev()),
            };
            if slot.is_some() {
                return Err(self.err_prev());
            }
            *slot = Some(colour);
            self.expect(TokenKind::Semicolon)?;
        }
        self.expect(TokenKind::RBrace)?;
        Ok(ChromeChoice::Colours(draft))
    }

    /// A canonical DIP integer: optional leading `-` when `signed`, digits
    /// only, no `+`, `.`, `_`, leading zero or `-0`.
    fn parse_dip_int(&mut self, signed: bool) -> Result<i32, ParseError> {
        let Some(token) = self.peek() else {
            return Err(self.err());
        };
        if token.kind != TokenKind::Number || !canonical_dip(&token.text, signed) {
            return Err(self.err());
        }
        let text = self.bump().expect("invariant").text;
        text.parse::<i32>().map_err(|_| self.err_prev())
    }

    fn parse_dip_pair(&mut self, signed: bool) -> Result<(i32, i32), ParseError> {
        self.expect(TokenKind::LParen)?;
        let x = self.parse_dip_int(signed)?;
        self.expect(TokenKind::Comma)?;
        let y = self.parse_dip_int(signed)?;
        self.expect(TokenKind::RParen)?;
        Ok((x, y))
    }

    /// Canonical integer digits on a Number token: at most one leading '-',
    /// digits only, no leading zeros. Mirrors `canonical_dip`.
    fn canonical_integer(text: &str) -> bool {
        let digits = text.strip_prefix('-').unwrap_or(text);
        !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
            && (digits.len() == 1 || !digits.starts_with('0'))
    }

    fn parse_bool(&mut self) -> Result<bool, ParseError> {
        let value = self.expect_ident()?;
        match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(self.err_prev()),
        }
    }

    /// Chromatics integers: canonical form, in range. Non-integral authored
    /// numbers are refused so allocation stays exact and portable (§9).
    fn parse_u32(&mut self) -> Result<u32, ParseError> {
        let Some(token) = self.peek() else {
            return Err(self.err());
        };
        if token.kind != TokenKind::Number || !Self::canonical_integer(&token.text) {
            return Err(self.err());
        }
        let text = self.bump().expect("invariant").text;
        text.parse::<u32>().map_err(|_| self.err_prev())
    }

    fn parse_u64(&mut self) -> Result<u64, ParseError> {
        let Some(token) = self.peek() else {
            return Err(self.err());
        };
        if token.kind != TokenKind::Number || !Self::canonical_integer(&token.text) {
            return Err(self.err());
        }
        let text = self.bump().expect("invariant").text;
        text.parse::<u64>().map_err(|_| self.err_prev())
    }

    fn parse_i64(&mut self) -> Result<i64, ParseError> {
        let Some(token) = self.peek() else {
            return Err(self.err());
        };
        if token.kind != TokenKind::Number || !Self::canonical_integer(&token.text) {
            return Err(self.err());
        }
        let text = self.bump().expect("invariant").text;
        text.parse::<i64>().map_err(|_| self.err_prev())
    }

    /// `@persistence { cache_targets: null; }` or an explicit platform map,
    /// optionally followed by `migration: copy|move|none;` or a platform map
    /// of those (decision D13). The schema is closed and string-valued: a
    /// target compiler resolves its own entry against the adapters it
    /// implements or refuses it; targets not listed keep their default
    /// local store (decision D11).
    #[allow(clippy::type_complexity)]
    fn parse_persistence_block(
        &mut self,
    ) -> Result<(Option<BTreeMap<String, String>>, Option<MigrationRule>), ParseError> {
        self.expect(TokenKind::LBrace)?;
        self.expect_ident_value("cache_targets")?;
        self.expect(TokenKind::Colon)?;
        let targets = match self.peek().map(|token| token.kind) {
            Some(TokenKind::Ident) if self.peek().is_some_and(|token| token.text == "null") => {
                self.bump();
                None
            }
            Some(TokenKind::LBrace) => {
                self.bump();
                let mut targets = BTreeMap::new();
                while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
                    let platform = self.expect_ident()?;
                    if !PERSISTENCE_PLATFORMS.contains(&platform.as_str())
                        || targets.contains_key(&platform)
                    {
                        return Err(self.err_prev());
                    }
                    self.expect(TokenKind::Colon)?;
                    let adapter = self.expect(TokenKind::Str)?.text;
                    if adapter.is_empty() {
                        return Err(self.err_prev());
                    }
                    targets.insert(platform, adapter);
                    self.expect(TokenKind::Semicolon)?;
                }
                self.expect(TokenKind::RBrace)?;
                if targets.is_empty() {
                    return Err(self.err_prev());
                }
                Some(targets)
            }
            _ => return Err(self.err()),
        };
        self.optional_semi();
        let mut migration = None;
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Ident && token.text == "migration")
        {
            self.bump();
            self.expect(TokenKind::Colon)?;
            migration = Some(match self.peek().map(|token| token.kind) {
                Some(TokenKind::LBrace) => {
                    self.bump();
                    let mut rules = BTreeMap::new();
                    while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
                        let platform = self.expect_ident()?;
                        if !PERSISTENCE_PLATFORMS.contains(&platform.as_str())
                            || rules.contains_key(&platform)
                        {
                            return Err(self.err_prev());
                        }
                        self.expect(TokenKind::Colon)?;
                        let rule = self.parse_migration()?;
                        rules.insert(platform, rule);
                        self.expect(TokenKind::Semicolon)?;
                    }
                    self.expect(TokenKind::RBrace)?;
                    if rules.is_empty() {
                        return Err(self.err_prev());
                    }
                    MigrationRule::PerPlatform(rules)
                }
                _ => MigrationRule::All(self.parse_migration()?),
            });
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;
        Ok((targets, migration))
    }

    fn parse_migration(&mut self) -> Result<Migration, ParseError> {
        match self.expect_ident()?.as_str() {
            "copy" => Ok(Migration::Copy),
            "move" => Ok(Migration::Move),
            "none" => Ok(Migration::None),
            _ => Err(self.err_prev()),
        }
    }

    /// `@iden { label: "..."; hint: "..."; }` (decision D14). `label` is
    /// required; each key at most once. On GSSH/5 the label and hint values
    /// may be phrasing. Reserved keys are kept with their ranges for strict
    /// admission to refuse; any other key is an error.
    fn parse_iden_block(&mut self) -> Result<Iden, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut label = None;
        let mut hint = None;
        let mut label_phrasing = None;
        let mut hint_phrasing = None;
        let mut reserved: Vec<(String, SourceRange)> = Vec::new();
        while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
            let start = self.peek().ok_or_else(|| self.err())?.byte_start;
            let key = self.expect_ident()?.replace('-', "_");
            self.expect(TokenKind::Colon)?;
            let phrased = self.peek().is_some_and(|token| {
                token.kind == TokenKind::AtWord || token.kind == TokenKind::LBrace
            });
            if phrased {
                if !matches!(key.as_str(), "label" | "hint") {
                    return Err(self.err());
                }
                let phrasing = self.parse_phrasing()?;
                self.expect(TokenKind::Semicolon)?;
                let (text_slot, phrase_slot) = match key.as_str() {
                    "label" => (&mut label, &mut label_phrasing),
                    "hint" => (&mut hint, &mut hint_phrasing),
                    _ => return Err(self.err_prev()),
                };
                if text_slot.is_some() || phrase_slot.is_some() {
                    return Err(self.err_prev());
                }
                *phrase_slot = Some(phrasing);
                if key == "label" {
                    *text_slot = Some(String::new());
                }
                continue;
            }
            let value = self.expect(TokenKind::Str)?.text;
            let end = self.expect(TokenKind::Semicolon)?.byte_end;
            let slot = match key.as_str() {
                "label" => &mut label,
                "hint" => &mut hint,
                other if RESERVED_IDEN_KEYS.contains(&other) => {
                    if reserved.iter().any(|(name, _)| name == other) {
                        return Err(self.err_prev());
                    }
                    reserved.push((key, SourceRange { start, end }));
                    continue;
                }
                _ => return Err(self.err_prev()),
            };
            if slot.is_some() {
                return Err(self.err_prev());
            }
            *slot = Some(value);
        }
        self.expect(TokenKind::RBrace)?;
        let Some(label) = label else {
            return Err(self.err_prev());
        };
        let mut iden = Iden::new(label, hint);
        iden.label_phrasing = label_phrasing;
        iden.hint_phrasing = hint_phrasing;
        iden.reserved = reserved;
        Ok(iden)
    }

    fn parse_effect_kind(&mut self) -> Result<EffectKind, ParseError> {
        match self.expect_ident()?.as_str() {
            "network" => Ok(EffectKind::Network),
            "filesystem" => Ok(EffectKind::FileSystem),
            "clock" => Ok(EffectKind::Clock),
            "random" => Ok(EffectKind::Random),
            "global_mutable" => Ok(EffectKind::GlobalMutable),
            _ => Err(self.err_prev()),
        }
    }

    fn parse_interaction_type(&mut self) -> Result<InteractionType, ParseError> {
        let ty = self.expect_ident()?;
        match ty.as_str() {
            "int" => Ok(InteractionType::Int),
            "text" => Ok(InteractionType::Text),
            "bool" => Ok(InteractionType::Bool),
            _ => Err(self.err_prev()),
        }
    }

    fn parse_interaction_expr(&mut self) -> Result<InteractionExpr, ParseError> {
        let mut expr = self.parse_interaction_atom()?;
        while matches!(self.peek().map(|t| t.kind), Some(TokenKind::Plus)) {
            self.bump();
            expr = InteractionExpr::Add(Box::new(expr), Box::new(self.parse_interaction_atom()?));
        }
        Ok(expr)
    }

    fn parse_interaction_atom(&mut self) -> Result<InteractionExpr, ParseError> {
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::Number) => {
                let text = self.bump().ok_or_else(|| self.err())?.text.replace('_', "");
                let value = text.parse::<i64>().map_err(|_| self.err_prev())?;
                Ok(InteractionExpr::Literal(InteractionValue::Int(value)))
            }
            Some(TokenKind::Str) => Ok(InteractionExpr::Literal(InteractionValue::Text(
                self.bump().ok_or_else(|| self.err())?.text,
            ))),
            Some(TokenKind::Ident) => {
                let head = self.expect_ident()?;
                match head.as_str() {
                    "true" => Ok(InteractionExpr::Literal(InteractionValue::Bool(true))),
                    "false" => Ok(InteractionExpr::Literal(InteractionValue::Bool(false))),
                    "state" | "input" | "effect" => {
                        self.expect(TokenKind::Dot)?;
                        let key = self.expect_ident()?;
                        if head == "state" {
                            Ok(InteractionExpr::State(key))
                        } else if head == "input" {
                            Ok(InteractionExpr::Input(key))
                        } else {
                            Ok(InteractionExpr::Effect(key))
                        }
                    }
                    _ => Err(self.err_prev()),
                }
            }
            _ => Err(self.err()),
        }
    }

    fn expect_ident_value(&mut self, value: &str) -> Result<(), ParseError> {
        if self
            .peek()
            .is_some_and(|t| t.kind == TokenKind::Ident && t.text == value)
        {
            self.bump();
            Ok(())
        } else {
            Err(self.err())
        }
    }

    fn optional_semi(&mut self) {
        if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Semicolon)) {
            self.bump();
        }
    }

    fn skip_block(&mut self) -> Result<(), ParseError> {
        // Consume optional identifier(s) up to `{`, then balanced braces.
        while matches!(self.peek().map(|t| t.kind), Some(TokenKind::Ident)) {
            self.bump();
        }
        self.expect(TokenKind::LBrace)?;
        let mut depth = 1usize;
        while depth > 0 {
            match self.bump().map(|t| t.kind) {
                Some(TokenKind::LBrace) => depth += 1,
                Some(TokenKind::RBrace) => depth -= 1,
                Some(_) => {}
                None => return Err(self.err()),
            }
        }
        Ok(())
    }

    fn parse_gestures(&mut self, cell: &mut SceneCell) -> Result<(), ParseError> {
        self.expect_ident_value("gestures")?;
        self.expect(TokenKind::LBrace)?;
        let mut policy = GesturePolicy::default();
        let block_start = self.anchor;
        while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
            self.mark();
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let decision = match self.expect_ident()?.as_str() {
                "allow" => GestureDecision::Allow,
                "deny" => GestureDecision::Deny,
                _ => return Err(self.err_prev()),
            };
            let slot = match key.as_str() {
                "copy" => &mut policy.copy,
                "cut" => &mut policy.cut,
                "paste" => &mut policy.paste,
                _ => return Err(self.err_prev()),
            };
            if slot.is_some() {
                return Err(self.err_prev());
            }
            *slot = Some(decision);
            self.optional_semi();
        }
        self.expect(TokenKind::RBrace)?;
        self.optional_semi();
        self.anchor = block_start;
        if cell.gestures.is_some() {
            return Err(self.err());
        }
        cell.gestures = Some(policy);
        Ok(())
    }

    fn parse_property(&mut self, cell: &mut SceneCell) -> Result<(), ParseError> {
        let key = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        match key.as_str() {
            "id" => cell.authored_id = Some(self.expect(TokenKind::Str)?.text),
            "position" => cell.position = Some(self.parse_vec3()?),
            "extent" => cell.extent = Some(self.parse_vec3()?),
            "position_p" => cell.position_p = Some(self.parse_vec3()?),
            "extent_p" => cell.extent_p = Some(self.parse_vec3()?),
            "fill" => cell.fill = Some(self.parse_hex()?),
            "stroke" => cell.stroke = Some(self.parse_hex()?),
            "stroke-width" | "stroke_width" => cell.stroke_width = Some(self.parse_number()?),
            "text" => self.parse_static_copy(&mut cell.text, &mut cell.text_phrasing)?,
            "text-size" | "text_size" => cell.text_size = Some(self.parse_number()?),
            "text-size-p" | "text_size_p" => cell.text_size_p = Some(self.parse_number()?),
            "stroke-width-p" | "stroke_width_p" => cell.stroke_width_p = Some(self.parse_number()?),
            // `wrap` is a positive integer column count. 0, negatives,
            // non-integers and out-of-range values are parse errors — a
            // malformed wrap must not silently disable the feature.
            "wrap" => {
                let text = match self.peek() {
                    Some(t) if t.kind == TokenKind::Number => {
                        self.bump().expect("invariant").text.replace('_', "")
                    }
                    _ => return Err(self.err()),
                };
                let value: i64 = text.parse().map_err(|_| self.err_prev())?;
                if value <= 0 || value > u32::MAX as i64 {
                    return Err(self.err_prev());
                }
                cell.wrap = Some(value as u32);
            }
            "scroll" => {
                let value = self.expect_ident()?;
                cell.scroll = match value.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(self.err_prev()),
                };
            }
            "text-anchor" | "text_anchor" => {
                let name = self.expect_ident()?;
                cell.anchor = Some(match name.as_str() {
                    "start" => Anchor::Start,
                    "middle" => Anchor::Middle,
                    "end" => Anchor::End,
                    _ => return Err(self.err_prev()),
                });
            }
            "placeholder" => {
                self.parse_static_copy(&mut cell.placeholder, &mut cell.placeholder_phrasing)?
            }
            "text-from" | "text_from" => cell.text_from = Some(self.parse_state_ref()?),
            "present-when" | "present_when" => {
                cell.present_when = Some(self.parse_condition()?);
            }
            "fill-to" | "fill_to" => cell.fill_to = Some(self.parse_hex()?),
            "fill-period-s" | "fill_period_s" => cell.fill_period_s = Some(self.parse_number()?),
            "appear-s" | "appear_s" => cell.appear_s = Some(self.parse_number()?),
            "appear-from-p" | "appear_from_p" => cell.appear_from_p = Some(self.parse_vec3()?),
            "max-length" | "max_length" => {
                let text = match self.peek() {
                    Some(token) if token.kind == TokenKind::Number => {
                        self.bump().expect("invariant").text.replace('_', "")
                    }
                    _ => return Err(self.err()),
                };
                let value: i64 = text.parse().map_err(|_| self.err_prev())?;
                if value < 0 {
                    return Err(self.err_prev());
                }
                cell.max_length = Some(value);
            }
            // Unknown properties are consumed as scalar values so the
            // language can grow without breaking older parsers.
            _ => {
                self.skip_value()?;
            }
        }
        // Optional single-line separator.
        if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Semicolon)) {
            self.bump();
        }
        Ok(())
    }

    /// A static string, or GSSH/5 phrasing. The two are mutually exclusive:
    /// phrasing leaves the shared string empty so layout does not pick a class.
    fn parse_static_copy(
        &mut self,
        text: &mut Option<String>,
        phrasing: &mut Option<Phrasing>,
    ) -> Result<(), ParseError> {
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Str)
        {
            *text = Some(self.expect(TokenKind::Str)?.text);
        } else {
            *phrasing = Some(self.parse_phrasing()?);
        }
        Ok(())
    }

    /// `@universal "..."`, or `{ @universal "..."; @tv "..."; }`.
    /// A class specifier outside a group, a duplicate, an unknown specifier
    /// and an empty group are syntax errors.
    fn parse_phrasing(&mut self) -> Result<Phrasing, ParseError> {
        let start = self.peek().ok_or_else(|| self.err())?.byte_start;
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::AtWord && token.text == "universal")
        {
            self.bump();
            let text = self.expect(TokenKind::Str)?.text;
            let end = self.tokens[self.i - 1].byte_end;
            return Ok(Phrasing {
                universal: Some(text),
                desktop: None,
                mobile: None,
                tv: None,
                web: None,
                range: SourceRange { start, end },
            });
        }
        self.expect(TokenKind::LBrace)?;
        let mut phrasing = Phrasing {
            universal: None,
            desktop: None,
            mobile: None,
            tv: None,
            web: None,
            range: SourceRange { start, end: start },
        };
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::RBrace)
        {
            self.bump();
            return Err(self.err_prev());
        }
        while !matches!(self.peek().map(|token| token.kind), Some(TokenKind::RBrace)) {
            let word = match self.peek() {
                Some(token) if token.kind == TokenKind::AtWord => {
                    let word = token.text.clone();
                    self.bump();
                    word
                }
                _ => return Err(self.err()),
            };
            let slot = match word.as_str() {
                "universal" => &mut phrasing.universal,
                "desktop" => &mut phrasing.desktop,
                "mobile" => &mut phrasing.mobile,
                "tv" => &mut phrasing.tv,
                "web" => &mut phrasing.web,
                _ => return Err(self.err_prev()),
            };
            if slot.is_some() {
                return Err(self.err_prev());
            }
            let text = self.expect(TokenKind::Str)?.text;
            self.expect(TokenKind::Semicolon)?;
            *slot = Some(text);
        }
        let end = self.expect(TokenKind::RBrace)?.byte_end;
        phrasing.range.end = end;
        Ok(phrasing)
    }

    fn parse_state_ref(&mut self) -> Result<String, ParseError> {
        if self.expect_ident()? != "state" {
            return Err(self.err_prev());
        }
        self.expect(TokenKind::Dot)?;
        self.expect_ident()
    }

    fn parse_vec3(&mut self) -> Result<Vec3, ParseError> {
        self.expect(TokenKind::LParen)?;
        let x = self.parse_number()?;
        self.expect(TokenKind::Comma)?;
        let y = self.parse_number()?;
        let z = if matches!(self.peek().map(|t| t.kind), Some(TokenKind::Comma)) {
            self.bump();
            self.parse_number()?
        } else {
            0.0
        };
        self.expect(TokenKind::RParen)?;
        Ok(Vec3 { x, y, z })
    }

    fn parse_number(&mut self) -> Result<f64, ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::Number => {
                let text = self.bump().expect("invariant").text.replace('_', "");
                let value = text.parse::<f64>().map_err(|_| self.err_prev())?;
                // Finite authored coordinates and sizes are required for
                // deterministic layout. Rust accepts sufficiently large
                // decimal literals as infinity, which otherwise flows into
                // geometry and can silently poison hit testing and previews.
                if !value.is_finite() {
                    return Err(self.err_prev());
                }
                Ok(value)
            }
            _ => Err(self.err()),
        }
    }

    fn parse_hex(&mut self) -> Result<u32, ParseError> {
        match self.peek() {
            Some(t) if t.kind == TokenKind::Hex => {
                let hex = self.bump().expect("invariant").text[1..].to_string();
                let v = match hex.len() {
                    3 => {
                        let r = u32::from_str_radix(&hex[0..1], 16).unwrap_or(0);
                        let g = u32::from_str_radix(&hex[1..2], 16).unwrap_or(0);
                        let b = u32::from_str_radix(&hex[2..3], 16).unwrap_or(0);
                        (r << 20) | (r << 16) | (g << 12) | (g << 8) | (b << 4) | b
                    }
                    6 => u32::from_str_radix(&hex, 16).unwrap_or(0),
                    8 => u32::from_str_radix(&hex[..6], 16).unwrap_or(0),
                    _ => return Err(self.err_prev()),
                };
                Ok(v)
            }
            _ => Err(self.err()),
        }
    }

    /// `state.<key> is <literal>` or `state.<key> at-least <int>`.
    fn parse_condition(&mut self) -> Result<Condition, ParseError> {
        let key = self.parse_state_ref()?;
        let op = match self.expect_ident()?.as_str() {
            "is" => ConditionOp::Is,
            "at-least" | "at_least" => ConditionOp::AtLeast,
            _ => return Err(self.err_prev()),
        };
        let value = match self.peek().map(|t| t.kind) {
            Some(TokenKind::Number) => {
                let text = self.bump().ok_or_else(|| self.err())?.text;
                if !is_canonical_decimal(&text) {
                    return Err(self.err_prev());
                }
                InteractionValue::Int(text.parse().map_err(|_| self.err_prev())?)
            }
            Some(TokenKind::Str) => {
                InteractionValue::Text(self.bump().ok_or_else(|| self.err())?.text)
            }
            Some(TokenKind::Ident) => match self.expect_ident()?.as_str() {
                "true" => InteractionValue::Bool(true),
                "false" => InteractionValue::Bool(false),
                _ => return Err(self.err_prev()),
            },
            _ => return Err(self.err()),
        };
        Ok(Condition { key, op, value })
    }

    /// `(n, n, ...)` with between `min` and `max` numbers.
    fn parse_numbers(&mut self, min: usize, max: usize) -> Result<Vec<f64>, ParseError> {
        self.expect(TokenKind::LParen)?;
        let mut v = vec![self.parse_number()?];
        while matches!(self.peek().map(|t| t.kind), Some(TokenKind::Comma)) {
            self.bump();
            v.push(self.parse_number()?);
        }
        self.expect(TokenKind::RParen)?;
        if v.len() < min || v.len() > max {
            return Err(self.err_prev());
        }
        Ok(v)
    }

    fn parse_path_string(&mut self) -> Result<Vec<crate::graphic::PathCmd>, ParseError> {
        let text = self.expect(TokenKind::Str)?.text;
        crate::graphic::parse_path_data(&text).ok_or_else(|| self.err_prev())
    }

    /// `@graphic name { id; canvas; @shape ...; @dimension ... }`.
    fn parse_graphic(&mut self, name: String) -> Result<crate::graphic::GraphicDecl, ParseError> {
        use crate::graphic::*;
        self.expect(TokenKind::LBrace)?;
        let mut g = GraphicDecl {
            name,
            authored_id: None,
            canvas: (0.0, 0.0),
            shapes: Vec::new(),
            dimensions: Vec::new(),
            range: SourceRange { start: 0, end: 0 },
        };
        let mut canvas = false;
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            match self.peek().map(|t| t.kind) {
                Some(TokenKind::AtWord) => {
                    let word = self.bump().ok_or_else(|| self.err())?.text;
                    let item = self.expect_ident()?;
                    match word.as_str() {
                        "shape" => g.shapes.push(self.parse_shape(item)?),
                        "dimension" => g.dimensions.push(self.parse_graphic_dimension(item)?),
                        _ => return Err(self.err_prev()),
                    }
                }
                _ => {
                    let key = self.expect_ident()?;
                    self.expect(TokenKind::Colon)?;
                    match key.as_str() {
                        "id" if g.authored_id.is_none() => {
                            g.authored_id = Some(self.expect(TokenKind::Str)?.text)
                        }
                        "canvas" if !canvas => {
                            let v = self.parse_numbers(2, 2)?;
                            g.canvas = (v[0], v[1]);
                            canvas = true;
                        }
                        _ => return Err(self.err_prev()),
                    }
                    self.expect(TokenKind::Semicolon)?;
                }
            }
        }
        self.expect(TokenKind::RBrace)?;
        if !canvas {
            return Err(self.err_prev());
        }
        Ok(g)
    }

    fn parse_shape(&mut self, name: String) -> Result<crate::graphic::ShapeDecl, ParseError> {
        use crate::graphic::*;
        self.expect(TokenKind::LBrace)?;
        let mut geometry = None;
        let (mut fill, mut stroke, mut stroke_width) = (None, None, None);
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let dup = match key.as_str() {
                "path" => geometry
                    .replace(Geometry::Path(self.parse_path_string()?))
                    .is_some(),
                "rect" => {
                    let v = self.parse_numbers(4, 5)?;
                    let r = v.get(4).copied().unwrap_or(0.0);
                    geometry
                        .replace(Geometry::Rect {
                            x: v[0],
                            y: v[1],
                            w: v[2],
                            h: v[3],
                            r,
                        })
                        .is_some()
                }
                "ellipse" => {
                    let v = self.parse_numbers(4, 4)?;
                    geometry
                        .replace(Geometry::Ellipse {
                            cx: v[0],
                            cy: v[1],
                            rx: v[2],
                            ry: v[3],
                        })
                        .is_some()
                }
                "fill" => fill.replace(self.parse_hex()?).is_some(),
                "stroke" => stroke.replace(self.parse_hex()?).is_some(),
                "stroke-width" | "stroke_width" => {
                    stroke_width.replace(self.parse_number()?).is_some()
                }
                _ => return Err(self.err_prev()),
            };
            if dup {
                return Err(self.err_prev());
            }
            self.expect(TokenKind::Semicolon)?;
        }
        self.expect(TokenKind::RBrace)?;
        let geometry = geometry.ok_or_else(|| self.err_prev())?;
        Ok(ShapeDecl {
            name,
            geometry,
            fill,
            stroke,
            stroke_width,
        })
    }

    fn parse_selector(&mut self) -> Result<crate::graphic::Selector, ParseError> {
        let first = self.expect_ident()?;
        if first == "all" {
            return Ok(crate::graphic::Selector::All);
        }
        let mut names = vec![first];
        while matches!(self.peek().map(|t| t.kind), Some(TokenKind::Comma))
            && self
                .tokens
                .get(self.i + 1)
                .is_some_and(|t| t.kind == TokenKind::Ident)
        {
            self.bump();
            names.push(self.expect_ident()?);
        }
        Ok(crate::graphic::Selector::Shapes(names))
    }

    fn parse_graphic_dimension(
        &mut self,
        name: String,
    ) -> Result<crate::graphic::GraphicDimension, ParseError> {
        use crate::graphic::*;
        self.expect(TokenKind::LBrace)?;
        let mut d = GraphicDimension {
            name,
            step_s: None,
            values: Vec::new(),
        };
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            if matches!(self.peek().map(|t| t.kind), Some(TokenKind::AtWord)) {
                let word = self.bump().ok_or_else(|| self.err())?.text;
                if word != "value" {
                    return Err(self.err_prev());
                }
                let vname = self.expect_ident()?;
                self.expect(TokenKind::LBrace)?;
                let mut effects = Vec::new();
                while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
                    let key = self.expect_ident()?;
                    self.expect(TokenKind::Colon)?;
                    let e = match key.as_str() {
                        "hide" => GraphicEffect::Hide(self.parse_selector()?),
                        "scale" => {
                            let s = self.parse_selector()?;
                            let v = self.parse_numbers(2, 2)?;
                            GraphicEffect::Scale(s, v[0], v[1])
                        }
                        "move" => {
                            let s = self.parse_selector()?;
                            let v = self.parse_numbers(2, 2)?;
                            GraphicEffect::Move(s, v[0], v[1])
                        }
                        "fade" => {
                            let s = self.parse_selector()?;
                            GraphicEffect::Fade(s, self.parse_number()?)
                        }
                        "paint" => {
                            let s = self.parse_selector()?;
                            GraphicEffect::Paint(s, self.parse_hex()?)
                        }
                        "morph" => {
                            let shape = self.expect_ident()?;
                            let target = self.parse_path_string()?;
                            GraphicEffect::Morph(shape, target, self.parse_number()?)
                        }
                        _ => return Err(self.err_prev()),
                    };
                    effects.push(e);
                    self.expect(TokenKind::Semicolon)?;
                }
                self.expect(TokenKind::RBrace)?;
                d.values.push(GraphicValue {
                    name: vname,
                    effects,
                });
            } else {
                let key = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                if (key != "step-s" && key != "step_s")
                    || d.step_s.is_some()
                    || !d.values.is_empty()
                {
                    return Err(self.err_prev());
                }
                d.step_s = Some(self.parse_number()?);
                self.expect(TokenKind::Semicolon)?;
            }
        }
        self.expect(TokenKind::RBrace)?;
        Ok(d)
    }

    /// `@draw graphic { dimension: "value" [when <condition>]; ... }`.
    fn parse_draw(&mut self, graphic: String) -> Result<crate::graphic::DrawDecl, ParseError> {
        self.expect(TokenKind::LBrace)?;
        let mut rules = Vec::new();
        while !matches!(self.peek().map(|t| t.kind), Some(TokenKind::RBrace)) {
            let dimension = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value = self.expect(TokenKind::Str)?.text;
            let when = if self
                .peek()
                .is_some_and(|t| t.kind == TokenKind::Ident && t.text == "when")
            {
                self.bump();
                Some(self.parse_condition()?)
            } else {
                None
            };
            self.expect(TokenKind::Semicolon)?;
            rules.push(crate::graphic::DrawRule {
                dimension,
                value,
                when,
            });
        }
        self.expect(TokenKind::RBrace)?;
        Ok(crate::graphic::DrawDecl {
            graphic,
            rules,
            range: SourceRange { start: 0, end: 0 },
        })
    }

    fn skip_value(&mut self) -> Result<(), ParseError> {
        match self.peek().map(|t| t.kind) {
            Some(TokenKind::LParen) => {
                let _ = self.parse_vec3()?;
                Ok(())
            }
            Some(TokenKind::Str) => {
                self.bump();
                Ok(())
            }
            Some(TokenKind::Hex) => {
                self.bump();
                Ok(())
            }
            Some(TokenKind::Number) => {
                self.bump();
                Ok(())
            }
            Some(TokenKind::Ident) => {
                self.bump();
                Ok(())
            }
            _ => Err(self.err()),
        }
    }
}

#[cfg(test)]
mod desktop_tests {
    use super::*;

    fn scene(body: &str) -> String {
        format!("@ontology demo {{\n{body}\n  @cell root {{ id: \"root\"; text: \"Hi\"; }}\n}}\n")
    }

    #[test]
    fn omission_resolves_to_fullscreen_and_an_empty_block_is_authored() {
        let omitted =
            parse_strict_gssh4("@ontology demo { @cell root { id: \"root\"; } }").unwrap();
        let resolved = omitted.declarations.resolved_desktop().unwrap();
        assert!(omitted.declarations.desktop.is_none());
        assert!(!resolved.authored);
        assert_eq!(resolved.state, DesktopState::Fullscreen);
        assert_eq!(resolved.library_id, Some(desktop::DEFAULT_CHROME));

        let empty = parse_strict_gssh4(&scene("@desktop {}")).unwrap();
        let resolved = empty.declarations.resolved_desktop().unwrap();
        assert!(resolved.authored);
        assert!(!resolved.controls_authored);
        assert_eq!(resolved.state, DesktopState::Fullscreen);
        assert!(parse(&scene("@desktop {}")).is_err());
        assert_eq!(
            parse_strict_gssh3(&scene("@desktop {}")).unwrap_err().code,
            "GY-E1015"
        );
    }

    #[test]
    fn windowed_placement_colours_and_rejections() {
        let windowed = scene(
            "@desktop { state: windowed; position: (-4, 8); extent_dip: (960, 640); controls: library \"goya.chrome.ink\"; }",
        );
        let parsed = parse_strict_gssh4(&windowed).unwrap();
        let resolved = parsed.declarations.resolved_desktop().unwrap();
        assert_eq!(resolved.state, DesktopState::Windowed);
        assert_eq!(resolved.position, DesktopPosition::Dip { x: -4, y: 8 });
        assert_eq!(resolved.extent_dip, Some((960, 640)));
        assert_eq!(resolved.library_id, Some("goya.chrome.ink"));
        assert!(resolved.controls_authored);

        let colours = scene(
            "@desktop { controls: { bar: #112233; glyph: #445566; close: #778899; close-glyph: #aabbcc; desk: #ddeeff; }; }",
        );
        let painted = parse_strict_gssh4(&colours)
            .unwrap()
            .declarations
            .resolved_desktop()
            .unwrap();
        assert!(painted.library_id.is_none());
        assert_eq!(painted.colours.bar, 0x112233);
        assert_eq!(painted.colours.close_glyph, 0xaabbcc);

        let windowed_gap = scene("@desktop { state: windowed; }");
        assert_eq!(
            parse_strict_gssh4(&windowed_gap).unwrap_err().code,
            "GY-E1018"
        );
        assert!(parse_with_source_map(&windowed_gap).is_ok());
        let positioned = scene("@desktop { position: center; }");
        assert_eq!(
            parse_strict_gssh4(&positioned).unwrap_err().code,
            "GY-E1018"
        );
        let extent = scene("@desktop { extent_dip: (200, 100); }");
        assert_eq!(parse_strict_gssh4(&extent).unwrap_err().code, "GY-E1018");
        let unknown = scene("@desktop { controls: library \"goya.chrome.missing\"; }");
        assert_eq!(parse_strict_gssh4(&unknown).unwrap_err().code, "GY-E1018");
        let partial = scene("@desktop { controls: { bar: #112233; }; }");
        assert_eq!(parse_strict_gssh4(&partial).unwrap_err().code, "GY-E1018");
        assert!(parse_with_source_map(&partial).is_ok());

        for bad in [
            "@desktop { extent_dip: (01, 10); state: windowed; }",
            "@desktop { extent_dip: (+10, 10); state: windowed; }",
            "@desktop { position: (-0, 1); state: windowed; extent_dip: (10, 10); }",
            "@desktop { extent_dip: (1.5, 10); state: windowed; }",
            "@desktop { extent_dip: (0, 10); state: windowed; }",
            "@desktop { extent_dip: (8193, 10); state: windowed; }",
            "@desktop { state: windowed; state: fullscreen; }",
            "@desktop { @cell nested { id: \"no\"; } }",
            "@desktop { controls: { bar: #112233; bar: #000000; glyph: #111111; close: #222222; close_glyph: #333333; desk: #444444; }; }",
        ] {
            assert!(parse_with_source_map(&scene(bad)).is_err(), "{bad}");
        }
    }
}

#[cfg(test)]
mod graphic_tests {
    use super::*;

    const CARD: &str = r#"@ontology deck {
      @graphic card {
        id: "graphic.card";
        canvas: (40, 40);
        @shape back { rect: (0, 0, 40, 40, 4); fill: #2b4c7e; }
        @shape mark { ellipse: (20, 20, 6, 6); fill: #f4f1ea; }
        @dimension face {
          step-s: 0.05;
          @value down { hide: mark; }
          @value up { hide: back; }
        }
      }
      @state { open: bool = false session; }
      @cell root { id: "root"; fill: #f4f1ea; fill-to: #e6edf3; fill-period-s: 30;
        @cell art { id: "art"; position_p: (0, 0, 0); extent_p: (1, 1, 0);
          @draw card { face: "up" when state.open is true; face: "down"; }
        }
        @button flip { id: "flip"; present-when: state.open is false; position_p: (0, 0, 0); extent_p: (1, 1, 0); text: "Flip"; }
      }
      @interaction flip { id: "action.flip"; trigger: button.flip.activate; class: D1; state { write open: bool; } input { } set open = true; }
    }"#;

    #[test]
    fn live_profile_admits_a_graphic_and_static_profiles_refuse_it() {
        let parsed = parse_strict_gssh4_live(CARD).expect("live admission");
        assert_eq!(parsed.declarations.graphics.len(), 1);
        assert_eq!(
            parsed
                .scene
                .find_node("art")
                .unwrap()
                .draw
                .as_ref()
                .unwrap()
                .graphic,
            "card"
        );
        assert!(parsed
            .scene
            .find_node("flip")
            .unwrap()
            .present_when
            .is_some());
        assert_eq!(parsed.scene.root.fill_to, Some(0xe6edf3));
        let error = parse_strict_gssh4(CARD).unwrap_err();
        assert_eq!(error.code, "GY-E1012");
        assert!(
            parse(CARD).is_err(),
            "compatibility parse must not drop the graphic"
        );
    }
}

#[cfg(test)]
mod phrasing_tests {
    use super::*;
    use crate::handoff::{admit_v5, admit_v5_live};
    use crate::scene::{layout, PhrasingClass};

    fn scene(body: &str) -> String {
        format!("@ontology demo {{\n{body}\n}}\n")
    }

    #[test]
    fn gssh4_keeps_bare_sentences_and_refuses_specifiers() {
        let bare = scene(
            r#"@cell root { id: "root"; text: "Find the eight pairs."; @iden { label: "Card"; hint: "Press OK"; } }"#,
        );
        let admitted = parse_strict_gssh4(&bare).unwrap();
        assert_eq!(
            admitted.scene.root.text.as_deref(),
            Some("Find the eight pairs.")
        );
        assert!(admitted.scene.root.text_phrasing.is_none());
        assert_eq!(
            admitted.scene.root.iden.as_ref().unwrap().hint.as_deref(),
            Some("Press OK")
        );

        let universal =
            scene(r#"@cell root { id: "root"; text: @universal "Find the eight pairs."; }"#);
        let refused = parse_strict_gssh4(&universal).unwrap_err();
        assert_eq!(refused.code, "GY-E1019");
        assert!(refused.name.contains("GSSH/5"), "{}", refused.name);
        assert_eq!(
            parse_strict_gssh4_live(&universal).unwrap_err().code,
            "GY-E1019"
        );
        assert_eq!(parse_strict(&universal).unwrap_err().code, "GY-E1019");
        assert!(parse(&universal).is_err());
        let kept = parse_with_source_map(&universal).unwrap();
        assert!(kept.scene.root.text.is_none());
        assert_eq!(
            kept.scene
                .root
                .text_phrasing
                .as_ref()
                .unwrap()
                .universal
                .as_deref(),
            Some("Find the eight pairs.")
        );
        let frame = layout(&kept.scene, 320.0, 180.0);
        let node = frame.nodes.iter().find(|node| node.name == "root").unwrap();
        assert!(node.text.is_none());

        let group = scene(
            r#"@cell root { id: "root"; text: { @universal "Find the eight pairs."; @tv "Press OK"; }; }"#,
        );
        assert_eq!(parse_strict_gssh4(&group).unwrap_err().code, "GY-E1019");
        assert_eq!(parse_strict_gssh5(&bare).unwrap_err().code, "GY-E1019");
    }

    #[test]
    fn gssh5_admits_universal_and_a_complete_override_set() {
        let universal =
            scene(r#"@cell root { id: "root"; text: @universal "Find the eight pairs."; }"#);
        let parsed = parse_strict_gssh5(&universal).unwrap();
        let phrasing = parsed.scene.root.text_phrasing.as_ref().unwrap();
        assert!(phrasing.complete());
        assert_eq!(
            phrasing.phrase(PhrasingClass::Web),
            Some("Find the eight pairs.")
        );
        assert_eq!(admit_v5(&universal).unwrap().contract(), "GSSH/5");

        let mixed = scene(
            r#"@cell root { id: "root"; text: { @universal "Find the eight pairs."; @tv "Find the eight pairs. Arrow keys move between cards; OK turns one over."; }; }"#,
        );
        let phrasing = parse_strict_gssh5(&mixed)
            .unwrap()
            .scene
            .root
            .text_phrasing
            .unwrap();
        assert!(phrasing.complete());
        assert_eq!(
            phrasing.phrase(PhrasingClass::Tv),
            Some("Find the eight pairs. Arrow keys move between cards; OK turns one over.")
        );
        assert_eq!(
            phrasing.phrase(PhrasingClass::Desktop),
            Some("Find the eight pairs.")
        );
        assert_eq!(
            phrasing.phrase(PhrasingClass::Web),
            Some("Find the eight pairs.")
        );

        let each = scene(
            r#"@cell root { id: "root"; text: { @desktop "Click a card."; @mobile "Tap a card."; @tv "Press OK to turn a card."; @web "Choose a card."; }; }"#,
        );
        let phrasing = parse_strict_gssh5(&each)
            .unwrap()
            .scene
            .root
            .text_phrasing
            .unwrap();
        assert!(phrasing.universal.is_none());
        assert!(phrasing.complete());
        assert_eq!(phrasing.phrase(PhrasingClass::Mobile), Some("Tap a card."));
        assert_eq!(
            phrasing.phrase(PhrasingClass::Tv),
            Some("Press OK to turn a card.")
        );
    }

    #[test]
    fn a_single_class_without_universal_is_incomplete() {
        let tv_only = scene(r#"@cell root { id: "root"; text: { @tv "Press OK"; }; }"#);
        let error = parse_strict_gssh5(&tv_only).unwrap_err();
        assert_eq!(error.code, "GY-E1019");
        assert!(error.name.contains("has no phrase"), "{}", error.name);
    }

    #[test]
    fn duplicate_unknown_and_empty_groups_are_syntax() {
        for bad in [
            r#"@cell root { id: "root"; text: { @tv "a"; @tv "b"; }; }"#,
            r#"@cell root { id: "root"; text: { @phone "a"; }; }"#,
            r#"@cell root { id: "root"; text: {}; }"#,
            r#"@cell root { id: "root"; text: @tv "a"; }"#,
        ] {
            assert_eq!(
                parse_strict_gssh5(&scene(bad)).unwrap_err().code,
                "GY-E1000",
                "{bad}"
            );
            assert!(parse_with_source_map(&scene(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn iden_and_placeholder_are_phrasing_sites() {
        let iden = scene(
            r#"@cell root { id: "root"; @iden { label: @universal "Card 1, face down"; hint: { @universal "Turn it over"; @tv "Press OK to turn it over"; }; } }"#,
        );
        let parsed = parse_strict_gssh5(&iden).unwrap();
        let iden_value = parsed.scene.root.iden.as_ref().unwrap();
        assert_eq!(iden_value.label, "");
        assert!(iden_value.hint.is_none());
        let hint = iden_value.hint_phrasing.as_ref().unwrap();
        assert_eq!(
            hint.phrase(PhrasingClass::Tv),
            Some("Press OK to turn it over")
        );
        assert_eq!(hint.phrase(PhrasingClass::Web), Some("Turn it over"));
        assert_eq!(parse_strict_gssh4(&iden).unwrap_err().code, "GY-E1019");

        let label_only = scene(r#"@cell root { id: "root"; @iden { label: @universal "Card"; } }"#);
        let admitted = parse_strict_gssh5(&label_only).unwrap();
        assert!(admitted.scene.root.iden.as_ref().unwrap().hint.is_none());
        assert!(admitted
            .scene
            .root
            .iden
            .as_ref()
            .unwrap()
            .hint_phrasing
            .is_none());

        let bare_hint = scene(
            r#"@cell root { id: "root"; @iden { label: @universal "Card"; hint: "Press OK"; } }"#,
        );
        assert_eq!(parse_strict_gssh5(&bare_hint).unwrap_err().code, "GY-E1019");

        let legacy = scene(r#"@cell root { id: "root"; @iden { label: "Card"; } }"#);
        assert!(parse_strict_gssh4(&legacy).is_ok());
        assert_eq!(parse_strict_gssh5(&legacy).unwrap_err().code, "GY-E1019");

        let field = scene(
            r#"@input field { id: "field"; placeholder: @universal "0"; @iden { label: @universal "Amount"; } }"#,
        );
        let input = parse_strict_gssh5(&field).unwrap();
        assert!(input.scene.root.placeholder.is_none());
        assert!(input.scene.root.placeholder_phrasing.is_some());
        assert_eq!(parse_strict_gssh4(&field).unwrap_err().code, "GY-E1001");

        let bare_placeholder =
            scene(r#"@input field { id: "field"; placeholder: "0"; @iden { label: "Amount"; } }"#);
        assert_eq!(
            parse_strict_gssh5(&bare_placeholder).unwrap_err().code,
            "GY-E1019"
        );
        assert_eq!(
            parse_strict_gssh4(&bare_placeholder).unwrap_err().code,
            "GY-E1001"
        );
    }

    #[test]
    fn blank_and_control_phrases_are_refused() {
        let blank = scene(r#"@cell root { id: "root"; text: @universal "  "; }"#);
        let error = parse_strict_gssh5(&blank).unwrap_err();
        assert_eq!(error.code, "GY-E1019");
        assert!(error.name.contains("blank"), "{}", error.name);

        let control =
            "@ontology demo { @cell root { id: \"root\"; text: @universal \"line\ntwo\"; } }";
        let error = parse_strict_gssh5(control).unwrap_err();
        assert_eq!(error.code, "GY-E1019");
        assert!(error.name.contains("control"), "{}", error.name);

        let big = "a".repeat(MAX_PHRASE_TEXT_BYTES + 1);
        let source = format!(
            "@ontology demo {{ @cell root {{ id: \"root\"; text: @universal \"{big}\"; }} }}"
        );
        let error = parse_strict_gssh5(&source).unwrap_err();
        assert_eq!(error.code, "GY-E1019");
        assert!(error.name.contains("above"), "{}", error.name);
    }

    #[test]
    fn live_bindings_stay_on_gssh5_live_and_phrasing_text_is_a_root_property() {
        let live = "@ontology demo { @state { label: text = \"Press OK\" session; } @cell root { id: \"root\"; text-from: state.label; } }";
        assert_eq!(parse_strict_gssh5(live).unwrap_err().code, "GY-E1012");
        let admitted = parse_strict_gssh5_live(live).unwrap();
        assert_eq!(admitted.scene.root.text_from.as_deref(), Some("label"));
        assert_eq!(admit_v5_live(live).unwrap().contract(), "GSSH/5-live");

        let both = "@ontology demo { @state { label: text = \"x\" session; } @cell root { id: \"root\"; text: @universal \"Caption\"; text-from: state.label; } }";
        assert!(parse_strict_gssh5_live(both).is_ok());
        assert_eq!(parse_strict_gssh5(both).unwrap_err().code, "GY-E1012");

        let rooted = "@ontology demo { id: \"title\"; text: @universal \"Title\"; @cell child { id: \"child\"; text: @universal \"Child\"; } }";
        let parsed = parse_strict_gssh5(rooted).unwrap();
        assert_eq!(parsed.scene.root.name, "demo");
        assert_eq!(parsed.scene.root.authored_id.as_deref(), Some("title"));
        assert!(parsed.scene.root.text_phrasing.is_some());
        assert_eq!(parsed.scene.root.children.len(), 1);
        assert_eq!(
            parsed.scene.root.children[0]
                .text_phrasing
                .as_ref()
                .unwrap()
                .phrase(PhrasingClass::Web),
            Some("Child")
        );

        let absolute =
            scene(r#"@cell root { id: "root"; position: (1, 2); text: @universal "Hi"; }"#);
        assert_eq!(parse_strict_gssh5(&absolute).unwrap_err().code, "GY-E1009");
        let desktop = scene(r#"@desktop {} @cell root { id: "root"; text: @universal "Hi"; }"#);
        assert!(parse_strict_gssh5(&desktop).is_ok());
    }
}
