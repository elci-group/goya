//! In-process semantic handoff from GY-LANG source to target compilers.
//!
//! Two profiles are provided; see `docs/gssh/GSSH-2.md`.
//!
//! * GSSH/1 ([`admit`]) wraps a strictly admitted GY-LANG/1 scene and its
//!   source entries. It rejects GSSH/2 declarations (`GY-E1006`), so
//!   existing consumers never receive state or bindings they ignore.
//! * GSSH/2 ([`admit_v2`]) adds declared state with initial values and
//!   lifetimes, input-to-field bindings with a fixed conversion, and stable
//!   origins for elements, properties, interactions, state keys and bindings.
//!
//! Neither profile is a serialized format. GSSH/2 is a Candidate contract
//! awaiting architecture approval; it is not evidence of target support.

use crate::{
    parse_strict, parse_strict_gssh2, parse_strict_gssh3, parse_strict_gssh3_live,
    parse_strict_gssh4, parse_strict_gssh4_live, InputBinding, PlatformException,
    InteractionType, InteractionValue, ParsedScene, SourceRange, StateDecl, StateLifetime,
    StrictParseError,
};
use std::collections::BTreeMap;
use std::ops::Deref;

/// Identifier for the GY-LANG/1 strict profile handoff.
pub const CONTRACT: &str = "GSSH/1";

/// Identifier for the handoff with declared state and input bindings.
pub const CONTRACT_V2: &str = "GSSH/2";

/// Identifier for GSSH/2 with every length proportional (no absolute
/// `position`, `extent`, `text-size` or `stroke-width`).
pub const CONTRACT_V3: &str = "GSSH/3";
/// GSSH/3 with live bindings (proposed; `text-from` only on this branch).
pub const CONTRACT_V3_LIVE: &str = "GSSH/3-live";
/// Platform-neutral ontology with explicit, target-scoped exceptions.
pub const CONTRACT_V4: &str = "GSSH/4";
/// GSSH/4 with live bindings (decision D12).
pub const CONTRACT_V4_LIVE: &str = "GSSH/4-live";

/// Largest UTF-8 text value a bound field may supply (the D1 text limit).
pub const MAX_FIELD_TEXT_BYTES: usize = 64 * 1024;

/// Read-only view of the admitted handoff contents.
///
/// The view may be constructed independently, but only [`SemanticHandoff`]
/// represents a snapshot returned by [`admit`].
#[derive(Debug, Clone)]
pub struct SemanticHandoffView {
    /// Contract identifier for versioned downstream interpretation.
    pub contract: &'static str,
    /// Parsed scene and available source-map origins from one parse.
    pub parsed: ParsedScene,
}

/// A strictly admitted, source-bound semantic scene snapshot.
///
/// Its contents are exposed through an immutable dereference so downstream
/// code cannot change the parsed scene after strict admission.
#[derive(Debug, Clone)]
pub struct SemanticHandoff {
    view: SemanticHandoffView,
}

impl Deref for SemanticHandoff {
    type Target = SemanticHandoffView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

/// Parse and admit source for transfer to a semantic consumer.
///
/// This uses strict GY-LANG admission, so compatibility-only constructs,
/// missing or duplicate authored IDs, and invalid interactions are rejected.
pub fn admit(source: &str) -> Result<SemanticHandoff, StrictParseError> {
    Ok(SemanticHandoff {
        view: SemanticHandoffView {
            contract: CONTRACT,
            parsed: parse_strict(source)?,
        },
    })
}

/// A source location named by stable authored identity rather than by
/// position. Resolving it against a different revision finds the same
/// construct if its identity survived the edit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// An element (`@cell`, `@button`, `@input`) by authored ID.
    Element(String),
    /// A property of an element, by element ID and canonical property name
    /// (`-` spelled `_`). See [`property_id`].
    Property { element: String, property: String },
    /// An interaction by authored ID.
    Interaction(String),
    /// One `@state` entry by key.
    State(String),
    /// One input binding by interaction ID and input key.
    Binding { interaction: String, input: String },
}

/// Canonical property name: `text-size` and `text_size` are one property.
pub fn canonical_property(name: &str) -> String {
    name.replace('-', "_")
}

/// Stable property identity: `<element id>#<canonical property>`.
/// Strict admission guarantees unique element IDs and at most one definition
/// of each canonical property per element, so this is unique per revision.
pub fn property_id(element: &str, property: &str) -> String {
    format!("{element}#{}", canonical_property(property))
}

/// Why a bound field's text could not become an interaction input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldConversionError {
    /// An `int` field is not the canonical decimal of a signed 64-bit value.
    NotCanonicalInt,
    /// A `text` field exceeds [`MAX_FIELD_TEXT_BYTES`].
    TextLimit,
    /// `bool` inputs cannot be bound to a text field.
    Unsupported,
}

impl std::fmt::Display for FieldConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotCanonicalInt => "field is not a canonical signed 64-bit decimal",
            Self::TextLimit => "field text exceeds the D1 text limit",
            Self::Unsupported => "bool inputs cannot be bound to a text field",
        })
    }
}

