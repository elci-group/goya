//! Negative and mutation corpus for strict GY-LANG admission (GSSH/1, GSSH/2).
//!
//! `tests/corpus/reject/*.gscene` start with three header comments:
//!
//! ```text
//! // profile: gssh1 | gssh2 | both
//! // expect: GY-E10xx
//! // at: <exact source text covered by the error range, `\n` escaped>
//! ```
//!
//! `tests/corpus/accept/*.gscene` start with `// profile: ...` and must be
//! admitted by each listed profile. Every accepted fixture is then mutated
//! (token drop, duplication, adjacent swap, byte deletion); each mutant must
//! be rejected or admitted without panicking, deterministically, and with
//! in-bounds, char-aligned ranges and resolvable origins.

use goya_lang::handoff::{self, Origin};
use goya_lang::{
    lex, parse_strict, parse_strict_gssh2, parse_with_source_map, ParsedScene, SourceRange,
    StrictParseError,
};
use std::fs;
use std::path::{Path, PathBuf};

const CODES: &[&str] = &[
    "GY-E1000", "GY-E1001", "GY-E1002", "GY-E1003", "GY-E1004", "GY-E1005", "GY-E1006", "GY-E1007",
    "GY-E1008",
];

#[derive(Clone, Copy, Debug)]
enum Profile {
    Gssh1,
    Gssh2,
}

fn admit(profile: Profile, source: &str) -> Result<ParsedScene, StrictParseError> {
    match profile {
        Profile::Gssh1 => parse_strict(source),
        Profile::Gssh2 => parse_strict_gssh2(source),
    }
}

fn fixtures(kind: &str) -> Vec<(PathBuf, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(kind);
    let mut out: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "gscene"))
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "empty corpus directory {}", dir.display());
    out
}

fn header<'a>(path: &Path, source: &'a str, key: &str) -> &'a str {
    let prefix = format!("// {key}: ");
    source
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("{} has no `{prefix}` header", path.display()))
}

fn profiles(path: &Path, source: &str) -> Vec<Profile> {
    match header(path, source, "profile") {
        "gssh1" => vec![Profile::Gssh1],
        "gssh2" => vec![Profile::Gssh2],
        "both" => vec![Profile::Gssh1, Profile::Gssh2],
        other => panic!("{}: unknown profile {other}", path.display()),
    }
}

fn assert_range(source: &str, range: SourceRange, context: &str) {
    assert!(
        range.start <= range.end && range.end <= source.len(),
        "{context}: range {range:?} outside 0..{}",
        source.len()
    );
    assert!(
        source.is_char_boundary(range.start) && source.is_char_boundary(range.end),
        "{context}: range {range:?} splits a character"
    );
}

#[test]
fn reject_fixtures_fail_with_their_code_at_their_exact_span() {
    for (path, source) in fixtures("reject") {
        let code = header(&path, &source, "expect");
        let at = header(&path, &source, "at").replace("\\n", "\n");
        for profile in profiles(&path, &source) {
            let error = admit(profile, &source).expect_err(&format!(
                "{} must be rejected under {profile:?}",
                path.display()
            ));
            let context = format!("{} ({profile:?})", path.display());
            assert_eq!(error.code, code, "{context}: {error}");
            assert_range(&source, error.range, &context);
            assert_eq!(
                &source[error.range.start..error.range.end],
                at,
                "{context}: wrong span"
            );
        }
    }
}

#[test]
fn accept_fixtures_are_admitted_with_resolvable_origins() {
    for (path, source) in fixtures("accept") {
        for profile in profiles(&path, &source) {
            let parsed = admit(profile, &source)
                .unwrap_or_else(|e| panic!("{} ({profile:?}): {e}", path.display()));
            check_admitted(&source, &parsed, &format!("{}", path.display()));
            if let Profile::Gssh2 = profile {
                check_gssh2(&source, &format!("{}", path.display()));
            }
        }
    }
}

