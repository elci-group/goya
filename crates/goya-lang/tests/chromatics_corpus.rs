//! Corpus tests for the draft `@chromatics` surface: parsing, static
//! validation (GY-E1021), strict refusal (GY-E1020), and the canonical
//! lowering into the `goya-chromatics` engine end to end.

use goya_chromatics::{Capability, Engine, Request, Trigger};
use goya_lang::{
    allocation_candidates, allocation_policy, parse_with_source_map, parse_strict,
    parse_strict_gssh5, validate_chromatics, ChromaticDiagnostic, ParsedScene,
};

fn parsed(source: &str) -> ParsedScene {
    parse_with_source_map(source).expect("scene parses")
}

/// The directive's §11 example shape: an entertainment favourites context
/// with four declared candidates, one per colour key.
const SCENE: &str = r#"@ontology launcher {
    @cell root {
        @button severance { text: "Severance"; }
        @button wire { text: "The Wire"; }
        @button horses { text: "Slow Horses"; }
        @button fallout { text: "Fallout"; }
    }
    @interaction resume_severance {
        trigger: button.severance.activate;
        class: D1;
        state { read cursor: int; write cursor: int; }
        input { step: int; }
        set cursor = state.cursor + input.step;
    }
    @interaction resume_wire {
        trigger: button.wire.activate;
        class: D1;
        state { read cursor: int; write cursor: int; }
        input { step: int; }
        set cursor = state.cursor + input.step;
    }
    @interaction resume_horses {
        trigger: button.horses.activate;
        class: D1;
        state { read cursor: int; write cursor: int; }
        input { step: int; }
        set cursor = state.cursor + input.step;
    }
    @interaction resume_fallout {
        trigger: button.fallout.activate;
        class: D1;
        state { read cursor: int; write cursor: int; }
        input { step: int; }
        set cursor = state.cursor + input.step;
    }
    @chromatics favourites {
        strategy: preferential;
        fallback: ranked;
        candidate severance {
            intent: resume_severance;
            label: "Resume Severance";
            entity: "show.severance";
            favourite: 0;
            rank: 2;
            cost: 8;
        }
        candidate wire {
            intent: resume_wire;
            label: "Resume The Wire";
            entity: "show.the_wire";
            favourite: 1;
            rank: 0;
            cost: 8;
        }
        candidate horses {
            intent: resume_horses;
            label: "Resume Slow Horses";
            entity: "show.slow_horses";
            favourite: 2;
            rank: 1;
            cost: 8;
        }
        candidate fallout {
            intent: resume_fallout;
            label: "Resume Fallout";
            entity: "show.fallout";
            favourite: 3;
            rank: 3;
            cost: 8;
        }
    }
}"#;

fn assert_in_bounds(source: &str, diagnostic: &ChromaticDiagnostic) {
    assert!(
        diagnostic.range.start <= diagnostic.range.end,
        "inverted range: {:?}",
        diagnostic
    );
    assert!(
        diagnostic.range.end <= source.len(),
        "range out of bounds: {:?} for {} bytes",
        diagnostic,
        source.len()
    );
}

fn engine_with(scene_text: &str) -> Engine {
    let scene = parsed(scene_text);
    assert!(
        validate_chromatics(&scene).is_empty(),
        "reference scene validates"
    );
    assert!(
        scene.scene.chromatics.is_some(),
        "chromatics kept"
    );
    let intents: Vec<String> = scene
        .scene
        .interactions
        .iter()
        .map(|i| i.name.clone())
        .collect();
    Engine::new(Capability::webos_colour_keys(), intents).expect("capability")
}

