//! Semantic Intent Compiler for Quitasol phase 0.
//!
//! **Proposed, not ratified.** This crate compiles `@intent` declarations of
//! a parsed Goya scene into a deterministic intent manifest. It establishes
//! no runtime, performs no Alexa (or any other platform) lowering, and admits
//! no target: the manifest records its lowering tier as `"unresolved"` until a
//! ratified contract specifies one.
//!
//! Like `goya_lang::intents`, this is quarantine surface: compilation is
//! fail-closed (the first failed invariant is returned as an error), total on
//! all inputs, and deterministic — manifest entries are sorted by intent name
//! and JSON serialisation is hand-written with no external dependencies.

use goya_lang::intents::{validate_intent_decl, IntentDiagnostic, UTTERANCE_GRAMMAR_V0};
use goya_lang::IntentDecl;

/// Contract identifier implemented by this crate. Draft revision: the intent
/// IR contract is proposed, not ratified.
pub const INTENT_IR_CONTRACT: &str = "QUITASOL-INTENT-IR/1-draft";

/// Intent profile recognised by this compiler. A future profile is a new
/// identifier and a new contract revision, never a silent reinterpretation.
pub const INTENT_PROFILE: &str = "quitasol.intent.v0";

/// Lowering tier recorded on every manifest this compiler emits. The tier is
/// always `"unresolved"`: no runtime and no voice-platform lowering exists in
/// phase 0.
pub const LOWERING_TIER: &str = "unresolved";

/// One compiled intent, ready for manifest serialisation. Mirrors the
/// authored declaration fields; no lowering, target or runtime binding is
/// attached at phase 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentManifestEntry {
    pub name: String,
    pub action: String,
    pub authored_id: Option<String>,
    pub utterances: Vec<String>,
    pub confirmation: bool,
    pub fallback_remote: Option<String>,
    pub fallback_keyboard: Option<String>,
}

/// The deterministic output of compilation: a versioned intent manifest.
/// Entries are sorted by intent name so that two compilations of the same
/// scene — or of scenes differing only in declaration order — are identical.
///
/// The manifest records the pinned utterance grammar and the `"unresolved"`
/// lowering tier; it is a proposal artefact, not an admitted runtime input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentManifest {
    contract: &'static str,
    profile: &'static str,
    utterance_grammar: &'static str,
    lowering_tier: &'static str,
    entries: Vec<IntentManifestEntry>,
}

impl IntentManifest {
    pub fn contract(&self) -> &'static str {
        self.contract
    }

    pub fn profile(&self) -> &'static str {
        self.profile
    }

    pub fn utterance_grammar(&self) -> &'static str {
        self.utterance_grammar
    }

    /// Always `"unresolved"` in phase 0: this compiler performs no lowering.
    pub fn lowering_tier(&self) -> &'static str {
        self.lowering_tier
    }

    /// Compiled intents, sorted by name (deterministic).
    pub fn entries(&self) -> &[IntentManifestEntry] {
        &self.entries
    }

    /// Deterministic JSON serialisation. Field order and escaping are fixed;
    /// two manifests that are `PartialEq` produce byte-identical output. The
    /// escaper is minimal and dependency-free: `"`, `\` and control
    /// characters are escaped (`\n`, `\r`, `\t` short forms, other control
    /// characters as `\u00XX`).
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"contract\":");
        push_json_string(&mut out, self.contract);
        out.push_str(",\"profile\":");
        push_json_string(&mut out, self.profile);
        out.push_str(",\"utterance_grammar\":");
        push_json_string(&mut out, self.utterance_grammar);
        out.push_str(",\"lowering_tier\":");
        push_json_string(&mut out, self.lowering_tier);
        out.push_str(",\"intents\":[");
        for (i, entry) in self.entries.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            push_json_string(&mut out, &entry.name);
            out.push_str(",\"action\":");
            push_json_string(&mut out, &entry.action);
            out.push_str(",\"authored_id\":");
            match &entry.authored_id {
                Some(id) => push_json_string(&mut out, id),
                None => out.push_str("null"),
            }
            out.push_str(",\"utterances\":[");
            for (j, utterance) in entry.utterances.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                push_json_string(&mut out, utterance);
            }
            out.push(']');
            out.push_str(",\"confirmation\":");
            out.push_str(if entry.confirmation { "true" } else { "false" });
            out.push_str(",\"fallback\":{\"remote\":");
            match &entry.fallback_remote {
                Some(remote) => push_json_string(&mut out, remote),
                None => out.push_str("null"),
            }
            out.push_str(",\"keyboard\":");
            match &entry.fallback_keyboard {
                Some(keyboard) => push_json_string(&mut out, keyboard),
                None => out.push_str("null"),
            }
            out.push_str("}}");
        }
        out.push_str("]}");
        out
    }
}