#[test]
fn gssh2_counter_exposes_state_bindings_and_stable_origins() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/accept/gssh2-counter.gscene"),
    )
    .unwrap();
    let handoff = handoff::admit_v2(&source).unwrap();
    assert_eq!(handoff.contract(), "GSSH/2");
    assert_eq!(
        handoff.initial_state().get("count"),
        Some(&goya_lang::InteractionValue::Int(0))
    );
    assert_eq!(handoff.durable_keys().collect::<Vec<_>>(), ["count"]);
    let binding = handoff.bindings_for("add").next().unwrap();
    assert_eq!(binding.field_id, "window.amount");

    let slice = |h: &handoff::SemanticHandoffV2, src: &str, origin: &Origin| {
        let range = h.origin(origin).unwrap_or_else(|| panic!("{origin:?}"));
        src[range.start..range.end].to_owned()
    };
    let text = Origin::Property {
        element: "window.amount".into(),
        property: "text".into(),
    };
    let bind = Origin::Binding {
        interaction: "action.add".into(),
        input: "amount".into(),
    };
    assert_eq!(slice(&handoff, &source, &text), "text: \"1\";");
    assert_eq!(
        slice(&handoff, &source, &bind),
        "amount: int from \"window.amount\""
    );
    assert_eq!(
        slice(&handoff, &source, &Origin::State("count".into())),
        "count: int = 0 durable;"
    );
    assert_eq!(
        handoff::property_id("window.amount", "text-size"),
        "window.amount#text_size"
    );
}

/// Insert an unrelated element before the input: every range moves and the
/// child-index paths shift, but the same origins name the same text.
#[test]
fn origins_survive_edits_that_shift_positions_and_paths() {
    let source = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/accept/gssh2-counter.gscene"),
    )
    .unwrap();
    let edited = source.replacen(
        "    @input amount {",
        "    @cell note { id: \"window.note\"; text: \"new\"; }\n    @input amount {",
        1,
    );
    let before = handoff::admit_v2(&source).unwrap();
    let after = handoff::admit_v2(&edited).unwrap();
    let mut compared = 0;
    for (origin, range) in before.origins() {
        let moved = after
            .origin(origin)
            .unwrap_or_else(|| panic!("{origin:?} lost after edit"));
        if let Origin::Element(_) = origin {
            // An element's span includes its children; only leaf constructs
            // are compared byte for byte below.
            continue;
        }
        assert_eq!(
            &source[range.start..range.end],
            &edited[moved.start..moved.end],
            "{origin:?}"
        );
        compared += 1;
    }
    assert!(compared > 10, "too few origins compared: {compared}");
    let path_before = path_of(before.parsed(), "window.amount");
    let path_after = path_of(after.parsed(), "window.amount");
    assert_ne!(path_before, path_after, "edit should shift the child path");
}

fn path_of(parsed: &ParsedScene, id: &str) -> Option<Vec<usize>> {
    parsed
        .entries
        .iter()
        .find(|entry| entry.kind == "input" && entry.semantic_id.as_deref() == Some(id))
        .and_then(|entry| entry.scene_path.clone())
}

fn check_admitted(source: &str, parsed: &ParsedScene, context: &str) {
    let mut previous = (0, 0);
    for entry in &parsed.entries {
        assert_range(source, entry.range, context);
        assert_eq!(entry.source_digest, parsed.source_digest, "{context}");
        let key = (entry.range.start, entry.range.end);
        assert!(key >= previous, "{context}: entries out of order");
        previous = key;
    }
    for binding in &parsed.declarations.bindings {
        assert_range(source, binding.range, context);
    }
    if let Some((decls, range)) = &parsed.declarations.state {
        assert_range(source, *range, context);
        for decl in decls {
            assert_range(source, decl.range, context);
        }
    }
}