impl std::error::Error for FieldConversionError {}

/// Reference conversion from a bound field's current text to an input value.
///
/// * `int`: `0`, or an optional `-` and digits without a leading zero, within
///   `i64`. No whitespace, `+`, `_`, fraction, exponent or `-0`; no locale
///   digits or grouping. Anything else fails; nothing is clamped or trimmed.
/// * `text`: the exact text, at most [`MAX_FIELD_TEXT_BYTES`] UTF-8 bytes.
///   No normalization, trimming or case change.
/// * `bool`: never bound (admission rejects it).
///
/// Targets must produce the same value or the same failure for the same text.
/// A failure for any bound input means the activation does not run: state
/// and revision stay unchanged.
pub fn convert_field(
    value_type: InteractionType,
    text: &str,
) -> Result<InteractionValue, FieldConversionError> {
    match value_type {
        InteractionType::Int => {
            if !crate::parser::is_canonical_decimal(text) {
                return Err(FieldConversionError::NotCanonicalInt);
            }
            text.parse()
                .map(InteractionValue::Int)
                .map_err(|_| FieldConversionError::NotCanonicalInt)
        }
        InteractionType::Text if text.len() > MAX_FIELD_TEXT_BYTES => {
            Err(FieldConversionError::TextLimit)
        }
        InteractionType::Text => Ok(InteractionValue::Text(text.to_owned())),
        InteractionType::Bool => Err(FieldConversionError::Unsupported),
    }
}

/// A strictly admitted GSSH/2 or GSSH/3 snapshot: the scene, its source
/// entries, declared state and input bindings, all from one parse of one
/// source. [`SemanticHandoffV2::contract`] says which profile admitted it.
#[derive(Debug, Clone)]
pub struct SemanticHandoffV2 {
    contract: &'static str,
    parsed: ParsedScene,
    origins: BTreeMap<Origin, SourceRange>,
}

impl SemanticHandoffV2 {
    /// [`CONTRACT_V2`] from [`admit_v2`], [`CONTRACT_V3`] from [`admit_v3`].
    pub fn contract(&self) -> &'static str {
        self.contract
    }

    /// Rules that this target must resolve. An empty slice means `any`.
    pub fn platform_exceptions(&self) -> &[PlatformException] {
        &self.parsed.declarations.platform_exceptions
    }

    /// Only exceptions for this target; other targets retain `any` semantics.
    pub fn platform_exceptions_for<'a>(
        &'a self,
        platform: &'a str,
    ) -> impl Iterator<Item = &'a PlatformException> + 'a {
        self.platform_exceptions()
            .iter()
            .filter(move |rule| rule.platform == platform)
    }

    pub fn parsed(&self) -> &ParsedScene {
        &self.parsed
    }

    /// Declared state in source order. Empty when the scene has no state.
    pub fn state(&self) -> &[StateDecl] {
        self.parsed
            .declarations
            .state
            .as_ref()
            .map(|(decls, _)| decls.as_slice())
            .unwrap_or(&[])
    }

    /// Input bindings in source order. Admission guarantees every
    /// interaction input has exactly one.
    pub fn bindings(&self) -> &[InputBinding] {
        &self.parsed.declarations.bindings
    }

    /// Bindings of one interaction, by structural name.
    pub fn bindings_for<'a>(
        &'a self,
        interaction: &'a str,
    ) -> impl Iterator<Item = &'a InputBinding> + 'a {
        self.bindings()
            .iter()
            .filter(move |binding| binding.interaction == interaction)
    }

    /// The authored initial state snapshot, keyed by state key.
    pub fn initial_state(&self) -> BTreeMap<String, InteractionValue> {
        self.state()
            .iter()
            .map(|decl| (decl.key.clone(), decl.initial.clone()))
            .collect()
    }

    /// Keys a storage adapter must persist and restore.
    pub fn durable_keys(&self) -> impl Iterator<Item = &str> {
        self.state()
            .iter()
            .filter(|decl| decl.lifetime == StateLifetime::Durable)
            .map(|decl| decl.key.as_str())
    }

    /// Configured platform cache adapters (GSSH/4 `@persistence`). `None`
    /// when the declaration is omitted (default local stores) or is the
    /// `cache_targets: null` opt-out; see [`Self::persistence_disabled`].
    pub fn persistence_cache_targets(&self) -> Option<&BTreeMap<String, String>> {
        self.parsed
            .declarations
            .persistence_cache_targets
            .as_ref()?
            .as_ref()
    }

    /// True for the authored `cache_targets: null` opt-out.
    pub fn persistence_disabled(&self) -> bool {
        matches!(
            self.parsed.declarations.persistence_cache_targets,
            Some(None)
        )
    }

    /// Source range of a construct in this revision.
    pub fn origin(&self, origin: &Origin) -> Option<SourceRange> {
        self.origins.get(origin).copied()
    }

    /// Every resolvable origin, in identity order.
    pub fn origins(&self) -> impl Iterator<Item = (&Origin, SourceRange)> {
        self.origins.iter().map(|(origin, range)| (origin, *range))
    }
}