/// Compile the scene's `@intent` declarations into an intent manifest.
///
/// Fail-closed: the first failed invariant is returned as an
/// [`IntentDiagnostic`] with code `GY-E1011` and the offending declaration's
/// range. Checks, in order per declaration (source order):
/// - no duplicate intent names (cross-declaration);
/// - at least one utterance, every utterance non-empty and valid under the
///   pinned grammar v0, and at least one non-voice fallback
///   ([`goya_lang::intents::validate_intent_decl`]);
/// - the intent's `action` names a declared `@interaction` in the scene.
///
/// A scene with zero intents compiles to an empty manifest; compilation is
/// total on all inputs and deterministic.
pub fn compile(scene: &goya_lang::Scene) -> Result<IntentManifest, IntentDiagnostic> {
    let mut seen_names = std::collections::BTreeSet::new();
    let mut entries = Vec::new();
    for intent in &scene.intents {
        if !seen_names.insert(intent.name.clone()) {
            return Err(diagnostic(
                format!("duplicate intent name {}", intent.name),
                intent.range,
            ));
        }
        let failures = validate_intent_decl(intent);
        if let Some(failure) = failures.into_iter().next() {
            return Err(failure);
        }
        if !scene.interactions.iter().any(|i| i.name == intent.action) {
            return Err(diagnostic(
                format!(
                    "intent {}: action {} does not reference a declared interaction",
                    intent.name, intent.action
                ),
                intent.range,
            ));
        }
        entries.push(entry_from_decl(intent));
    }
    // Deterministic output regardless of declaration order.
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(IntentManifest {
        contract: INTENT_IR_CONTRACT,
        profile: INTENT_PROFILE,
        utterance_grammar: UTTERANCE_GRAMMAR_V0,
        lowering_tier: LOWERING_TIER,
        entries,
    })
}

fn entry_from_decl(intent: &IntentDecl) -> IntentManifestEntry {
    IntentManifestEntry {
        name: intent.name.clone(),
        action: intent.action.clone(),
        authored_id: intent.authored_id.clone(),
        utterances: intent.utterances.clone(),
        confirmation: intent.confirmation,
        fallback_remote: intent.fallback_remote.clone(),
        fallback_keyboard: intent.fallback_keyboard.clone(),
    }
}

fn diagnostic(name: String, range: goya_lang::SourceRange) -> IntentDiagnostic {
    IntentDiagnostic {
        code: "GY-E1011",
        name,
        range,
    }
}

