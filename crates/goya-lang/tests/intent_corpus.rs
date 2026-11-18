//! Experimental `@intent` corpus (Quitasol phase 0; proposed, not ratified).
//!
//! The strict GSSH corpus rejects every `@intent` outright (GY-E1010), so
//! intent-level validation has its own corpus here: fixtures are admitted by
//! the compatibility parser, validated by [`goya_lang::intents`], and
//! mutation-swept for totality and deterministic diagnostics.

use goya_lang::{
    lex, parse_with_source_map, validate_intents, IntentDiagnostic, UTTERANCE_GRAMMAR_V0,
};
use std::path::Path;

/// One accepted intent scene: parse, validate, and assert span sanity.
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

fn assert_in_bounds(source: &str, diagnostic: &IntentDiagnostic) {
    let range = diagnostic.range;
    assert!(range.start <= range.end && range.end <= source.len());
    assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
}

#[test]
fn accept_scene_validates_and_preserves_declared_fields() {
    let parsed = parse_with_source_map(ACCEPT).unwrap();
    let diagnostics = validate_intents(&parsed);
    assert_eq!(diagnostics, Vec::new(), "{diagnostics:?}");

    let intents = &parsed.scene.intents;
    assert_eq!(intents.len(), 2);
    assert_eq!(intents[0].name, "search");
    assert_eq!(intents[0].action, "searchCatalogue");
    assert_eq!(intents[0].authored_id.as_deref(), Some("intent.search.v1"));
    assert_eq!(intents[0].utterances.len(), 3);
    assert!(!intents[0].confirmation);
    assert_eq!(intents[0].fallback_remote.as_deref(), Some("searchPanel"));
    assert_eq!(intents[0].fallback_keyboard.as_deref(), Some("searchField"));
    assert!(intents[1].confirmation, "explicit confirmation: true");
    assert!(intents[1].fallback_keyboard.is_none());

    // The declaration origin spans the whole construct.
    let entry = parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "intent")
        .unwrap();
    assert!(ACCEPT[entry.range.start..entry.range.end].starts_with("@intent search {"));
}

#[test]
fn rejections_carry_code_cause_and_declaration_span() {
    let valid = ACCEPT;
    // (cause, from, to) — mutations that parse and then fail validation.
    let cases: [(&str, &str, &str); 2] = [
        (
            "no fallback",
            "remote: searchPanel;",
            "",
        ),
        (
            "bad slot",
            "utterance: \"Find {query}\";",
            "utterance: \"Find {not a slot}\";",
        ),
    ];
    for (cause, from, to) in cases {
        let mut mutant = valid.replacen(from, to, 1);
        // The fixture's fallback block spans lines; also strip the keyboard
        // key so the "no fallback" case loses both non-voice targets.
        if cause == "no fallback" {
            mutant = mutant.replace("keyboard: searchField;", "");
        }
        let parsed = parse_with_source_map(&mutant)
            .unwrap_or_else(|error| panic!("{cause}: mutation failed to parse: {error:?}"));
        let diagnostics = validate_intents(&parsed);
        assert_eq!(
            diagnostics.len(),
            1,
            "{cause}: expected exactly one diagnostic, got {diagnostics:?}"
        );
        assert_eq!(diagnostics[0].code, "GY-E1011", "{cause}");
        assert_in_bounds(&mutant, &diagnostics[0]);
    }

    // Duplicate utterances are rejected at parse time, not validation.
    let duplicate_utterance =
        valid.replacen("utterance: \"Show me {query}\";", "utterance: \"Find {query}\";", 1);
    assert!(parse_with_source_map(&duplicate_utterance).is_err());

    // Missing utterances and duplicate intent names are semantic-only.
    let no_utterances = valid
        .replace("utterance: \"Find {query}\";", "")
        .replace("utterance: \"Search for {query}\";", "")
        .replace("utterance: \"Show me {query}\";", "");
    let parsed = parse_with_source_map(&no_utterances).unwrap();
    let diagnostics = validate_intents(&parsed);
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.name.contains("declares no utterance")));

    let duplicated = valid.replacen("@intent playback", "@intent search", 1);
    let parsed = parse_with_source_map(&duplicated).unwrap();
    let diagnostics = validate_intents(&parsed);
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].name.contains("duplicate intent name"));
    assert!(duplicated[diagnostics[0].range.start..diagnostics[0].range.end]
        .starts_with("@intent search"));
}

#[test]
fn grammar_identifier_is_pinned() {
    assert_eq!(UTTERANCE_GRAMMAR_V0, "quitasol.utterance.v0");
}

