//! Recursive-descent parser: tokens → Scene. Total on all inputs; every
//! failure is a `ParseError` with a line and an exact UTF-8 byte range.

use crate::lexer::{lex, Token, TokenKind};
use crate::scene::{
    Anchor, CellKind, EffectRequestDecl, EffectValueType, GestureDecision, GesturePolicy,
    IntentCategory, IntentDecl, InteractionClass, InteractionDecl, InteractionExpr,
    InteractionType, InteractionValue, Scene, SceneCell, StateAssignment, TransportClass, TypedKey,
    TypedSlot, Vec3,
};
use goya_determinism::EffectKind;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    /// End-exclusive UTF-8 byte range from the start of the construct being
    /// parsed through the offending token. At end of input the range ends at
    /// the source length.
    pub range: SourceRange,
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
            "GY-E1012" => "live binding requires GSSH/3 live admission",
            "GY-E1013" => "invalid live binding",
            _ => "strict parsing failed",
        };
        write!(f, "{}: {} '{}'", self.code, description, self.name)
    }
}

impl std::error::Error for StrictParseError {}

/// Parse a scene document.
pub fn parse(source: &str) -> Result<Scene, ParseError> {
    parse_with_source_map(source).map(|parsed| parsed.scene)
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
        entries: Vec::new(),
        source_len: source.len(),
        anchor: 0,
        depth: 0,
        state: None,
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
        } else if entry.kind == "property" || entry.kind == "gestures" {
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
        },
        entries: p.entries,
        source_digest,
        declarations: Declarations {
            state: p.state,
            bindings: p.bindings,
            state_uses: p.state_uses,
            input_keys: p.input_keys,
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
    /// render state. This branch carries only `text-from` from the
    /// Arachnid proposal (`image-from`, `present-when`, `host` state and
    /// `@link` are not here yet and stay unknown).
    Gssh3Live,
}

/// Properties only GSSH/3 live admission accepts.
const LIVE_PROPERTIES: &[&str] = &["text_from"];

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
    validate_live(&parsed)?;
    validate_declarations_with(&parsed, true)?;
    reject_absolute_lengths(&parsed)?;
    Ok(parsed)
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
fn validate_live(parsed: &ParsedScene) -> Result<(), StrictParseError> {
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
    let mut check = |cell: &SceneCell, path: &[usize]| -> Result<(), StrictParseError> {
        if let Some(key) = &cell.text_from {
            if !decls.iter().any(|d| &d.key == key) {
                return Err(bad(
                    format!("text-from names undeclared state '{key}'"),
                    property_range(path, "text_from"),
                ));
            }
        }
        Ok(())
    };
    walk(&parsed.scene.root, &mut Vec::new(), &mut check)
}

/// Keys named by live bindings anywhere in the tree.
fn live_keys(cell: &SceneCell, out: &mut std::collections::BTreeSet<String>) {
    out.extend(cell.text_from.iter().cloned());
    for child in &cell.children {
        live_keys(child, out);
    }
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
            "property" if !is_known_property(&entry.name) => Some("GY-E1001"),
            "template" => Some("GY-E1002"),
            "state" | "binding" if profile == Profile::Gssh1 => Some("GY-E1006"),
            "property"
                if profile < Profile::Gssh3Live
                    && LIVE_PROPERTIES.contains(&entry.name.replace('-', "_").as_str()) =>
            {
                Some("GY-E1012")
            }
            // Intent declarations are Quitasol phase-0 experimental surface.
            // No strict GSSH consumer implements them, so none may admit one.
            "intent" => Some("GY-E1010"),
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
    Ok(parsed)
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

fn validate_declarations(parsed: &ParsedScene) -> Result<(), StrictParseError> {
    validate_declarations_with(parsed, false)
}

fn validate_declarations_with(parsed: &ParsedScene, live: bool) -> Result<(), StrictParseError> {
    let scene = &parsed.scene;
    let mut bound = std::collections::BTreeSet::new();
    if live {
        live_keys(&scene.root, &mut bound);
    }
    let declared = &parsed.declarations;
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
    if let Some(decl) = decls
        .iter()
        .find(|decl| !used.contains_key(decl.key.as_str()) && !bound.contains(&decl.key))
    {
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
            | "text-from"
            | "text_from"
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

struct P {
    tokens: Vec<Token>,
    i: usize,
    interactions: Vec<InteractionDecl>,
    intents: Vec<IntentDecl>,
    entries: Vec<SourceEntry>,
    source_len: usize,
    /// Start of the construct currently being parsed; diagnostics span from
    /// here through the offending token.
    anchor: usize,
    /// Cell-body nesting depth. The ontology body is depth 1.
    depth: usize,
    state: Option<(Vec<StateDecl>, SourceRange)>,
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
            text_size: None,
            anchor: None,
            wrap: None,
            scroll: false,
            gestures: None,
            text_from: None,
            children: Vec::new(),
            placeholder: None,
            max_length: None,
        };
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
            "text" => match self.expect(TokenKind::Str) {
                Ok(t) => cell.text = Some(t.text),
                Err(e) => return Err(e),
            },
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
            "placeholder" => cell.placeholder = Some(self.expect(TokenKind::Str)?.text),
            "text-from" | "text_from" => cell.text_from = Some(self.parse_state_ref()?),
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
