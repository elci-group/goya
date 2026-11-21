//! `text-from` under GSSH/3 live admission (proposed; the construct comes
//! from the Arachnid live-binding proposal, docs/gssh/decisions.md D9).

use goya_lang::handoff::{admit_v2, admit_v3, admit_v3_live, Origin, CONTRACT_V3_LIVE};
use goya_lang::{parse_strict, parse_strict_gssh3_live};

const SCENE: &str = r#"@ontology tally {
    @state { count: int = 0 session; label: text = "hi" session; }
    @cell root {
        id: "root";
        @cell total { id: "total"; position_p: (0, 0, 0); extent_p: (1, 0.5, 0); text: "Total: "; text-from: state.count; }
        @cell shown { id: "shown"; text-from: state.label; }
        @input amount { id: "amount"; }
        @button add { id: "add"; }
    }
    @interaction add {
        id: "action.add";
        trigger: button.add.activate;
        class: D1;
        state { read count: int; write count: int; }
        input { amount: int from "amount"; }
        set count = state.count + input.amount;
    }
}"#;

fn slice(range: goya_lang::SourceRange) -> &'static str {
    &SCENE[range.start..range.end]
}

#[test]
fn live_admission_carries_text_from_and_counts_it_as_a_use() {
    let handoff = admit_v3_live(SCENE).unwrap();
    assert_eq!(handoff.contract(), CONTRACT_V3_LIVE);
    let scene = &handoff.parsed().scene;
    let total = &scene.root.children[0];
    assert_eq!(total.text_from.as_deref(), Some("count"));
    assert_eq!(total.text.as_deref(), Some("Total: "));
    // `label` is used only by a display binding and is still admitted.
    assert_eq!(scene.root.children[1].text_from.as_deref(), Some("label"));
    let origin = handoff
        .origin(&Origin::Property {
            element: "total".into(),
            property: "text_from".into(),
        })
        .unwrap();
    assert_eq!(slice(origin), "text-from: state.count;");
}

#[test]
fn older_profiles_refuse_text_from_at_its_range() {
    for error in [
        parse_strict(SCENE).unwrap_err(),
        admit_v2(SCENE).unwrap_err(),
        admit_v3(SCENE).unwrap_err(),
    ] {
        // GSSH/1 stops at the @state block first; the others at text-from.
        if error.code == "GY-E1006" {
            continue;
        }
        assert_eq!(error.code, "GY-E1012");
        assert!(slice(error.range).starts_with("text-from: state.count"));
    }
    let error = admit_v3(SCENE).unwrap_err();
    assert_eq!(error.code, "GY-E1012");
}

#[test]
fn invalid_text_from_is_refused() {
    let undeclared = SCENE.replace("text-from: state.label;", "text-from: state.nope;");
    let error = parse_strict_gssh3_live(&undeclared).unwrap_err();
    assert_eq!(error.code, "GY-E1013");
    assert_eq!(
        &undeclared[error.range.start..error.range.end],
        "text-from: state.nope;"
    );
    let bare = SCENE.replace("text-from: state.label;", "text-from: label;");
    assert_eq!(parse_strict_gssh3_live(&bare).unwrap_err().code, "GY-E1000");
    let unused = SCENE.replace("text-from: state.label;", "");
    let error = parse_strict_gssh3_live(&unused).unwrap_err();
    assert_eq!(error.code, "GY-E1008");
    let absolute = SCENE.replace("position_p: (0, 0, 0);", "position: (0, 0, 0);");
    assert_eq!(
        parse_strict_gssh3_live(&absolute).unwrap_err().code,
        "GY-E1009"
    );
}

#[test]
fn a_root_whose_only_property_is_text_from_is_kept() {
    let source = r#"@ontology one { @state { n: int = 3 session; } @cell root { text-from: state.n; @cell a { id: "a"; } } }"#;
    let parsed = goya_lang::parse_with_source_map(source).unwrap();
    assert_eq!(parsed.scene.root.text_from.as_deref(), Some("n"));
}
