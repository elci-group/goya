//! GY-LANG/1 — Goya scene language, RUGID-compatible syntax.
//!
//! Revised per architecture decision: the Goya surface language adopts the
//! RUGID scene grammar (github.com/elci-group/RUGID) — `@ontology` /
//! `@cell` / `@button` / `@input` blocks with `position`/`extent` (absolute points) and
//! `position_p`/`extent_p` (proportional 0..1 of the parent). This replaces
//! the Wave-1 line-oriented module DSL for GUI authoring; `goya-ir`'s
//! programmatic API remains unchanged and interoperates via [`layout`].
//!
//! Contract: parsing and layout are total (never panic on any input) and
//! deterministic. See docs/contracts.md GY-LANG/1.

pub mod display_binding;
pub mod gestures;
pub mod graphic;
pub mod handoff;
mod hotswap;
pub mod intents;
mod lexer;
mod lowering;
mod parser;
pub mod proportional;
mod scene;
mod transition;

pub use handoff::{SemanticHandoff, CONTRACT as SEMANTIC_HANDOFF_CONTRACT};

pub use hotswap::SceneStore;
pub use intents::{
    utterance_error, utterance_slots, validate_intent_decl, validate_intents, IntentDiagnostic,
    UTTERANCE_GRAMMAR_V0,
};
pub use lexer::{lex, LexError, Token, TokenKind};
pub use lowering::{lower, LowerError, Lowered, NodeKey, NodeRecord, NodeTable, Role};
pub use parser::{
    extended_live_construct, parse, parse_strict, parse_strict_gssh2, parse_strict_gssh3,
    parse_strict_gssh3_live, parse_strict_gssh4, parse_strict_gssh4_live, parse_with_source_map,
    Declarations, InputBinding, KeyUse, LinkDecl, LinkSlot, Migration, MigrationRule, ParseError,
    ParsedScene, PlatformException, PlatformUse, SourceEntry, SourceRange, StateDecl,
    StateLifetime, StrictParseError, LINK_ROLES, MAX_ANIMATION_PERIOD_S, MAX_APPEAR_FRACTION,
    MAX_APPEAR_S, MAX_DRIFT_FRACTION, MIN_ANIMATION_PERIOD_S, MIN_APPEAR_S,
};
pub use scene::{
    hit_test, hit_test_scrolled, layout, Anchor, CellKind, Condition, ConditionOp, DrawNode, Frame,
    GestureDecision, GesturePolicy, Iden, IntentCategory, IntentDecl, RectMp, Scene, SceneCell,
    ScrollOffsets, TransportClass, TypedSlot, Vec3,
};
pub use scene::{
    EffectRequestDecl, EffectValueType, InteractionClass, InteractionDecl, InteractionExpr,
    InteractionType, InteractionValue, StateAssignment, TypedKey,
};
pub use transition::{
    compile_d1_interactions, compile_d2_interactions, CompiledD2Interaction, CompiledInteraction,
    TransitionError,
};

/// Contract identifier implemented by this crate.
pub const LANG_CONTRACT: &str = "GY-LANG/1";

#[cfg(test)]
mod tests {
    use super::*;
    use goya_ir::Node;

