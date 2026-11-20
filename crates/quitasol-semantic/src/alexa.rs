//! Q5 Alexa interaction-model emission (proposed, not ratified).
//!
//! Emits an Alexa interaction-model fragment from the Q4 manifest:
//! - Goya typed slots → Alexa slot types (AMAZON.SearchQuery, AMAZON.NUMBER, etc.)
//! - Goya semantic categories → Alexa intent groupings and sample utterances
//! - Certification boundaries recorded as metadata (skill policy, COPPA, invocation rules)
//!
//! This is a **candidate artifact** — not an admitted Alexa skill. Certification
//! boundaries (skill policy, COPPA if child-directed, invocation rules) are
//! recorded as metadata in the emitted JSON; nothing here claims certification.

use crate::compile;
use crate::fireos::emit_fire_os_plan;
use crate::resolve::resolve_manifest;
use goya_lang::intents::IntentDiagnostic;
use goya_lang::{parse, Scene, TransportClass};

/// Contract identifier for the emitted Alexa interaction model fragment.
/// Draft revision: proposed, not ratified.
pub const ALEXA_IM_CONTRACT: &str = "QUITASOL-ALEXA-IM/1-draft";

/// Maps a Goya scalar type to its closest Alexa built-in slot type.
fn goya_type_to_alexa_slot(ty: goya_lang::InteractionType) -> &'static str {
    match ty {
        goya_lang::InteractionType::Text => "AMAZON.SearchQuery",
        goya_lang::InteractionType::Int => "AMAZON.NUMBER",
        goya_lang::InteractionType::Bool => "AMAZON.YES_NO",
    }
}

/// Maps a Goya transport class to the corresponding Alexa built-in intent
/// (for playback transport controls).
fn transport_to_builtin_intent(transport: TransportClass) -> Option<&'static str> {
    match transport {
        TransportClass::PlayPause => Some("AMAZON.PauseIntent"),
        TransportClass::SkipNext => Some("AMAZON.NextIntent"),
        TransportClass::SkipPrevious => Some("AMAZON.PreviousIntent"),
        TransportClass::SeekForward => Some("AMAZON.FastForwardIntent"),
        TransportClass::SeekBackward => Some("AMAZON.RewindIntent"),
        TransportClass::Restart => Some("AMAZON.StartOverIntent"),
    }
}

/// Maps a Q4 semantic category to an Alexa intent name suffix and sample
/// utterance prefix. The full intent name will be `Category_IntentName`.
fn category_to_alexa_prefix(category: &str) -> (&'static str, &'static str) {
    match category {
        "search" => ("Search", "find"),
        "playback" => ("Playback", "play"),
        "navigation" => ("Navigate", "go to"),
        "selection" => ("Select", "choose"),
        "settings" => ("Settings", "set"),
        "confirm" => ("Confirm", "confirm"),
        _ => ("General", "do"),
    }
}

/// One Alexa intent definition emitted from a Goya intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlexaIntent {
    /// Alexa intent name: `Category_IntentName` (e.g., "Search_FindMovies").
    pub name: String,
    /// Sample utterances with slots replaced by `{slot}` (Alexa format).
    pub samples: Vec<String>,
    /// Slot definitions for this intent.
    pub slots: Vec<AlexaSlot>,
    /// Whether this maps to an Alexa built-in intent.
    pub builtin: Option<&'static str>,
}

/// One slot definition for an Alexa intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlexaSlot {
    pub name: String,
    pub slot_type: String, // e.g., "AMAZON.SearchQuery"
}

/// Certification metadata recorded in the emitted artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationInfo {
    /// Whether the skill declares child-directed content (COPPA).
    pub child_directed: bool,
    /// Skill policy compliance note.
    pub policy_note: String,
    /// Invocation name (required for certification).
    pub invocation_name: Option<String>,
    /// Invocation rules compliance note.
    pub invocation_note: String,
}

/// The deterministic Alexa interaction model fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlexaInteractionModel {
    pub contract: &'static str,
    pub profile: &'static str,
    pub intent_manifest_contract: &'static str,
    pub invocation_name: Option<String>,
    pub intents: Vec<AlexaIntent>,
    pub builtins_used: Vec<String>,
    pub certification: CertificationInfo,
}
}