/// Minimal JSON string escaper: quotes, backslashes, control characters.
fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_lang::parse;

    /// Two intents, declared in non-alphabetical order; reuses the shape of
    /// the accepted fixture in crates/goya-lang/tests/intent_corpus.rs.
    const ACCEPT: &str = "\
@ontology app {
    @button go { id: \"button.go\"; }
    @input searchField { id: \"element.searchField\"; }
    @cell searchPanel { id: \"element.searchPanel\"; }
    @interaction searchCatalogue {
        id: \"action.search.v1\";
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @interaction playPause {
        id: \"action.playPause.v1\";
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @intent search {
        id: \"intent.search.v1\";
        action: searchCatalogue;
        voice {
            utterance: \"Find {query}\";
            utterance: \"Search for {query}\";
            utterance: \"Show me {query}\";
            confirmation: false;
        }
        fallback {
            remote: searchPanel;
            keyboard: searchField;
        }
    }
    @intent playback {
        action: playPause;
        voice {
            utterance: \"Pause playback\";
            confirmation: true;
        }
        fallback {
            remote: searchPanel;
        }
    }
}
";

    #[test]
    fn accept_scene_compiles_to_sorted_manifest() {
        let scene = parse(ACCEPT).unwrap();
        let manifest = compile(&scene).unwrap();
        assert_eq!(manifest.contract(), INTENT_IR_CONTRACT);
        assert_eq!(manifest.profile(), INTENT_PROFILE);
        assert_eq!(manifest.utterance_grammar(), UTTERANCE_GRAMMAR_V0);
        assert_eq!(manifest.lowering_tier(), "unresolved");
        let names: Vec<&str> = manifest.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["playback", "search"], "sorted by name");
        let playback = &manifest.entries()[0];
        assert!(playback.confirmation);
        assert_eq!(playback.fallback_remote.as_deref(), Some("searchPanel"));
        assert_eq!(playback.fallback_keyboard, None);
        let search = &manifest.entries()[1];
        assert_eq!(search.authored_id.as_deref(), Some("intent.search.v1"));
        assert_eq!(search.utterances.len(), 3);
    }

    /// Same scene with the two intents swapped in declaration order.
    const REORDERED: &str = "\
@ontology app {
    @button go { id: \"button.go\"; }
    @input searchField { id: \"element.searchField\"; }
    @cell searchPanel { id: \"element.searchPanel\"; }
    @interaction searchCatalogue {
        id: \"action.search.v1\";
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @interaction playPause {
        id: \"action.playPause.v1\";
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @intent playback {
        action: playPause;
        voice {
            utterance: \"Pause playback\";
            confirmation: true;
        }
        fallback {
            remote: searchPanel;
        }
    }
    @intent search {
        id: \"intent.search.v1\";
        action: searchCatalogue;
        voice {
            utterance: \"Find {query}\";
            utterance: \"Search for {query}\";
            utterance: \"Show me {query}\";
            confirmation: false;
        }
        fallback {
            remote: searchPanel;
            keyboard: searchField;
        }
    }
}
";

    #[test]
    fn compilation_and_json_are_deterministic_across_declaration_order() {
        let first = compile(&parse(ACCEPT).unwrap()).unwrap();
        let second = compile(&parse(REORDERED).unwrap()).unwrap();
        assert_eq!(first, second, "entries sorted by name regardless of order");
        assert_eq!(first.to_json(), second.to_json(), "byte-identical JSON");

        // Two compiles of the same scene are byte-identical.
        assert_eq!(
            first.to_json(),
            compile(&parse(ACCEPT).unwrap()).unwrap().to_json()
        );
    }

    #[test]
    fn unknown_action_is_rejected_fail_closed() {
        let mutant = ACCEPT.replacen("action: playPause;", "action: nowhere;", 1);
        let scene = parse(&mutant).unwrap();
        let error = compile(&scene).unwrap_err();
        assert_eq!(error.code, "GY-E1011");
        assert!(
            error
                .name
                .contains("does not reference a declared interaction"),
            "{error}"
        );
        let span = &mutant[error.range.start..error.range.end];
        assert!(span.starts_with("@intent playback"), "{span}");
    }

    #[test]
    fn duplicate_intent_names_are_rejected_fail_closed() {
        let duplicated = ACCEPT.replacen("@intent playback", "@intent search", 1);
        let scene = parse(&duplicated).unwrap();
        let error = compile(&scene).unwrap_err();
        assert_eq!(error.code, "GY-E1011");
        assert!(error.name.contains("duplicate intent name"), "{error}");
        assert!(duplicated[error.range.start..error.range.end].starts_with("@intent search"));
    }

    #[test]
    fn missing_fallback_is_rejected_fail_closed() {
        let mutant = ACCEPT
            .replacen(
                "remote: searchPanel;\n            keyboard: searchField;",
                "",
                1,
            )
            .replacen("remote: searchPanel;", "", 1);
        let scene = parse(&mutant).unwrap();
        let error = compile(&scene).unwrap_err();
        assert_eq!(error.code, "GY-E1011");
        assert!(error.name.contains("no non-voice fallback"), "{error}");
    }

    #[test]
    fn bad_slot_grammar_is_rejected_fail_closed() {
        let mutant = ACCEPT.replacen(
            "utterance: \"Find {query}\";",
            "utterance: \"Find {not a slot}\";",
            1,
        );
        let scene = parse(&mutant).unwrap();
        let error = compile(&scene).unwrap_err();
        assert_eq!(error.code, "GY-E1011");
        assert!(
            error.name.contains("slot name is not an identifier"),
            "{error}"
        );
    }

    #[test]
    fn zero_intents_compile_to_an_empty_manifest() {
        let scene = parse(
            "@ontology app { @button go { } @interaction a { trigger: button.go.activate; class: D1; state { } input { } } }",
        )
        .unwrap();
        let manifest = compile(&scene).unwrap();
        assert!(manifest.entries().is_empty());
        assert_eq!(
            manifest.to_json(),
            "{\"contract\":\"QUITASOL-INTENT-IR/1-draft\",\"profile\":\"quitasol.intent.v0\",\"utterance_grammar\":\"quitasol.utterance.v0\",\"lowering_tier\":\"unresolved\",\"intents\":[]}"
        );
    }

    #[test]
    fn json_contains_expected_fields_in_order() {
        let scene = parse(ACCEPT).unwrap();
        let json = compile(&scene).unwrap().to_json();
        let prefix = "{\"contract\":\"QUITASOL-INTENT-IR/1-draft\",\"profile\":\"quitasol.intent.v0\",\"utterance_grammar\":\"quitasol.utterance.v0\",\"lowering_tier\":\"unresolved\",\"intents\":[";
        assert!(json.starts_with(prefix), "{json}");
        assert!(json.contains("\"name\":\"playback\""), "{json}");
        assert!(json.contains("\"action\":\"searchCatalogue\""), "{json}");
        assert!(
            json.contains("\"authored_id\":\"intent.search.v1\""),
            "{json}"
        );
        assert!(
            json.contains("\"utterances\":[\"Pause playback\"]"),
            "{json}"
        );
        assert!(json.contains("\"confirmation\":true"), "{json}");
        assert!(
            json.contains(
                "\"fallback\":{\"remote\":\"searchPanel\",\"keyboard\":\"searchField\"}}"
            ),
            "{json}"
        );
        assert!(
            json.contains("\"fallback\":{\"remote\":\"searchPanel\",\"keyboard\":null}}"),
            "{json}"
        );
    }

    #[test]
    fn json_escapes_quotes_and_control_characters() {
        // The surface lexer has no escape sequences, so a quote inside an
        // utterance cannot be authored in source; inject it on the parsed
        // scene to exercise end-to-end escaping.
        let mut scene = parse(ACCEPT).unwrap();
        let playback = scene
            .intents
            .iter_mut()
            .find(|intent| intent.name == "playback")
            .unwrap();
        playback
            .utterances
            .insert(0, "Pause \"it\" playback".to_owned());
        let json = compile(&scene).unwrap().to_json();
        assert!(
            json.contains("\"utterances\":[\"Pause \\\"it\\\" playback\""),
            "{json}"
        );

        // Control characters are escaped as \u00XX.
        let mut out = String::new();
        push_json_string(&mut out, "a\u{1}b\u{7f}");
        assert_eq!(out, "\"a\\u0001b\\u007f\"");
    }
}
