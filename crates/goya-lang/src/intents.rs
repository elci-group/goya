//! Experimental static validation for `@intent` declarations (Quitasol
//! phase 0).
//!
//! This module implements the pinned utterance grammar v0 and the intent
//! declaration invariants proposed in docs/quitasol/intents.md. Like
//! [`crate::gestures`], it is quarantine surface: the profile it describes is
//! **proposed, not ratified**, no strict admission consumes it, and nothing
//! here admits a schema, target, lowering or release. Validation never
//! mutates the scene and is deterministic on all inputs.

use crate::parser::{ParsedScene, SourceRange};
use crate::scene::IntentDecl;

/// Pinned utterance grammar identifier. Utterances are validated against
/// grammar v0 only; a future grammar is a new identifier and a new contract
/// revision, never a silent reinterpretation of v0 text.
pub const UTTERANCE_GRAMMAR_V0: &str = "quitasol.utterance.v0";

/// One failed intent invariant. Codes sit in the GY-E10xx series:
/// `GY-E1011` covers every intent semantic failure, with `name` carrying the
/// specific cause and `range` pointing at the offending declaration span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentDiagnostic {
    pub code: &'static str,
    pub name: String,
    pub range: SourceRange,
}

impl std::fmt::Display for IntentDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: invalid intent: {}", self.code, self.name)
    }
}

impl std::error::Error for IntentDiagnostic {}

/// Validate every `@intent` in the scene. Returns all failures in source
/// order; an empty result means the declarations satisfy the phase-0
/// invariants.
///
/// Invariants enforced:
/// - at least one utterance;
/// - every utterance is non-empty and parses under grammar v0 (`{slot}`
///   placeholders name identifiers; slots are not duplicated within one
///   utterance; a bare `{` or `}` is invalid);
/// - slot sets may vary per utterance — grammar v0 models no cross-utterance
///   slot roles, so none are enforced;
/// - no duplicate utterances (also rejected at parse time);
/// - at least one non-voice fallback (`remote` or `keyboard`);
/// - no duplicate intent names.
pub fn validate_intents(parsed: &ParsedScene) -> Vec<IntentDiagnostic> {
    let mut out = Vec::new();
    let mut seen_names = std::collections::BTreeSet::new();
    for intent in &parsed.scene.intents {
        if !seen_names.insert(intent.name.clone()) {
            out.push(diagnostic(
                "GY-E1011",
                format!("duplicate intent name {}", intent.name),
                intent.range,
            ));
        }
        if intent.utterances.is_empty() {
            out.push(diagnostic(
                "GY-E1011",
                format!("intent {} declares no utterance", intent.name),
                intent.range,
            ));
        }
        for utterance in &intent.utterances {
            if let Some(cause) = utterance_error(utterance) {
                out.push(diagnostic(
                    "GY-E1011",
                    format!("intent {}: utterance {:?}: {}", intent.name, utterance, cause),
                    intent.range,
                ));
            }
        }
        if intent.fallback_remote.is_none() && intent.fallback_keyboard.is_none() {
            out.push(diagnostic(
                "GY-E1011",
                format!(
                    "intent {} declares no non-voice fallback; voice is additive",
                    intent.name
                ),
                intent.range,
            ));
        }
    }
    out
}

fn diagnostic(code: &'static str, name: String, range: SourceRange) -> IntentDiagnostic {
    IntentDiagnostic { code, name, range }
}

/// Grammar v0: any text where `{ name }` placeholders delimit slot names and
/// all other text is literal. Literal text must not contain braces; slot
/// names must be ASCII identifiers; `{}` and unclosed braces are invalid.
/// Returns `Some(cause)` when the utterance is invalid.
pub fn utterance_error(utterance: &str) -> Option<&'static str> {
    if utterance.is_empty() {
        return Some("utterance is empty");
    }
    let mut slots = Vec::new();
    let mut rest = utterance;
    loop {
        match rest.find('{') {
            None => {
                if rest.contains('}') {
                    return Some("unmatched '}'");
                }
                return None;
            }
            Some(open) => {
                if rest[..open].contains('}') {
                    return Some("unmatched '}'");
                }
                let after = &rest[open + 1..];
                let close = match after.find('}') {
                    Some(close) => close,
                    None => return Some("unclosed '{'"),
                };
                let slot = &after[..close];
                if slot.is_empty() {
                    return Some("empty slot");
                }
                if !slot
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return Some("slot name is not an identifier");
                }
                if slots.contains(&slot) {
                    return Some("duplicate slot in one utterance");
                }
                slots.push(slot);
                rest = &after[close + 1..];
            }
        }
    }
}