impl AlexaInteractionModel {
    pub fn contract(&self) -> &'static str {
        self.contract
    }

    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"contract\":");
        crate::push_json_string(&mut out, self.contract);
        out.push_str(",\"profile\":");
        crate::push_json_string(&mut out, self.profile);
        out.push_str(",\"intent_manifest_contract\":");
        crate::push_json_string(&mut out, self.intent_manifest_contract);
        out.push_str(",\"invocation_name\":");
        match &self.invocation_name {
            Some(name) => crate::push_json_string(&mut out, name),
            None => out.push_str("null"),
        }
        out.push_str(",\"intents\":[");
        for (i, intent) in self.intents.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            crate::push_json_string(&mut out, &intent.name);
            out.push_str(",\"samples\":[");
            for (j, sample) in intent.samples.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                crate::push_json_string(&mut out, sample);
            }
            out.push_str("],\"slots\":[");
            for (j, slot) in intent.slots.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str("{\"name\":");
                crate::push_json_string(&mut out, &slot.name);
                out.push_str(",\"type\":");
                crate::push_json_string(&mut out, &slot.slot_type);
                out.push('}');
            }
            out.push(']');
            out.push_str(",\"builtin\":");
            match intent.builtin {
                Some(b) => crate::push_json_string(&mut out, b),
                None => out.push_str("null"),
            }
            out.push('}');
        }
        out.push_str("],\"builtins_used\":[");
        for (i, b) in self.builtins_used.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            crate::push_json_string(&mut out, b);
        }
        out.push_str("],\"certification\":{\"child_directed\":");
        out.push_str(if self.certification.child_directed { "true" } else { "false" });
        out.push_str(",\"policy_note\":");
        crate::push_json_string(&mut out, &self.certification.policy_note);
        out.push_str(",\"invocation_name\":");
        match &self.certification.invocation_name {
            Some(name) => crate::push_json_string(&mut out, name),
            None => out.push_str("null"),
        }
        out.push_str(",\"invocation_note\":");
        crate::push_json_string(&mut out, &self.certification.invocation_note);
        out.push_str("]}");
        out
    }
}