/// Mutation sweep: every token drop, duplication, adjacent swap and byte
/// deletion of the accepted scene must either fail to parse, fail to
/// validate, or validate cleanly — deterministically, without panicking, and
/// with in-bounds char-aligned diagnostic ranges.
#[test]
fn mutations_never_panic_and_diagnostics_are_deterministic() {
    let tokens = lex(ACCEPT).unwrap();
    let span = |i: usize| tokens[i].byte_start..tokens[i].byte_end;
    let mut mutants = Vec::new();
    for i in 0..tokens.len() {
        let r = span(i);
        mutants.push(format!("{}{}", &ACCEPT[..r.start], &ACCEPT[r.end..]));
        mutants.push(format!(
            "{}{} {}",
            &ACCEPT[..r.end],
            &ACCEPT[r.clone()],
            &ACCEPT[r.end..]
        ));
        if i + 1 < tokens.len() {
            let n = span(i + 1);
            mutants.push(format!(
                "{}{}{}{}{}",
                &ACCEPT[..r.start],
                &ACCEPT[n.clone()],
                &ACCEPT[r.end..n.start],
                &ACCEPT[r.clone()],
                &ACCEPT[n.end..]
            ));
        }
    }
    for (start, len) in ACCEPT.char_indices().map(|(i, c)| (i, c.len_utf8())) {
        mutants.push(format!("{}{}", &ACCEPT[..start], &ACCEPT[start + len..]));
    }
    let mut admitted = 0usize;
    let mut rejected = 0usize;
    for (n, mutant) in mutants.iter().enumerate() {
        let run = || -> Option<Vec<IntentDiagnostic>> {
            let parsed = parse_with_source_map(mutant).ok()?;
            Some(validate_intents(&parsed))
        };
        let first = run();
        let second = run();
        assert_eq!(first, second, "mutant {n}: nondeterministic validation");
        if let Some(diagnostics) = &first {
            admitted += 1;
            for diagnostic in diagnostics {
                assert_in_bounds(mutant, diagnostic);
            }
        } else {
            rejected += 1;
        }
    }
    // The sweep must exercise both outcomes to mean anything.
    assert!(admitted > 10 && rejected > 10, "{admitted}/{rejected}");
}

/// The strict corpus lives beside this one; make sure its reject fixture for
/// `@intent` stays wired into the GSSH corpus (all three profiles reject).
#[test]
fn strict_corpus_reject_fixture_exists() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/reject/intent-not-admitted-by-gssh-profiles.gscene");
    let source = std::fs::read_to_string(&path).unwrap();
    for admit in [
        goya_lang::parse_strict,
        goya_lang::parse_strict_gssh2,
        goya_lang::parse_strict_gssh3,
    ] {
        let error = admit(&source).unwrap_err();
        assert_eq!(error.code, "GY-E1010", "{error}");
    }
}

/// Q2 draft surface: the optional `transport:` key parses into
/// `TransportClass`, rejects unknown keywords and duplicates, and defaults
/// to `None`.
#[test]
fn transport_key_parses_or_fails_closed() {
    use goya_lang::{parse_with_source_map, TransportClass};

    let base = |transport: &str| {
        format!(
            "@ontology app {{ @button go {{ }} @interaction a {{ trigger: button.go.activate; class: D1; state {{ }} input {{ }} }} @intent main {{ action: a; transport: {transport}; voice {{ utterance: \"Pause\"; }} fallback {{ remote: go; }} }} }}"
        )
    };
    for (keyword, expected) in [
        ("play_pause", TransportClass::PlayPause),
        ("skip_next", TransportClass::SkipNext),
        ("skip_previous", TransportClass::SkipPrevious),
        ("seek_forward", TransportClass::SeekForward),
        ("seek_backward", TransportClass::SeekBackward),
        ("restart", TransportClass::Restart),
    ] {
        let scene = parse_with_source_map(&base(keyword)).unwrap();
        assert_eq!(scene.scene.intents[0].transport, Some(expected), "{keyword}");
    }

    // Unknown keyword: parse error, never a silent default.
    assert!(parse_with_source_map(&base("rewind")).is_err());
    assert!(parse_with_source_map(&base("PlayPause")).is_err());

    // Duplicate key: parse error.
    let duplicated = base("play_pause").replacen(
        "transport: play_pause;",
        "transport: play_pause; transport: restart;",
        1,
    );
    assert!(parse_with_source_map(&duplicated).is_err());

    // Absent key: None.
    let absent = "@ontology app { @button go { } @interaction a { trigger: button.go.activate; class: D1; state { } input { } } @intent main { action: a; voice { utterance: \"Go\"; } fallback { remote: go; } } }";
    let scene = parse_with_source_map(absent).unwrap();
    assert_eq!(scene.scene.intents[0].transport, None);
}