    #[test]
    fn source_map_preserves_unicode_ranges_and_duplicate_names() {
        let source = "@ontology demo { @cell same { text: \"é\" } @cell same { text: \"two\" } }";
        let parsed = parse_with_source_map(source).unwrap();
        let cells: Vec<_> = parsed
            .entries
            .iter()
            .filter(|entry| entry.kind == "cell")
            .collect();
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].scene_path, Some(vec![0]));
        assert_eq!(cells[1].scene_path, Some(vec![1]));
        assert_eq!(
            &source[cells[0].range.start..cells[0].range.end],
            "@cell same { text: \"é\" }"
        );
        assert_eq!(
            &source[cells[1].range.start..cells[1].range.end],
            "@cell same { text: \"two\" }"
        );
        let property = parsed
            .entries
            .iter()
            .find(|entry| entry.kind == "property")
            .unwrap();
        assert_eq!(
            &source[property.range.start..property.range.end],
            "text: \"é\""
        );
        assert!(parse_with_source_map("@ontology broken { @cell x {").is_err());
        let nested = parse_with_source_map(
            "@ontology a { @cell root { @cell x { @cell x { text: \"hi\" } } } }",
        )
        .unwrap();
        let paths: Vec<_> = nested
            .entries
            .iter()
            .filter(|e| e.kind == "cell")
            .map(|e| e.scene_path.clone().unwrap())
            .collect();
        assert_eq!(paths, vec![vec![], vec![0], vec![0, 0]]);
    }

    #[test]
    fn source_map_uses_authored_ids_for_elements_and_properties() {
        let source = "@ontology demo { @cell same { id: \"left\"; text: \"L\" } @cell same { id: \"right\"; text: \"R\" } }";
        let parsed = parse_strict(source).unwrap();
        let elements: Vec<_> = parsed
            .entries
            .iter()
            .filter(|entry| entry.kind == "cell")
            .collect();
        assert_eq!(elements[0].scene_path, Some(vec![0]));
        assert_eq!(elements[0].semantic_id.as_deref(), Some("left"));
        assert_eq!(elements[1].scene_path, Some(vec![1]));
        assert_eq!(elements[1].semantic_id.as_deref(), Some("right"));
        let label = parsed
            .entries
            .iter()
            .find(|entry| entry.kind == "property" && entry.name == "text")
            .unwrap();
        assert_eq!(label.semantic_id.as_deref(), Some("left"));
    }

    #[test]
    fn semantic_handoff_is_versioned_and_keeps_source_bound_identity() {
        let source = "@ontology demo { @button save { id: \"save-button\"; text: \"Save\" } }";
        let handoff = handoff::admit(source).unwrap();
        assert_eq!(handoff.contract, "GSSH/1");
        assert_eq!(SEMANTIC_HANDOFF_CONTRACT, handoff.contract);
        assert_eq!(
            handoff.parsed.scene.root.authored_id.as_deref(),
            Some("save-button")
        );
        assert_eq!(
            handoff
                .parsed
                .entries
                .iter()
                .find(|entry| entry.kind == "cell" || entry.kind == "button")
                .unwrap()
                .semantic_id
                .as_deref(),
            Some("save-button")
        );
        assert!(handoff
            .parsed
            .entries
            .iter()
            .all(|entry| entry.source_digest == handoff.parsed.source_digest));

        let rejected = "@ontology demo { @button save { text: \"Save\" } }";
        assert_eq!(handoff::admit(rejected).unwrap_err().code, "GY-E1005");
    }

    #[test]
    fn source_map_digest_tracks_exact_source_revision_deterministically() {
        let source = "@ontology demo { @cell root { id: \"root\"; } }";
        let first = parse_with_source_map(source).unwrap();
        let second = parse_with_source_map(source).unwrap();
        let changed = parse_with_source_map(&source.replace("root\"", "root2\"")).unwrap();
        assert_eq!(first.source_digest, second.source_digest);
        assert_ne!(first.source_digest, changed.source_digest);
        assert!(first
            .entries
            .iter()
            .all(|entry| entry.source_digest == first.source_digest));
    }

    #[test]
    fn authored_identity_survives_sibling_reordering_while_revision_changes() {
        let before = "@ontology demo { @cell first { id: \"stable.first\"; } @cell second { id: \"stable.second\"; } }";
        let after = "@ontology demo { @cell second { id: \"stable.second\"; } @cell first { id: \"stable.first\"; } }";
        let before = parse_strict(before).unwrap();
        let after = parse_strict(after).unwrap();
        let ids = |parsed: &ParsedScene| {
            parsed
                .entries
                .iter()
                .filter(|entry| entry.kind == "cell")
                .filter_map(|entry| entry.semantic_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(ids(&before), ids(&after));
        assert_ne!(before.source_digest, after.source_digest);
    }

    #[test]
    fn interaction_identity_is_authored_and_source_mapped() {
        let source = "@ontology app { @button go { id: \"element.go\"; } @interaction save { id: \"action.save.v1\"; trigger: button.go.activate; class: D1; state { write count: int; } input { } set count = 1; } }";
        let parsed = parse_strict(source).unwrap();
        assert_eq!(parsed.scene.interactions[0].name, "save");
        assert_eq!(
            parsed.scene.interactions[0].authored_id.as_deref(),
            Some("action.save.v1")
        );
        let origin = parsed
            .entries
            .iter()
            .find(|entry| entry.kind == "interaction")
            .unwrap();
        assert_eq!(origin.semantic_id.as_deref(), Some("action.save.v1"));
        assert!(source[origin.range.start..origin.range.end].starts_with("@interaction save"));
    }

    #[test]
    fn strict_parser_rejects_unknown_property_at_its_utf8_origin() {
        let source = "@ontology demo { @cell root { text: \"é\"; colur: #fff } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1001");
        assert_eq!(error.name, "colur");
        assert_eq!(&source[error.range.start..error.range.end], "colur: #fff");
        // The compatibility parser retains its documented permissive mode.
        assert!(parse(source).is_ok());
    }

    #[test]
    fn strict_frontend_does_not_admit_unimplemented_gesture_declarations() {
        // Legacy parsing may discard unknown properties; native admission must
        // not turn an authored interaction policy into an unprotected field.
        let property =
            "@ontology demo { @input notes { id: \"notes\" text: \"é\" gestures: allow } }";
        let error = parse_strict(property).unwrap_err();
        assert_eq!(error.code, "GY-E1001");
        assert_eq!(
            &property[error.range.start..error.range.end],
            "gestures: allow"
        );

        let block = "@ontology demo { @input notes { id: \"notes\" text: \"é\" gestures { copy: allow; paste: deny } } }";
        let parsed = parse_strict(block).unwrap();
        let policy = parsed
            .scene
            .root
            .gestures
            .expect("gesture block is preserved");
        assert_eq!(policy.copy, Some(GestureDecision::Allow));
        assert_eq!(policy.paste, Some(GestureDecision::Deny));
        assert_eq!(policy.cut, None);
        let entry = parsed
            .entries
            .iter()
            .find(|entry| entry.kind == "gestures")
            .unwrap();
        assert_eq!(
            &block[entry.range.start..entry.range.end],
            "gestures { copy: allow; paste: deny }"
        );
    }

    #[test]
    fn strict_parser_rejects_recognized_but_unrepresented_template() {
        let source = "@ontology demo { @cell root { @template card { text: \"x\" } } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1002");
        assert_eq!(
            &source[error.range.start..error.range.end],
            "@template card { text: \"x\" }"
        );
    }

    #[test]
    fn strict_parser_rejects_duplicate_properties_with_second_origin() {
        let source = "@ontology demo { @cell root { text: \"a\"; text: \"b\" } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1003");
        assert_eq!(&source[error.range.start..error.range.end], "text: \"b\"");
    }

    #[test]
    fn strict_parser_rejects_invalid_authored_action() {
        let source = "@ontology demo { @button go { id: \"button.go\"; text: \"Go\" } @interaction save { id: \"action.save\"; trigger: button.missing.activate; class: D1; state { write x: int; } input { } set x = 1; } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1004", "{error:?}");
        assert!(error.name.contains("TriggerNotFound"));
        assert!(source[error.range.start..error.range.end].starts_with("@interaction save"));
    }

    #[test]
    fn strict_interaction_error_points_to_the_interaction_that_introduced_it() {
        let source = "@ontology app { @button good { id: \"button.good\"; } @interaction first { id: \"action.first\"; trigger: button.good.activate; class: D1; state { write count: int; } input { } set count = 1; } @interaction second { id: \"action.second\"; trigger: button.missing.activate; class: D1; state { write count: int; } input { } set count = 1; } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1004");
        assert!(source[error.range.start..error.range.end].starts_with("@interaction second"));
    }

    #[test]
    fn strict_interaction_origin_follows_d1_before_d2_error_precedence() {
        let source = "@ontology app { @button fetch { id: \"button.fetch\"; } @button save { id: \"button.save\"; } @interaction early_d2 { id: \"action.early_d2\"; trigger: button.fetch.activate; class: D2; effects { network; } requests { lookup: effect network capability \"network\" request \"static request\" result int64; } state { write count: text; } input { } set count = effect.lookup; } @interaction later_d1 { id: \"action.later_d1\"; trigger: button.save.activate; class: D1; state { } input { } set count = 1; } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1004");
        assert!(error.name.contains("AssignmentMismatch"));
        assert!(source[error.range.start..error.range.end].starts_with("@interaction later_d1"));
    }

    #[test]
    fn strict_parser_requires_unique_explicit_element_ids() {
        let missing = "@ontology demo { @cell root { text: \"Hi\" } }";
        let missing_error = parse_strict(missing).unwrap_err();
        assert_eq!(missing_error.code, "GY-E1005");
        assert_eq!(
            &missing[missing_error.range.start..missing_error.range.end],
            "@cell root { text: \"Hi\" }"
        );
        let duplicate =
            "@ontology demo { @cell root { id: \"same\"; @cell child { id: \"same\"; } } }";
        let duplicate_error = parse_strict(duplicate).unwrap_err();
        assert_eq!(duplicate_error.code, "GY-E1005");
        assert_eq!(
            &duplicate[duplicate_error.range.start..duplicate_error.range.end],
            "@cell child { id: \"same\"; }"
        );
        let valid = "@ontology demo { @cell root { id: \"root\"; text: \"Hi\" } }";
        assert!(parse_strict(valid).is_ok());
    }

    #[test]
    fn strict_id_errors_point_to_the_offending_element_origin() {
        let missing = "@ontology demo { @cell first { id: \"first\"; } @button second { } }";
        let error = parse_strict(missing).unwrap_err();
        assert_eq!(error.code, "GY-E1005");
        assert!(missing[error.range.start..error.range.end].starts_with("@button second"));

        let duplicate =
            "@ontology demo { @cell first { id: \"same\"; } @cell second { id: \"same\"; } }";
        let error = parse_strict(duplicate).unwrap_err();
        assert_eq!(error.code, "GY-E1005");
        assert!(duplicate[error.range.start..error.range.end].starts_with("@cell second"));
    }

    #[test]
    fn strict_root_id_error_points_to_implicit_ontology_origin() {
        let source = "@ontology demo { text: \"Hello\" }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1005");
        assert_eq!(
            &source[error.range.start..error.range.end],
            "@ontology demo { text: \"Hello\" }"
        );
    }

    #[test]
    fn strict_parser_requires_id_for_nonempty_multi_child_ontology_root() {
        // The ontology wrapper is virtual only when parsing collapses it.
        // A root with visual state remains a real scene element even when it
        // has multiple children.
        let source = "@ontology demo { fill: #fff; @cell first { id: \"first\"; } @cell second { id: \"second\"; } }";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1005");
        assert_eq!(error.name, "missing ID for demo");
    }

    #[test]
    fn strict_parser_syntax_failure_spans_construct_through_offending_token() {
        let source = "@ontology demo {\n  @cell root {\n    text \"missing-colon\"\n  }\n}";
        let error = parse_strict(source).unwrap_err();
        assert_eq!(error.code, "GY-E1000");
        assert_eq!(
            &source[error.range.start..error.range.end],
            "text \"missing-colon\""
        );
    }

    #[test]
    fn parser_rejects_geometry_numbers_that_overflow_to_infinity() {
        let huge = "9".repeat(400);
        let source =
            format!("@ontology demo {{ @cell root {{ id: \"root\"; position: ({huge}, 0, 0) }} }}");
        assert!(parse(&source).is_err());
        assert!(parse_strict(&source).is_err());
        assert!(parse_strict(
            "@ontology demo { @cell root { id: \"root\"; position: (1, 0, 0) } }"
        )
        .is_ok());
    }

    const HELLO: &str = "\
@ontology hello {
    @cell root {
        position: (0, 0, 0)
        extent: (800, 600, 0)
        fill: #1a1a1a

        @cell title {
            position: (400, 60, 0)
            text: \"Hello, Goya!\"
            text-size: 32
            fill: #00ff88
            text-anchor: middle
        }

        @button go {
            position: (300, 300, 0)
            extent: (200, 60, 0)
            fill: #3267c8
            text: \"Go\"
        }
    }
}
";

    #[test]
    fn parses_rugid_hello_world() {
        let scene = parse(HELLO).unwrap();
        assert_eq!(scene.name, "hello");
        let root = &scene.root;
        assert_eq!(root.name, "root");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[1].kind, CellKind::Button);
    }

    #[test]
    fn layout_resolves_absolute_and_proportional() {
        let src = "\
@ontology nested {
    @cell outer {
        position: (100, 100, 0)
        extent: (600, 400, 0)
        @cell middle {
            position_p: (0.1, 0.1, 0)
            extent_p: (0.8, 0.8, 0)
        }
    }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let outer = frame.find("outer").unwrap();
        // Absolute: points → millipoints, parent origin at viewport (0,0).
        assert_eq!(outer.rect.x, 100_000);
        assert_eq!(outer.rect.y, 100_000);
        assert_eq!(outer.rect.width, 600_000);
        let middle = frame.find("middle").unwrap();
        // Proportional: 0.1 of 600 pt = 60 pt from outer's origin.
        assert_eq!(middle.rect.x, 100_000 + 60_000);
        assert_eq!(middle.rect.width, 480_000); // 0.8 * 600 pt
        assert_eq!(middle.rect.height, 320_000); // 0.8 * 400 pt
    }

    #[test]
    fn scrolling_parent_preserves_overflowing_child_extent_and_hierarchy() {
        let src = "\
@ontology nested {
    @cell root {
        extent: (800, 600, 0)
        @cell viewport {
            position: (100, 100, 0)
            extent: (300, 120, 0)
            scroll: true
            @cell content {
                position: (20, 200, 0)
                extent: (180, 50, 0)
            }
        }
    }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let viewport = frame.find("viewport").unwrap();
        let content = frame.find("content").unwrap();
        assert_eq!(content.parent.as_deref(), Some("viewport"));
        assert_eq!(content.rect.x, viewport.rect.x + 20_000);
        assert_eq!(content.rect.y, viewport.rect.y + 200_000);
        assert_eq!(content.rect.width, 180_000);
        assert_eq!(content.rect.height, 50_000);
        assert_eq!(frame.scroll_limits("viewport"), (0, 130_000));
    }

    #[test]
    fn scroll_offsets_clamp_translate_children_and_clip_hits() {
        let src = "@ontology nested { @cell root { extent: (800, 600, 0) @cell viewport { position: (100, 100, 0) extent: (300, 120, 0) scroll: true @button action { position: (20, 200, 0) extent: (180, 50, 0) text: \"go\" } } } }";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let mut offsets = ScrollOffsets::default();
        assert_eq!(
            frame.scroll_target_at(&offsets, 150_000, 150_000),
            Some("viewport".to_owned())
        );
        assert!(offsets.scroll_by(&frame, "viewport", 20_000, 200_000));
        assert_eq!(offsets.get("viewport"), (0, 130_000));
        let nodes = frame.scrolled_nodes(&offsets);
        let action = nodes.iter().find(|node| node.name == "action").unwrap();
        assert_eq!(action.rect.y, 170_000);
        assert_eq!(action.clip.unwrap().y, 100_000);
        assert_eq!(
            hit_test_scrolled(&frame, &offsets, 150_000, 175_000),
            Some("action".to_owned())
        );
        assert_eq!(hit_test_scrolled(&frame, &offsets, 150_000, 225_000), None);
    }

    #[test]
    fn hit_test_returns_button_name() {
        let scene = parse(HELLO).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        assert_eq!(hit_test(&frame, 400_000, 330_000), Some("go".to_string()));
        assert_eq!(hit_test(&frame, 10_000, 10_000), None);
    }

    #[test]
    fn parses_input_node_with_geometry_and_placeholder() {
        let src = "\
@ontology s {
    @input name {
        position: (40, 520, 0)
        extent: (720, 48, 0)
        fill: #202028
        stroke: #3267c8
        stroke-width: 1
        text: \"Type a message\"
        text-size: 16
        text-anchor: middle
    }
}
";
        let scene = parse(src).unwrap();
        let input = &scene.root;
        assert_eq!(input.name, "name");
        assert_eq!(input.kind, CellKind::Input);
        assert!(input.is_input);
        assert_eq!(input.text.as_deref(), Some("Type a message"));
        assert_eq!(input.fill, Some(0x202028));
        assert_eq!(input.stroke, Some(0x3267C8));
        assert_eq!(input.stroke_width, Some(1.0));
        assert_eq!(input.text_size, Some(16.0));
        assert_eq!(input.anchor, Some(Anchor::Middle));

        let frame = layout(&scene, 800.0, 600.0);
        let node = frame.find("name").unwrap();
        assert!(node.is_input);
        assert!(!node.is_button);
        assert_eq!(
            node.rect,
            RectMp {
                x: 40_000,
                y: 520_000,
                width: 720_000,
                height: 48_000
            }
        );
        assert_eq!(node.text.as_deref(), Some("Type a message"));
    }

    #[test]
    fn parses_scene_mixing_cell_button_and_input() {
        let src = "\
@ontology s {
    @cell root {
        position: (0, 0, 0)
        extent: (800, 600, 0)
        @cell label { text: \"Count\" }
        @button go { position: (0, 0, 0); extent: (80, 40, 0); text: \"Go\" }
        @input composer { position: (0, 100, 0); extent: (400, 40, 0); text: \"Compose\" }
    }
}
";
        let scene = parse(src).unwrap();
        assert_eq!(scene.root.children.len(), 3);
        assert_eq!(scene.root.children[0].kind, CellKind::Cell);
        assert!(!scene.root.children[0].is_input);
        assert_eq!(scene.root.children[1].kind, CellKind::Button);
        assert!(!scene.root.children[1].is_input);
        assert_eq!(scene.root.children[2].kind, CellKind::Input);
        assert!(scene.root.children[2].is_input);
    }

    #[test]
    fn hit_test_returns_input_name_when_clicked() {
        let src = "\
@ontology s {
    @cell root {
        position: (0, 0, 0)
        extent: (800, 600, 0)
        @button go { position: (0, 0, 0); extent: (100, 40, 0); text: \"Go\" }
        @input composer { position: (0, 100, 0); extent: (400, 40, 0); text: \"Compose\" }
    }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        // Inside the input rect.
        assert_eq!(
            hit_test(&frame, 50_000, 120_000),
            Some("composer".to_string())
        );
        // The button still wins inside its own rect.
        assert_eq!(hit_test(&frame, 50_000, 20_000), Some("go".to_string()));
        // Outside both interactive rects.
        assert_eq!(hit_test(&frame, 700_000, 500_000), None);
    }

    #[test]
    fn comments_and_inline_properties() {
        let src = "\
@ontology s {
    // comment line
    @button b { position: (0, 0, 0); extent: (80, 80, 0); text: \"C\"; fill: #f44 }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let b = frame.find("b").unwrap();
        assert_eq!(b.rect.width, 80_000);
        assert_eq!(b.fill, Some(0xFF4444));
    }

    #[test]
    fn signed_positive_geometry_remains_valid() {
        let scene = parse("@ontology s { @cell root { position: (+1, 2, 0); } }").unwrap();
        assert_eq!(scene.root.position.unwrap().x, 1.0);
    }

    #[test]
    fn parse_errors_at_eof_report_a_useful_line() {
        let empty = parse("").unwrap_err();
        assert_eq!(empty.line, 1);

        let missing_close = parse("@ontology app {\n  @cell root {").unwrap_err();
        assert_eq!(missing_close.line, 2);
    }

    #[test]
    fn wrap_parses_and_layout_wraps_text() {
        let src = "\
@ontology s {
    @cell root {
        position: (0, 0, 0)
        extent: (800, 600, 0)
        @cell note {
            position: (0, 0, 0)
            text: \"the quick brown fox jumps over\"
            wrap: 10
        }
    }
}
";
        let scene = parse(src).unwrap();
        assert_eq!(scene.root.children[0].wrap, Some(10));
        let frame = layout(&scene, 800.0, 600.0);
        let note = frame.find("note").unwrap();
        // Greedy: each line fits in 10 columns.
        assert_eq!(
            note.text.as_deref(),
            Some("the quick\nbrown fox\njumps over")
        );
    }

    #[test]
    fn wrap_hard_splits_oversized_tokens() {
        let src = "\
@ontology s {
    @cell t {
        text: \"abcdefghij klm\"
        wrap: 4
    }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let t = frame.find("t").unwrap();
        // The 10-char token alone exceeds width 4: hard-split, remainder
        // continues on its own line before "klm" joins.
        assert_eq!(t.text.as_deref(), Some("abcd\nefgh\nij\nklm"));
    }

    #[test]
    fn wrap_hard_splits_only_between_grapheme_clusters() {
        let family = "👨‍👩‍👧‍👦";
        let src = format!("@ontology s {{ @cell t {{ text: \"éx {family}y\" wrap: 1 }} }}");
        let scene = parse(&src).unwrap();
        let text = layout(&scene, 800.0, 600.0).find("t").unwrap().text.clone();
        assert_eq!(text.as_deref(), Some("é\nx\n👨‍👩‍👧‍👦\ny"));
    }

    #[test]
    fn wrap_collapses_multi_spaces_between_words() {
        let src = "\
@ontology s {
    @cell t {
        text: \"a    b  c\"
        wrap: 10
    }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let t = frame.find("t").unwrap();
        // Separator runs collapse to single spaces between words on a line.
        assert_eq!(t.text.as_deref(), Some("a b c"));
    }

    #[test]
    fn absent_wrap_leaves_text_unchanged() {
        let src = "\
@ontology s {
    @cell t {
        text: \"the quick brown fox jumps over the lazy dog\"
    }
}
";
        let scene = parse(src).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        let t = frame.find("t").unwrap();
        assert_eq!(
            t.text.as_deref(),
            Some("the quick brown fox jumps over the lazy dog")
        );
    }

    #[test]
    fn layout_with_wrap_is_deterministic() {
        let src = "\
@ontology s {
    @cell t {
        text: \"alpha beta gamma delta epsilon zeta\"
        wrap: 12
    }
}
";
        let scene = parse(src).unwrap();
        let a = layout(&scene, 800.0, 600.0);
        let b = layout(&scene, 800.0, 600.0);
        let ta = a.find("t").unwrap().text.clone();
        let tb = b.find("t").unwrap().text.clone();
        assert_eq!(ta, tb);
        assert_eq!(ta.as_deref(), Some("alpha beta\ngamma delta\nepsilon zeta"));
    }

    #[test]
    fn wrap_rejects_zero_negative_and_overflow() {
        let zero = parse("@ontology s { @cell t { wrap: 0 } }").unwrap_err();
        let negative = parse("@ontology s { @cell t { wrap: -3 } }").unwrap_err();
        let overflow = parse("@ontology s { @cell t { wrap: 99999999999999 } }").unwrap_err();
        let fractional = parse("@ontology s { @cell t { wrap: 1.5 } }").unwrap_err();
        for e in [zero, negative, overflow, fractional] {
            assert!(e.line >= 1);
        }
    }

    #[test]
    fn duplicate_ancestor_names_and_huge_offsets_stay_total() {
        let scene = parse(
            "@ontology a { @cell go { extent: (100, 100, 0) scroll: true @button go { extent: (10, 10, 0) } } }",
        )
        .unwrap();
        let frame = layout(&scene, 100.0, 100.0);
        assert_eq!(hit_test(&frame, 1_000, 1_000).as_deref(), Some("go"));
        assert_eq!(frame.scroll_limits("go"), frame.scroll_limits("go"));

        let huge = parse(
            "@ontology a { @cell root { extent: (10, 10, 0) @cell child { position: (10000000000000000, 0, 0) extent: (1, 1, 0) } } }",
        )
        .unwrap();
        let frame = layout(&huge, 10.0, 10.0);
        let _ = hit_test(&frame, 0, 0);
        let _ = frame.scroll_limits("root");
        assert!(lower(&huge, 10.0, 10.0).is_ok());
    }

    /// Deterministic mutation fuzzing: parse + layout must be total.
    #[test]
    fn fuzz_parse_never_panics() {
        let mut rng = 0x1234_5678_9ABC_DEF0u64;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let corpus = [
            HELLO.to_string(),
            "@ontology x { @cell c { position_p: (0.5, 0.5, 0) } }".to_string(),
        ];
        for i in 0..10_000u64 {
            let mut input = corpus[(next() as usize) % corpus.len()].clone();
            match (next() + i) % 3 {
                0 => {
                    let cut = (next() as usize) % (input.len() + 1);
                    input.truncate(cut);
                }
                1 => {
                    let pos = (next() as usize) % (input.len() + 1);
                    let junk: String = (0..6).map(|_| (next() % 94 + 33) as u8 as char).collect();
                    input.insert_str(pos.min(input.len()), &junk);
                }
                _ => {
                    let bytes: Vec<u8> = (0..20).map(|_| (next() % 256) as u8).collect();
                    input = String::from_utf8_lossy(&bytes).into_owned();
                }
            }
            if let Ok(scene) = parse(&input) {
                let frame = layout(&scene, 800.0, 600.0);
                let _ = hit_test(&frame, 0, 0);
                let _ = lower(&scene, 800.0, 600.0);
            }
        }
    }

    // Test cases for enhanced parser features

    #[test]
    fn enhanced_property_admission_parses_input() {
        let src = "@ontology app { @input field { id: \"field\" input: true; } }";
        let scene = parse(src).unwrap();
        // One named cell is the scene root. The ontology wrapper is not a cell.
        let input_cell = &scene.root;
        assert_eq!(input_cell.name, "field");
        assert_eq!(input_cell.kind, CellKind::Input);
        assert!(input_cell.is_input);
    }

    #[test]
    fn enhanced_property_admission_parses_placeholder() {
        let src = "@ontology app { @input field { placeholder: \"Enter text\" } }";
        let scene = parse(src).unwrap();
        let input_cell = &scene.root;
        assert_eq!(input_cell.placeholder.as_deref(), Some("Enter text"));
    }

    #[test]
    fn enhanced_property_admission_parses_max_length() {
        let src = "@ontology app { @input field { max-length: 10; } }";
        let scene = parse(src).unwrap();
        let input_cell = &scene.root;
        assert_eq!(input_cell.max_length, Some(10));
    }

    #[test]
    fn enhanced_property_admission_parses_max_length_alternative() {
        let src = "@ontology app { @input field { max_length: 20; } }";
        let scene = parse(src).unwrap();
        let input_cell = &scene.root;
        assert_eq!(input_cell.max_length, Some(20));
    }

    #[test]
    fn enhanced_property_parsing_handles_all_new_properties() {
        let src = "@ontology app { @input field { input: true; placeholder: \"Username\"; max-length: 50; } }";
        let scene = parse(src).unwrap();
        let input_cell = &scene.root;
        assert!(input_cell.is_input);
        assert_eq!(input_cell.placeholder.as_deref(), Some("Username"));
        assert_eq!(input_cell.max_length, Some(50));
    }

    #[test]
    fn multi_cell_support_with_scrolling() {
        let src = "@ontology app { @cell container { scroll: true @input field1 { position: (0, 0, 0) } @input field2 { position: (0, 50, 0) } } }";
        let scene = parse(src).unwrap();
        let container = &scene.root;
        assert_eq!(container.name, "container");
        assert!(container.scroll);
        assert_eq!(container.children.len(), 2);
        assert!(container.children[0].is_input);
        assert!(container.children[1].is_input);
    }

    #[test]
    fn unpreserved_gesture_keys_are_syntax_errors_with_exact_ranges() {
        // focus/blur/select/submit were never given meaning by any consumer.
        for key in ["focus", "blur", "select", "submit"] {
            let src =
                format!("@ontology app {{ @input field {{ gestures {{ {key}: allow; }} }} }}");
            let error = parse(&src).unwrap_err();
            assert_eq!(
                &src[error.range.start..error.range.end],
                format!("{key}: allow")
            );
        }
    }

    #[test]
    fn input_state_initialization_supports_input_binding() {
        let src = "@ontology app { @button trigger { } @interaction process { trigger: button.trigger.activate; class: D1; state { write value: int; } input { field: int; } set value = input.field; } }";
        let scene = parse(src).unwrap();
        assert_eq!(scene.root.name, "trigger");
        assert_eq!(scene.root.kind, CellKind::Button);
        assert!(scene.root.children.is_empty());
        let interaction = &scene.interactions[0];
        assert_eq!(interaction.name, "process");
        assert_eq!(interaction.inputs.len(), 1);
        assert_eq!(interaction.inputs[0].name, "field");
        assert_eq!(interaction.inputs[0].value_type, InteractionType::Int);
    }

    #[test]
    fn enhanced_properties_backward_compatibility() {
        // Ensure existing properties still work
        let src = "@ontology app { @cell root { position: (0, 0, 0); extent: (100, 100, 0); text: \"test\"; wrap: 20; scroll: true; fill: #ff0000; } }";
        let scene = parse(src).unwrap();
        let root = scene.root;
        assert!(root.position.is_some());
        assert!(root.extent.is_some());
        assert_eq!(root.text.as_deref(), Some("test"));
        assert_eq!(root.wrap, Some(20));
        assert!(root.scroll);
        assert_eq!(root.fill, Some(0xFF0000));
    }

    #[test]
    fn lower_preserves_new_input_properties() {
        let src =
            "@ontology app { @input field { placeholder: \"Email address\"; max-length: 255; } }";
        let scene = parse(src).unwrap();
        let lowered = lower(&scene, 800.0, 600.0).unwrap();
        let field_record = lowered.table.get("field").unwrap();
        let Node::Input { placeholder } = lowered.module.node(field_record.ir_node).unwrap() else {
            panic!("Expected Input node");
        };
        assert_eq!(placeholder, "Email address");
    }
}
