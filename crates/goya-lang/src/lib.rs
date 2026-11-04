//! GY-LANG/1 — Goya scene language, RUGID-compatible syntax.
//!
//! Revised per architecture decision: the Goya surface language adopts the
//! RUGID scene grammar (github.com/elci-group/RUGID) — `@ontology` /
//! `@cell` / `@button` blocks with `position`/`extent` (absolute points) and
//! `position_p`/`extent_p` (proportional 0..1 of the parent). This replaces
//! the Wave-1 line-oriented module DSL for GUI authoring; `goya-ir`'s
//! programmatic API remains unchanged and interoperates via [`layout`].
//!
//! Contract: parsing and layout are total (never panic on any input) and
//! deterministic. See docs/contracts.md GY-LANG/1.

mod hotswap;
mod lexer;
mod lowering;
mod parser;
mod scene;
mod transition;

pub use hotswap::SceneStore;
pub use lexer::{lex, LexError, Token, TokenKind};
pub use lowering::{lower, LowerError, Lowered, NodeKey, NodeRecord, NodeTable, Role};
pub use parser::{parse, ParseError};
pub use scene::{hit_test, layout, Anchor, CellKind, DrawNode, RectMp, Scene, SceneCell, Vec3};
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
    fn hit_test_returns_button_name() {
        let scene = parse(HELLO).unwrap();
        let frame = layout(&scene, 800.0, 600.0);
        assert_eq!(hit_test(&frame, 400_000, 330_000), Some("go".to_string()));
        assert_eq!(hit_test(&frame, 10_000, 10_000), None);
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
            let parsed = std::panic::catch_unwind(|| parse(&input));
            if let Ok(Ok(scene)) = parsed {
                let _ = std::panic::catch_unwind(|| layout(&scene, 800.0, 600.0));
            }
        }
    }
}