#[test]
fn a_declared_scene_lowers_into_the_engine_and_allocates() {
    let engine = &mut engine_with(SCENE);
    let scene = parsed(SCENE);
    let decl = scene.scene.chromatics.as_ref().unwrap();
    let policy = allocation_policy(decl);
    let candidates = allocation_candidates(decl);
    // Defaults survive lowering: slots are preserved and duplicates are
    // excluded without saying so.
    assert!(policy.preserve_slots);
    assert!(policy.exclude_duplicates);
    assert!(policy.exclude_native);
    assert_eq!(policy.primary.as_str(), "preferential");
    assert_eq!(policy.fallback.map(|f| f.as_str()), Some("ranked"));

    let allocation = engine
        .allocate(&Request {
            context: "entertainment.shows.favourites",
            policy: &policy,
            candidates: &candidates,
            trigger: Trigger::Context,
            now: 1,
            interacting: false,
        })
        .expect("allocation verifies");
    assert_eq!(allocation.generation.id, 1);
    let red = allocation.generation.binding(0).expect("red bound");
    assert_eq!(red.candidate, "severance");
    assert_eq!(red.intent, "resume_severance");
    assert_eq!(red.label, "Resume Severance");
    assert_eq!(red.slot_name, "red");
    let green = allocation.generation.binding(1).expect("green bound");
    assert_eq!(green.candidate, "wire");
    let yellow = allocation.generation.binding(2).expect("yellow bound");
    assert_eq!(yellow.candidate, "horses");
    let blue = allocation.generation.binding(3).expect("blue bound");
    assert_eq!(blue.candidate, "fallout");
}

#[test]
fn swapped_favourites_keep_their_keys_through_the_lowering() {
    // §10.4: the same four shows exchange ranking positions; the resolver
    // should ordinarily retain their existing physical bindings.
    let engine = &mut engine_with(SCENE);
    let scene = parsed(SCENE);
    let decl = scene.scene.chromatics.as_ref().unwrap();
    let policy = allocation_policy(decl);
    let first = allocation_candidates(decl);
    engine
        .allocate(&Request {
            context: "entertainment.shows.favourites",
            policy: &policy,
            candidates: &first,
            trigger: Trigger::Context,
            now: 1,
            interacting: false,
        })
        .expect("first allocation");

    // The same four candidates with exchanged favourite orders: wire now
    // ranks first. Lowering is per-declaration, so exchanging the authored
    // numbers is exactly what a re-parsed scene would produce.
    let mut second = allocation_candidates(decl);
    second[0].favourite = Some(1);
    second[1].favourite = Some(0);

    let reallocation = engine
        .allocate(&Request {
            context: "entertainment.shows.favourites",
            policy: &policy,
            candidates: &second,
            trigger: Trigger::Background,
            now: 2,
            interacting: false,
        })
        .expect("reallocation verifies");
    assert_eq!(
        reallocation.generation.binding(0).unwrap().candidate,
        "severance",
        "severance keeps red although wire now ranks first"
    );
    assert_eq!(
        reallocation.generation.binding(0).unwrap().placed,
        goya_chromatics::Placement::Preserved,
        "the binding is placed by preservation, not reordered"
    );
    assert_eq!(
        reallocation.generation.binding(1).unwrap().candidate,
        "wire"
    );
}

#[test]
fn profiles_allocate_independently_through_separate_contexts() {
    let engine = &mut engine_with(SCENE);
    let scene = parsed(SCENE);
    let decl = scene.scene.chromatics.as_ref().unwrap();
    let policy = allocation_policy(decl);
    let candidates = allocation_candidates(decl);
    let request = |context: &'static str| Request {
        context,
        policy: &policy,
        candidates: &candidates,
        trigger: Trigger::Context,
        now: 1,
        interacting: false,
    };
    let a = engine.allocate(&request("profile-sal.shows")).unwrap();
    let b = engine.allocate(&request("profile-alex.shows")).unwrap();
    // §19: profiles never share history. Generation ids count per engine,
    // so the second context's first generation is not 1; isolation is the
    // history key, not the id sequence.
    assert_ne!(a.generation.id, b.generation.id);
    assert!(engine.previous("profile-sal.shows").is_some());
    assert!(engine.previous("profile-alex.shows").is_some());
    engine.forget("profile-sal");
    assert!(engine.previous("profile-sal.shows").is_none());
    assert!(engine.previous("profile-alex.shows").is_some());
}

#[test]
fn strict_admission_refuses_the_draft_surface() {
    for message in [
        parse_strict(SCENE).expect_err("gssh1 refuses").to_string(),
        parse_strict_gssh5(SCENE)
            .expect_err("gssh5 refuses")
            .to_string(),
    ] {
        assert!(
            message.starts_with("GY-E1020"),
            "expected GY-E1020, got: {message}"
        );
    }
}