/// Emit the Alexa interaction model from a Q4 manifest (via FireOS plan and resolve).
///
/// Fail-closed: the intent manifest must compile and resolve successfully.
/// Returns the deterministic Alexa interaction model fragment.
pub fn emit_alexa_interaction_model(
    scene: &goya_lang::Scene,
    profile_id: &str,
    width_pt: f64,
    height_pt: f64,
) -> Result<AlexaInteractionModel, IntentDiagnostic> {
    let resolved = resolve_manifest(scene, profile_id, width_pt, height_pt)?;
    let plan = emit_fire_os_plan(scene, &resolved)?;
    let manifest = compile(scene)?;

    let mut intents = Vec::new();
    let mut builtins_used = std::collections::BTreeSet::new();

    for intent in manifest.entries() {
        let (prefix, verb) = category_to_alexa_prefix(intent.category);
        let alexa_name = format!("{}_{}", prefix, intent.name);

        // Determine if this maps to a built-in
        let builtin = intent.transport.and_then(transport_to_builtin_intent);

        // Generate sample utterances: replace {slot} with {slot} (Alexa format)
        let mut samples = Vec::new();
        for utterance in &intent.utterances {
            // Convert Goya {slot} to Alexa {slot} format (same syntax)
            samples.push(utterance.clone());
        }

        // Build slot definitions
        let mut slots = Vec::new();
        for (slot_name, slot_type) in &intent.slots {
            let alexa_type = goya_type_to_alexa_slot(*slot_type);
            slots.push(AlexaSlot {
                name: slot_name.clone(),
                slot_type: alexa_type.to_owned(),
            });
        }

        // Check for built-in
        if let Some(builtin) = builtin {
            builtins_used.insert(builtin.to_owned());
        }

        intents.push(AlexaIntent {
            name: alexa_name,
            samples,
            slots,
            builtin,
        });
    }

    let builtins_used: Vec<String> = builtins_used.into_iter().collect();

    // Certification metadata (recorded honestly; not claimed)
    let certification = CertificationInfo {
        child_directed: false, // default; override via scene metadata in future
        policy_note: "Alexa skill certification requires: privacy policy URL, valid invocation name (2+ words, not generic), no trademark infringement, content policy compliance. Skill must pass functional testing and policy review.".to_owned(),
        invocation_name: None, // to be provided by developer
        invocation_note: "Invocation name must be 2+ words, unique, not generic (e.g., 'movie finder' not 'movie'). Must follow Alexa Skills Kit invocation name requirements.".to_owned(),
    };

    Ok(AlexaInteractionModel {
        contract: ALEXA_IM_CONTRACT,
        profile: crate::INTENT_PROFILE,
        intent_manifest_contract: crate::INTENT_IR_CONTRACT,
        invocation_name: None,
        intents,
        builtins_used,
        certification,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_lang::parse;

    const SCENE: &str = "\
@ontology app {
    @button go { }
    @input searchField { }
    @interaction searchCatalogue {
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @intent search {
        action: searchCatalogue;
        category: search;
        slots { query: text; limit: int; }
        voice {
            utterance: \"Find {query}\";
            utterance: \"Search for {query} top {limit}\";
        }
        fallback { remote: go; }
    }
}
";

    #[test]
    fn alexa_model_emits_search_intent_with_slots() {
        let scene = parse(SCENE).unwrap();
        let model = emit_alexa_interaction_model(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0)
            .expect("model emits");

        assert_eq!(model.contract(), ALEXA_IM_CONTRACT);
        assert_eq!(model.intents.len(), 1);
        let intent = &model.intents[0];
        assert_eq!(intent.name, "Search_search");
        assert_eq!(intent.samples.len(), 2);
        assert!(intent.samples.iter().any(|s| s.contains("{query}")));
        assert!(intent.samples.iter().any(|s| s.contains("{limit}")));
        assert_eq!(intent.slots.len(), 2);
        let query_slot = intent.slots.iter().find(|s| s.name == "query").unwrap();
        assert_eq!(query_slot.slot_type, "AMAZON.SearchQuery");
        let limit_slot = intent.slots.iter().find(|s| s.name == "limit").unwrap();
        assert_eq!(limit_slot.slot_type, "AMAZON.NUMBER");
        assert_eq!(intent.builtin, None);
    }

const CERT_SCENE: &str = r#"@ontology app {
    @button go { id: "button.go"; }
    @input searchField { id: "element.searchField"; }
    @cell searchPanel { id: "element.searchPanel"; }
    @interaction a {
        id: "action.a";
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @intent main {
        id: "intent.main.v1";
        action: a;
        category: general;
        slots { query: text; }
        voice {
            utterance: "Go";
            confirmation: false;
        }
        fallback {
            remote: searchPanel;
            keyboard: searchField;
        }
    }
}"#;

const TRANSPORT_SCENE: &str = r#"@ontology app {
    @button go { id: "button.go"; }
    @input playField { id: "element.playField"; }
    @cell playPanel { id: "element.playPanel"; }
    @interaction p {
        id: "action.p";
        trigger: button.go.activate;
        class: D1;
        state { }
        input { }
    }
    @intent play {
        id: "intent.play.v1";
        action: p;
        transport: play_pause;
        category: playback;
        slots { query: text; }
        voice {
            utterance: "Play";
        }
        fallback {
            remote: playPanel;
        }
    }
}"#;
        let model = emit_alexa_interaction_model(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0)
            .expect("model emits");
        let intent = &model.intents[0];
        assert_eq!(intent.builtin, Some("AMAZON.PauseIntent"));
        assert!(model.builtins_used.contains(&"AMAZON.PauseIntent".to_owned()));
    }

#[test]
    fn alexa_model_includes_certification_metadata() {
        let scene = parse(CERT_SCENE).unwrap();
        let model = emit_alexa_interaction_model(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0)
            .expect("model emits");
        let cert = &model.certification;
        assert!(!cert.child_directed);
        assert!(cert.policy_note.contains("privacy policy"));
        assert!(cert.invocation_note.contains("2+ words"));
        assert!(cert.invocation_name.is_none());
    }

    #[test]
    fn alexa_model_json_is_deterministic() {
        let scene = parse(SCENE).unwrap();
        let a = emit_alexa_interaction_model(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap();
        let b = emit_alexa_interaction_model(&scene, "oem.generic.tv/1.0.0", 800.0, 600.0).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_json(), b.to_json());
        let json = a.to_json();
        assert!(json.starts_with("{\"contract\":\"QUITASOL-ALEXA-IM/1-draft\""));
        assert!(json.contains("\"name\":\"Search_search\""));
        assert!(json.contains("\"type\":\"AMAZON.SearchQuery\""));
        assert!(json.contains("\"child_directed\":false"));
    }
}