/// Parse and admit source under GSSH/2.
pub fn admit_v2(source: &str) -> Result<SemanticHandoffV2, StrictParseError> {
    Ok(with_origins(CONTRACT_V2, parse_strict_gssh2(source)?))
}

/// Parse and admit source under GSSH/3: GSSH/2 with proportional lengths only.
pub fn admit_v3(source: &str) -> Result<SemanticHandoffV2, StrictParseError> {
    Ok(with_origins(CONTRACT_V3, parse_strict_gssh3(source)?))
}

/// Parse and admit source under GSSH/3 live admission (proposed): GSSH/3
/// plus `text-from` display bindings.
pub fn admit_v3_live(source: &str) -> Result<SemanticHandoffV2, StrictParseError> {
    Ok(with_origins(
        CONTRACT_V3_LIVE,
        parse_strict_gssh3_live(source)?,
    ))
}

/// Admit GSSH/4; target compilers must explicitly resolve their exceptions.
pub fn admit_v4(source: &str) -> Result<SemanticHandoffV2, StrictParseError> {
    Ok(with_origins(CONTRACT_V4, parse_strict_gssh4(source)?))
}

/// Admit GSSH/4-live: GSSH/4 plus `text-from` display bindings (D12).
pub fn admit_v4_live(source: &str) -> Result<SemanticHandoffV2, StrictParseError> {
    Ok(with_origins(
        CONTRACT_V4_LIVE,
        parse_strict_gssh4_live(source)?,
    ))
}

fn with_origins(contract: &'static str, parsed: ParsedScene) -> SemanticHandoffV2 {
    let mut origins = BTreeMap::new();
    for entry in &parsed.entries {
        let origin = match entry.kind.as_str() {
            "cell" | "button" | "input" => entry.semantic_id.clone().map(Origin::Element),
            "property" | "gestures" | "iden" => entry.semantic_id.clone().map(|element| Origin::Property {
                element,
                property: canonical_property(&entry.name),
            }),
            "interaction" => entry.semantic_id.clone().map(Origin::Interaction),
            "state-key" => Some(Origin::State(entry.name.clone())),
            "binding" => entry
                .semantic_id
                .clone()
                .map(|interaction| Origin::Binding {
                    interaction,
                    input: entry.name.clone(),
                }),
            _ => None,
        };
        if let Some(origin) = origin {
            origins.insert(origin, entry.range);
        }
    }
    SemanticHandoffV2 {
        contract,
        parsed,
        origins,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_conversion_accepts_exactly_the_round_tripping_decimals() {
        // Same definition as Sash's interactive edit: the text must parse as
        // i64 and print back to itself.
        let cases = [
            "0",
            "1",
            "-1",
            "42",
            "-0",
            "+1",
            "01",
            "1_000",
            " 1",
            "1 ",
            "1.0",
            "1e3",
            "",
            "-",
            "--1",
            "٣",
            "9223372036854775807",
            "-9223372036854775808",
            "9223372036854775808",
            "-9223372036854775809",
            "00",
            "-01",
        ];
        for text in cases {
            let expected = text
                .parse::<i64>()
                .ok()
                .filter(|value| value.to_string() == text)
                .map(InteractionValue::Int)
                .ok_or(FieldConversionError::NotCanonicalInt);
            assert_eq!(
                convert_field(InteractionType::Int, text),
                expected,
                "{text:?}"
            );
        }
    }

    #[test]
    fn text_conversion_is_verbatim_and_bounded() {
        let text = " Ünïcode\u{301}\n ";
        assert_eq!(
            convert_field(InteractionType::Text, text),
            Ok(InteractionValue::Text(text.to_owned()))
        );
        let limit = "a".repeat(MAX_FIELD_TEXT_BYTES);
        assert!(convert_field(InteractionType::Text, &limit).is_ok());
        assert_eq!(
            convert_field(InteractionType::Text, &format!("{limit}a")),
            Err(FieldConversionError::TextLimit)
        );
        assert_eq!(
            convert_field(InteractionType::Bool, "true"),
            Err(FieldConversionError::Unsupported)
        );
    }

    #[test]
    fn gssh1_admission_rejects_gssh2_declarations_that_v2_admits() {
        let source = "@ontology demo { @state { n: int = 0 session; } @cell root { id: \"r\"; @button go { id: \"go\"; } } @interaction go { id: \"action.go\"; trigger: button.go.activate; class: D1; state { write n: int; } input { } set n = 1; } }";
        assert_eq!(admit(source).unwrap_err().code, "GY-E1006");
        let v2 = admit_v2(source).unwrap();
        assert_eq!(v2.initial_state().len(), 1);
        assert_eq!(v2.durable_keys().count(), 0);
    }

    #[test]
    fn admission_exposes_consistent_read_only_snapshot() {
        let handoff =
            admit("@ontology demo { @cell root { id: \"root\"; text: \"Hi\" } }").unwrap();
        assert_eq!(handoff.contract, CONTRACT);
        assert!(handoff
            .parsed
            .entries
            .iter()
            .all(|entry| entry.source_digest == handoff.parsed.source_digest));
    }
}