#[test]
fn validation_reports_every_draft_invariant() {
    let declared = |chromatics: &str| {
        format!(
            r#"@ontology launcher {{
    @cell root {{ @button go {{ text: "Go"; }} }}
    @interaction open_go {{
        trigger: button.go.activate;
        class: D1;
        state {{ read cursor: int; write cursor: int; }}
        input {{ step: int; }}
        set cursor = state.cursor + input.step;
    }}
    {chromatics}
}}"#
        )
    };
    let cases: [(&str, &str, &str); 6] = [
        (
            "duplicate slot reservations",
            r#"@chromatics bad { strategy: manual;
                candidate a { intent: open_go; label: "A"; pin: red; }
                candidate b { intent: open_go; label: "B"; pin: red; } }"#,
            "pins slot red a second time",
        ),
        (
            "duplicate entities",
            r#"@chromatics bad { strategy: manual;
                candidate a { intent: open_go; label: "A"; entity: "one"; }
                candidate b { intent: open_go; label: "B"; entity: "one"; } }"#,
            "another candidate already carries",
        ),
        (
            "undeclared interaction",
            r#"@chromatics bad { strategy: manual;
                candidate a { intent: no_such_interaction; label: "A"; } }"#,
            "does not declare",
        ),
        (
            "risky candidate without confirmation",
            r#"@chromatics bad { strategy: manual;
                candidate a { intent: open_go; label: "A"; risk: destructive; } }"#,
            "must declare confirm: true",
        ),
        (
            "probability out of range",
            r#"@chromatics bad { strategy: manual;
                candidate a { intent: open_go; label: "A"; probability: 1001; } }"#,
            "outside 0..=1000",
        ),
        (
            "zero cost",
            r#"@chromatics bad { strategy: manual;
                candidate a { intent: open_go; label: "A"; cost: 0; } }"#,
            "at least one input",
        ),
    ];
    for (label, chromatics, cause) in cases {
        let source = declared(chromatics);
        let scene = parsed(&source);
        let diagnostics = validate_chromatics(&scene);
        assert_eq!(
            diagnostics.len(),
            1,
            "{label}: expected one diagnostic, got {diagnostics:?}"
        );
        assert_eq!(diagnostics[0].code, "GY-E1021", "{label}");
        assert!(
            diagnostics[0].name.contains(cause),
            "{label}: expected cause {cause:?} in {:?}",
            diagnostics[0].name
        );
        assert_in_bounds(&source, &diagnostics[0]);
    }
}

#[test]
fn the_block_schema_is_closed() {
    let declared = |body: &str| {
        format!(
            r#"@ontology launcher {{
    @cell root {{ @button go {{ text: "Go"; }} }}
    @interaction open_go {{
        trigger: button.go.activate;
        class: D1;
        state {{ read cursor: int; write cursor: int; }}
        input {{ step: int; }}
        set cursor = state.cursor + input.step;
    }}
    {body}
}}"#
        )
    };
    let rejected: [(&str, &str); 14] = [
        ("unknown key", r#"@chromatics b { strategy: manual; colour: red; }"#),
        (
            "duplicate key",
            r#"@chromatics b { strategy: manual; strategy: ranked; }"#,
        ),
        (
            "unknown strategy",
            r#"@chromatics b { strategy: magic; }"#,
        ),
        (
            "unknown risk",
            r#"@chromatics b { strategy: manual; candidate a { intent: open_go; label: "A"; risk: casual; } }"#,
        ),
        (
            "missing strategy",
            r#"@chromatics b { candidate a { intent: open_go; label: "A"; } }"#,
        ),
        (
            "missing intent",
            r#"@chromatics b { strategy: manual; candidate a { label: "A"; } }"#,
        ),
        (
            "empty label",
            r#"@chromatics b { strategy: manual; candidate a { intent: open_go; label: ""; } }"#,
        ),
        (
            "missing label",
            r#"@chromatics b { strategy: manual; candidate a { intent: open_go; } }"#,
        ),
        (
            "duplicate candidate",
            r#"@chromatics b { strategy: manual;
                candidate a { intent: open_go; label: "A"; }
                candidate a { intent: open_go; label: "A2"; } }"#,
        ),
        (
            "fractional integer",
            r#"@chromatics b { strategy: manual; candidate a { intent: open_go; label: "A"; rank: 1.5; } }"#,
        ),
        (
            "negative unsigned field",
            r#"@chromatics b { strategy: manual; candidate a { intent: open_go; label: "A"; rank: -1; } }"#,
        ),
        (
            "second block",
            r#"@chromatics b { strategy: manual; } @chromatics c { strategy: manual; }"#,
        ),
        (
            "nested block",
            r#"@cell root2 { @chromatics b { strategy: manual; } }"#,
        ),
        (
            "empty description",
            r#"@chromatics b { strategy: manual; candidate a { intent: open_go; label: "A"; describe: ""; } }"#,
        ),
    ];
    for (label, chromatics) in rejected {
        let source = declared(chromatics);
        assert!(
            parse_with_source_map(&source).is_err(),
            "{label}: expected a parse error, got a scene"
        );
    }
}