/// The slots of a valid utterance, in order of appearance. Empty for
/// utterances without placeholders.
pub fn utterance_slots(utterance: &str) -> Vec<&str> {
    if utterance_error(utterance).is_some() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut rest = utterance;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after.find('}').expect("utterance_error rejected unclosed");
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    out
}

/// Validate one intent declaration in isolation (no scene context): grammar,
/// utterances and fallback. Used by the semantic compiler for per-entry
/// diagnostics before cross-declaration checks.
pub fn validate_intent_decl(intent: &IntentDecl) -> Vec<IntentDiagnostic> {
    let mut out = Vec::new();
    if intent.utterances.is_empty() {
        out.push(diagnostic(
            "GY-E1011",
            format!("intent {} declares no utterance", intent.name),
            intent.range,
        ));
    }
    for utterance in &intent.utterances {
        if let Some(cause) = utterance_error(utterance) {
            out.push(diagnostic(
                "GY-E1011",
                format!("intent {}: utterance {:?}: {}", intent.name, utterance, cause),
                intent.range,
            ));
        }
    }
    if intent.fallback_remote.is_none() && intent.fallback_keyboard.is_none() {
        out.push(diagnostic(
            "GY-E1011",
            format!(
                "intent {} declares no non-voice fallback; voice is additive",
                intent.name
            ),
            intent.range,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_with_source_map;

    fn validate(source: &str) -> Vec<IntentDiagnostic> {
        validate_intents(&parse_with_source_map(source).unwrap())
    }

    #[test]
    fn grammar_v0_accepts_literals_and_slots() {
        for ok in [
            "Find {query}",
            "Scroll down",
            "Set brightness to {value} percent",
            "{a} then {b}",
            "emoji 🙂 slot {s_1}",
        ] {
            assert_eq!(utterance_error(ok), None, "{ok:?}");
        }
        for bad in [
            "",
            "unclosed {query",
            "before } after",
            "{}",
            "{not a slot}",
            "{a}{a}",
        ] {
            assert!(utterance_error(bad).is_some(), "{bad:?}");
        }
    }

    #[test]
    fn slots_are_extracted_in_order() {
        assert_eq!(utterance_slots("Find {query}"), vec!["query"]);
        assert_eq!(utterance_slots("{a} mid {b}"), vec!["a", "b"]);
        assert!(utterance_slots("no slots").is_empty());
        assert!(utterance_slots("bad {").is_empty());
    }

    #[test]
    fn valid_intents_pass_and_fallback_is_required() {
        let ok = "@ontology app { @button go { } @interaction searchCatalogue { trigger: button.go.activate; class: D1; state { } input { } } @intent search { action: searchCatalogue; voice { utterance: \"Find {query}\"; confirmation: false; } fallback { remote: searchPanel; keyboard: searchField; } } }";
        assert_eq!(validate(ok), Vec::new());

        let no_fallback =
            ok.replace("fallback { remote: searchPanel; keyboard: searchField; }", "fallback { }");
        let diagnostics = validate(&no_fallback);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "GY-E1011");
        assert!(diagnostics[0].name.contains("no non-voice fallback"));
    }

    #[test]
    fn duplicate_names_and_empty_voice_are_rejected_with_declaration_spans() {
        let source = "@ontology app { @button go { } @interaction a { trigger: button.go.activate; class: D1; state { } input { } } @intent dup { action: a; voice { } fallback { remote: r; } } @intent dup { action: a; voice { utterance: \"x\"; } fallback { remote: r; } } }";
        let parsed = parse_with_source_map(source).unwrap();
        let diagnostics = validate_intents(&parsed);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].name.contains("declares no utterance"));
        assert!(diagnostics[1].name.contains("duplicate intent name"));
        // Ranges point at each offending declaration.
        for (i, diagnostic) in diagnostics.iter().enumerate() {
            let span = &source[diagnostic.range.start..diagnostic.range.end];
            assert!(
                span.starts_with("@intent dup"),
                "diagnostic {i} span: {span}"
            );
        }
    }
}