fn check_gssh2(source: &str, context: &str) {
    let handoff = handoff::admit_v2(source).unwrap_or_else(|e| panic!("{context}: {e}"));
    for (origin, range) in handoff.origins() {
        assert_range(source, range, &format!("{context} {origin:?}"));
    }
    for interaction in &handoff.parsed().scene.interactions {
        for input in &interaction.inputs {
            let bound: Vec<_> = handoff
                .bindings_for(&interaction.name)
                .filter(|binding| binding.input == input.name)
                .collect();
            assert_eq!(
                bound.len(),
                1,
                "{context}: {}.{}",
                interaction.name,
                input.name
            );
            let id = interaction.authored_id.clone().unwrap();
            assert!(handoff
                .origin(&Origin::Binding {
                    interaction: id,
                    input: input.name.clone()
                })
                .is_some());
        }
        for key in interaction
            .state_reads
            .iter()
            .chain(&interaction.state_writes)
        {
            let decl = handoff
                .state()
                .iter()
                .find(|decl| decl.key == key.name)
                .unwrap_or_else(|| panic!("{context}: {} undeclared", key.name));
            assert_eq!(decl.value_type, key.value_type, "{context}");
        }
    }
    for decl in handoff.state() {
        let matches = matches!(
            (&decl.initial, decl.value_type),
            (
                goya_lang::InteractionValue::Int(_),
                goya_lang::InteractionType::Int
            ) | (
                goya_lang::InteractionValue::Text(_),
                goya_lang::InteractionType::Text
            ) | (
                goya_lang::InteractionValue::Bool(_),
                goya_lang::InteractionType::Bool
            )
        );
        assert!(matches, "{context}: initial value type for {}", decl.key);
    }
}

/// Check one mutant under one profile. Returns whether it was admitted.
fn check_mutant(profile: Profile, mutant: &str, context: &str) -> bool {
    let first = admit(profile, mutant);
    let second = admit(profile, mutant);
    match (&first, &second) {
        (Err(a), Err(b)) => {
            assert_eq!(a, b, "{context}: nondeterministic rejection");
            assert!(
                CODES.contains(&a.code),
                "{context}: unknown code {}",
                a.code
            );
            assert_range(mutant, a.range, context);
            false
        }
        (Ok(parsed), Ok(_)) => {
            check_admitted(mutant, parsed, context);
            if let Profile::Gssh2 = profile {
                check_gssh2(mutant, context);
            } else {
                assert!(
                    parsed.declarations.state.is_none() && parsed.declarations.bindings.is_empty(),
                    "{context}: GSSH/1 admitted GSSH/2 declarations"
                );
            }
            true
        }
        _ => panic!("{context}: admission is not deterministic"),
    }
}

fn token_mutants(source: &str) -> Vec<String> {
    let tokens = lex(source).expect("accepted fixtures lex");
    let span = |i: usize| tokens[i].byte_start..tokens[i].byte_end;
    let mut out = Vec::new();
    for i in 0..tokens.len() {
        let r = span(i);
        // drop
        out.push(format!("{}{}", &source[..r.start], &source[r.end..]));
        // duplicate
        out.push(format!(
            "{}{} {}",
            &source[..r.end],
            &source[r.clone()],
            &source[r.end..]
        ));
        // swap with next
        if i + 1 < tokens.len() {
            let n = span(i + 1);
            out.push(format!(
                "{}{}{}{}{}",
                &source[..r.start],
                &source[n.clone()],
                &source[r.end..n.start],
                &source[r.clone()],
                &source[n.end..]
            ));
        }
    }
    out
}

#[test]
fn token_mutations_never_panic_and_keep_range_invariants() {
    let mut admitted = 0usize;
    let mut rejected = 0usize;
    for (path, source) in fixtures("accept") {
        for (n, mutant) in token_mutants(&source).iter().enumerate() {
            for profile in [Profile::Gssh1, Profile::Gssh2] {
                let context = format!("{} mutant {n} ({profile:?})", path.display());
                if check_mutant(profile, mutant, &context) {
                    admitted += 1;
                } else {
                    rejected += 1;
                }
            }
            // The compatibility parser is total too, with exact ranges.
            if let Err(error) = parse_with_source_map(mutant) {
                assert_range(mutant, error.range, &path.display().to_string());
            }
        }
    }
    // The sweep must exercise both outcomes to mean anything.
    assert!(
        admitted > 50 && rejected > 1000,
        "{admitted} admitted, {rejected} rejected"
    );
}

#[test]
fn byte_deletions_never_panic_and_keep_range_invariants() {
    for (path, source) in fixtures("accept") {
        let boundaries: Vec<_> = source
            .char_indices()
            .map(|(i, c)| (i, c.len_utf8()))
            .collect();
        for (start, len) in boundaries {
            let mutant = format!("{}{}", &source[..start], &source[start + len..]);
            for profile in [Profile::Gssh1, Profile::Gssh2] {
                let context = format!("{} delete@{start} ({profile:?})", path.display());
                check_mutant(profile, &mutant, &context);
            }
        }
    }
}