#[test]
fn stability_and_accommodation_keys_reach_the_policy() {
    let source = r#"@ontology launcher {
    @cell root { @button go { text: "Go"; } }
    @interaction open_go {
        trigger: button.go.activate;
        class: D1;
        state { read cursor: int; write cursor: int; }
        input { step: int; }
        set cursor = state.cursor + input.step;
    }
    @chromatics careful {
        strategy: utility;
        exclude-native-equivalents: false;
        exclude-duplicates: false;
        ordered-slots: true;
        preserve-slots: false;
        stability {
            min-improvement: 250;
            min-residency: 4;
            max-deferral: 90;
        }
        candidate a {
            intent: open_go;
            label: "Open";
            cost: 12;
            probability: 250;
            contextual: 100;
            urgency: 5;
            preference: 40;
            pin: yellow;
        }
    }
}"#;
    let scene = parsed(source);
    assert!(validate_chromatics(&scene).is_empty());
    let decl = scene.scene.chromatics.as_ref().unwrap();
    let policy = allocation_policy(decl);
    // `exclude-native-equivalents: false` is the explicit accessibility
    // accommodation (§3.3); it must survive lowering unchanged.
    assert!(!policy.exclude_native);
    assert!(!policy.exclude_duplicates);
    assert!(policy.ordered_slots);
    assert!(!policy.preserve_slots);
    assert_eq!(policy.stability.min_improvement, 250);
    assert_eq!(policy.stability.min_residency, 4);
    assert_eq!(policy.stability.max_deferral, 90);
    let candidates = allocation_candidates(decl);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].pinned.as_deref(), Some("yellow"));
    assert_eq!(candidates[0].cost, 12);
    assert_eq!(candidates[0].probability, 250);
    assert_eq!(candidates[0].contextual, 100);
    assert_eq!(candidates[0].urgency, 5);
    assert_eq!(candidates[0].preference, 40);
    // The authored utility terms compute the §9 score in milli-inputs:
    // 250 × (12 − 1) + 100 + 5 + 40.
    assert_eq!(candidates[0].utility(), 250 * 11 + 145);
}

#[test]
fn accessibility_metadata_reaches_the_published_binding() {
    // §21: symbol and accessible description ride the candidate through the
    // canonical lowering into the published binding, so the presentation
    // layer need not rely on colour.
    let source = r#"@ontology launcher {
    @cell root { @button go { text: "Go"; } }
    @interaction open_go {
        trigger: button.go.activate;
        class: D1;
        state { read cursor: int; write cursor: int; }
        input { step: int; }
        set cursor = state.cursor + input.step;
    }
    @chromatics go {
        strategy: manual;
        candidate open {
            intent: open_go;
            label: "Open";
            symbol: star;
            describe: "Opens the go view";
            pin: red;
        }
    }
}"#;
    let scene = parsed(source);
    assert!(validate_chromatics(&scene).is_empty());
    let decl = scene.scene.chromatics.as_ref().unwrap();
    let candidates = allocation_candidates(decl);
    assert_eq!(candidates[0].symbol.as_deref(), Some("star"));
    assert_eq!(candidates[0].description.as_deref(), Some("Opens the go view"));

    let policy = allocation_policy(decl);
    let mut engine = Engine::new(
        Capability::webos_colour_keys(),
        ["open_go".to_string()],
    )
    .unwrap();
    let allocation = engine
        .allocate(&Request {
            context: "go",
            policy: &policy,
            candidates: &candidates,
            trigger: Trigger::Context,
            now: 1,
            interacting: false,
        })
        .unwrap();
    let binding = allocation.generation.binding(0).unwrap();
    assert_eq!(binding.symbol.as_deref(), Some("star"));
    assert_eq!(binding.description.as_deref(), Some("Opens the go view"));
    assert_eq!(binding.label, "Open");
}
