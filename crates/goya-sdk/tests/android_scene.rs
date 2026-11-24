//! Aragon scene compiler: admitted-construct coverage, exact-range
//! rejections, source maps, and (with a Kotlin toolchain) JVM differential
//! tests of the generated Kotlin against the Rust reference evaluators.

use goya_lang::handoff::{admit_v4, convert_field, Origin};
use goya_lang::{InteractionType, InteractionValue};
use goya_sdk::{
    compile_android_scene, compile_android_scene_with, emit_android_d1_runtime_mapped,
    AndroidSceneOptions, AndroidSceneReason, SourceMapError,
};
use std::collections::BTreeMap;

const COUNTER: &str = r#"@ontology counter {
  @persistence { cache_targets: null; }
  @state { total: int = 0 session; note: text = "none" session; }
  @cell root { id: "root"; fill: #f4f1ea;
    @cell title { id: "title"; position_p: (0.04, 0.02, 0); extent_p: (0.76, 0.05, 0); text: "Goya counter"; text-size-p: 0.6; }
    @input amount { id: "field.amount"; @iden { label: "Amount"; hint: "A whole number to add"; } position_p: (0.04, 0.085, 0); extent_p: (0.5, 0.065, 0); stroke: #555555; stroke-width-p: 0.04; }
    @input memo { id: "field.memo"; @iden { label: "Memo"; } position_p: (0.05, 0.3, 0); extent_p: (0.9, 0.08, 0); }
    @button add { id: "button.add"; position_p: (0.59, 0.085, 0); extent_p: (0.31, 0.065, 0); text: "Add"; text-size-p: 0.4; }
    @cell list { id: "list"; position_p: (0.05, 0.45, 0); extent_p: (0.9, 0.5, 0); scroll: true;
      @cell para { id: "para"; extent_p: (0.8, 2.4, 0); wrap: 12; text: "A paragraph wrapped by the reference wrapper." }
    }
  }
  @interaction add { id: "action.add"; trigger: button.add.activate; class: D1;
    state { read total: int; write total: int; write note: text; }
    input { amount: int from "field.amount"; memo: text from "field.memo"; }
    set total = state.total + input.amount; set note = input.memo; }
}"#;

fn file<'a>(scene: &'a goya_sdk::AndroidScene, name: &str) -> &'a str {
    &scene
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .text
}

#[test]
fn compiles_every_admitted_construct_deterministically_with_a_verified_source_map() {
    let scene = compile_android_scene(COUNTER).unwrap();
    assert_eq!(scene, compile_android_scene(COUNTER).unwrap());
    // `@iden` (decision D14) becomes native Android accessibility, mapped
    // back to its declaration.
    let view = &scene
        .files
        .iter()
        .find(|f| f.name == "GoyaSceneView.kt")
        .unwrap()
        .text;
    assert!(view.contains(r#"cell.identify("Amount", "A whole number to add")"#));
    assert!(view.contains(r#"cell.identify("Memo", null)"#));
    assert!(view.contains("fun identify(label: String, hint: String?)"));
    assert!(view.contains("info.hintText = name"));
    let names: Vec<_> = scene.files.iter().map(|file| file.name.as_str()).collect();
    assert_eq!(
        names,
        ["GoyaSceneSpec.kt", "GoyaSceneView.kt", "GoyaD1Runtime.kt"]
    );
    assert_eq!(
        scene.cell_ids,
        [
            "root",
            "title",
            "field.amount",
            "field.memo",
            "button.add",
            "list",
            "para"
        ]
    );
    scene.source_map.verify(COUNTER, &scene.files).unwrap();

    let handoff = admit_v4(COUNTER).unwrap();
    let origins: Vec<_> = scene
        .source_map
        .entries
        .iter()
        .map(|entry| entry.origin.clone())
        .collect();
    let property = |element: &str, property: &str| Origin::Property {
        element: element.into(),
        property: property.into(),
    };
    for expected in [
        Origin::Element("para".into()),
        property("root", "fill"),
        property("title", "position_p"),
        property("title", "extent_p"),
        property("title", "text_size_p"),
        property("field.amount", "stroke"),
        property("field.amount", "stroke_width_p"),
        property("field.memo", "position_p"),
        property("field.memo", "extent_p"),
        property("para", "text"),
        Origin::Interaction("action.add".into()),
        Origin::Binding {
            interaction: "action.add".into(),
            input: "amount".into(),
        },
        Origin::Binding {
            interaction: "action.add".into(),
            input: "memo".into(),
        },
        Origin::State("total".into()),
        Origin::State("note".into()),
    ] {
        assert!(origins.contains(&expected), "{expected:?} is not mapped");
    }

    // Wrapped text comes from the reference wrapper, not from Android.
    let view = file(&scene, "GoyaSceneView.kt");
    assert!(view.contains(r#"label("A paragraph\nwrapped by\nthe\nreference\nwrapper.")"#));
    // Lookup maps a generated literal back to its authored property.
    let literal = view.find("label(\"A paragraph").unwrap() + "label(".len();
    let entry = scene
        .source_map
        .lookup("GoyaSceneView.kt", literal)
        .unwrap();
    assert_eq!(entry.origin, property("para", "text"));
    assert_eq!(
        &COUNTER[entry.source.start..entry.source.end],
        r#"text: "A paragraph wrapped by the reference wrapper.""#
    );
    assert_eq!(handoff.origin(&entry.origin), Some(entry.source));

    let json = scene.source_map.to_json();
    assert!(json.starts_with("{\"contract\":\"ARAGON-SOURCE-MAP/1\""));
    assert_eq!(
        json,
        compile_android_scene(COUNTER).unwrap().source_map.to_json()
    );

    // The specification has no Android dependency.
    assert!(!file(&scene, "GoyaSceneSpec.kt").contains("import android"));
}

#[test]
fn gssh4_platform_rules_are_target_scoped_and_android_rules_fail_closed() {
    let other_target = r#"@ontology demo {
      @persistence { cache_targets: null; }
      @cell root { id: "root"; }
      @platform webos { "media": only-use("webos.player"); }
    }"#;
    let compiled = compile_android_scene(other_target).unwrap();
    compiled
        .source_map
        .verify(other_target, &compiled.files)
        .unwrap();

    let android = r#"@ontology demo {
      @persistence { cache_targets: null; }
      @cell root { id: "root"; }
      @platform android { "media": only-use("native.player"); }
    }"#;
    let expected_start = android.find("\"media\"").unwrap();
    let expected_end = android[expected_start..].find(';').unwrap() + expected_start + 1;
    let error = compile_android_scene(android).unwrap_err();
    assert_eq!(
        error.range,
        goya_lang::SourceRange {
            start: expected_start,
            end: expected_end
        }
    );
    assert!(error.to_string().contains("feature \"media\""));
    assert!(matches!(
        error.reason,
        AndroidSceneReason::UnsupportedPlatformException(_)
    ));
}

#[test]
fn source_map_verification_detects_edits_and_foreign_files() {
    let scene = compile_android_scene(COUNTER).unwrap();
    let edited = COUNTER.replace("Goya counter", "Goya counter!");
    assert_eq!(
        scene.source_map.verify(&edited, &scene.files),
        Err(SourceMapError::DigestMismatch)
    );
    assert!(matches!(
        scene.source_map.verify(COUNTER, &scene.files[..1]),
        Err(SourceMapError::MissingFile(_))
    ));
}

/// Each refused construct must be reported at exactly its own source range.
#[test]
fn every_unsupported_construct_is_rejected_at_its_exact_source_range() {
    let wrap = |cells: &str, extra: &str| {
        format!("@ontology t {{ @persistence {{ cache_targets: null; }} {extra} @cell root {{ id: \"root\"; {cells} }} }}")
    };
    let button = "@button go { id: \"go\"; text: \"Go\" }";
    let cases: Vec<(String, &str)> = vec![
        (wrap("stroke: #000;", ""), "stroke: #000;"),
        (wrap("stroke-width-p: 0.1;", ""), "stroke-width-p: 0.1;"),
        (wrap("text-anchor: middle;", ""), "text-anchor: middle;"),
        (
            wrap("@input f { id: \"f\"; @iden { label: \"F\"; } gestures { copy: deny; } }", ""),
            "gestures { copy: deny; }",
        ),
        (
            wrap("@button b { id: \"b\"; @cell c { id: \"c\"; } }", ""),
            "@cell c { id: \"c\"; }",
        ),
        (
            wrap(
                "@cell a { id: \"a\"; scroll: true; } @cell s { id: \"s\"; @cell a { id: \"a2\"; } }",
                "",
            ),
            "@cell a { id: \"a\"; scroll: true; }",
        ),
        (
            format!("@ontology t {{ @persistence {{ cache_targets: {{ android: \"android.preferences.v1\"; }} }} @state {{ n: int = 0 durable; }} @cell root {{ id: \"root\"; {button} }} @interaction go {{ id: \"action.go\"; trigger: button.go.activate; class: D1; state {{ write n: int; }} input {{ }} set n = 1; }} }}"),
            "n: int = 0 durable;",
        ),
        (
            format!("@ontology t {{ @persistence {{ cache_targets: null; }} @state {{ count: int = 0 session; }} @cell root {{ id: \"root\"; {button} }} @interaction go {{ id: \"action.go\"; trigger: button.go.activate; class: D2; effects {{ network; }} requests {{ lookup: effect network capability \"network\" request \"static request\" result int64; }} state {{ write count: int; }} input {{ }} set count = effect.lookup; }} }}"),
            "@interaction go",
        ),
    ];
    for (source, construct) in cases {
        let error = match compile_android_scene(&source) {
            Ok(_) => panic!("accepted {source}"),
            Err(error) => error,
        };
        let AndroidSceneReason::Unsupported(_) = error.reason else {
            panic!("{source}: unexpected {error}");
        };
        let handoff = admit_v4(&source).unwrap();
        let origin = error.origin.clone().expect("refusals name their construct");
        assert_eq!(handoff.origin(&origin), Some(error.range), "{source}");
        let start = source.find(construct).unwrap();
        assert_eq!(error.range.start, start, "{source}: {error}");
        let reported = &source[error.range.start..error.range.end];
        let core = construct.trim_end_matches(';');
        assert!(
            reported.starts_with(core)
                && (construct.starts_with('@')
                    || core.starts_with(reported.trim_end_matches(';'))
                    || reported.trim_end_matches(';') == core),
            "{source}: reported {reported:?} for {construct:?}"
        );
    }
}

#[test]
fn admission_failures_keep_the_strict_parser_range() {
    let source = "@ontology t { @persistence { cache_targets: null; } @cell root { id: \"root\"; colour: #fff; } }";
    let error = compile_android_scene(source).unwrap_err();
    let AndroidSceneReason::Admission(strict) = &error.reason else {
        panic!("{error}");
    };
    assert_eq!(strict.code, "GY-E1001");
    assert_eq!(error.range, strict.range);
    // Absolute lengths are refused by GSSH/3 admission, at the property.
    let source = "@ontology t { @persistence { cache_targets: null; } @cell root { id: \"root\"; extent: (10, 10, 0); } }";
    let error = compile_android_scene(source).unwrap_err();
    let AndroidSceneReason::Admission(strict) = &error.reason else {
        panic!("{error}");
    };
    assert_eq!(strict.code, "GY-E1009");
    assert_eq!(
        &source[error.range.start..error.range.end],
        "extent: (10, 10, 0);"
    );
    assert_eq!(
        compile_android_scene_with(
            COUNTER,
            &AndroidSceneOptions {
                package: "dev.goya".into(),
                spec_object: "GoyaSceneView".into(),
            }
        )
        .unwrap_err()
        .reason,
        AndroidSceneReason::InvalidOptions
    );
}

#[test]
fn oversized_state_and_scenes_are_refused_before_emission() {
    let long = "x".repeat(64 * 1024 + 1);
    let source = format!("@ontology t {{ @persistence {{ cache_targets: null; }} @state {{ big: text = \"{long}\" session; }} @cell root {{ id: \"root\"; @button go {{ id: \"go\"; }} }} @interaction go {{ id: \"action.go\"; trigger: button.go.activate; class: D1; state {{ write big: text; }} input {{ }} set big = \"\"; }} }}");
    let error = compile_android_scene(&source).unwrap_err();
    assert_eq!(error.origin, Some(Origin::State("big".into())));

    let mut cells = String::new();
    for index in 0..1024 {
        cells.push_str(&format!("@cell c{index} {{ id: \"c{index}\"; }} "));
    }
    let source = format!("@ontology t {{ @persistence {{ cache_targets: null; }} @cell root {{ id: \"root\"; {cells} }} }}");
    let error = compile_android_scene(&source).unwrap_err();
    assert_eq!(error.origin, Some(Origin::Element("c1023".into())));
}

// ---------------------------------------------------------------------------
// JVM differential tests. They need `kotlinc` and `java` on PATH; with
// ARAGON_REQUIRE_KOTLIN=1 a missing toolchain is a failure, not a skip.

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

fn random_fraction(rng: &mut Rng) -> String {
    rng.pick(&[
        "0",
        "1",
        "0.5",
        "0.333",
        "0.1",
        "0.9",
        "1.5",
        "-0.2",
        "0.0001",
        "0.7071067811865476",
        "3.25",
        "12",
        "1000000000000",
    ])
    .to_string()
}

fn random_cell(rng: &mut Rng, depth: u32, next: &mut usize, out: &mut String) {
    let index = *next;
    *next += 1;
    out.push_str(&format!("@cell n{index} {{ id: \"c{index}\"; "));
    if rng.below(4) > 0 {
        out.push_str(&format!(
            "extent_p: ({}, {}, 0); ",
            random_fraction(rng),
            random_fraction(rng)
        ));
    }
    if rng.below(4) > 0 {
        out.push_str(&format!(
            "position_p: ({}, {}, 0); ",
            random_fraction(rng),
            random_fraction(rng)
        ));
    }
    if rng.below(2) == 0 {
        out.push_str(&format!(
            "text: \"t\"; text-size-p: {}; ",
            random_fraction(rng)
        ));
    }
    if rng.below(3) == 0 {
        out.push_str(&format!(
            "stroke: #123456; stroke-width-p: {}; ",
            random_fraction(rng)
        ));
    }
    if rng.below(4) == 0 {
        out.push_str("scroll: true; ");
    }
    if depth < 4 {
        for _ in 0..rng.below(4) {
            random_cell(rng, depth + 1, next, out);
        }
    }
    out.push_str("} ");
}

const SIZES: &[(f64, f64)] = &[
    (0.0, 0.0),
    (1.0, 1.0),
    (320.0, 640.0),
    (411.42857142857144, 866.2857142857143),
    (1234.5, 77.25),
    (-5.0, 10.0),
    (9.3e15, 9.3e15),
];

fn layout_expectation(source: &str) -> String {
    let scene = goya_lang::parse(source).unwrap();
    let mut out = String::new();
    for (width, height) in SIZES {
        let frame = goya_lang::layout(&scene, *width, *height);
        let rects: Vec<String> = frame
            .nodes
            .iter()
            .flat_map(|node| {
                [node.rect.x, node.rect.y, node.rect.width, node.rect.height].map(|v| v.to_string())
            })
            .collect();
        let limits: Vec<String> = frame
            .nodes
            .iter()
            .map(|node| {
                let (x, y) = frame.scroll_limits(&node.name);
                format!("{x},{y}")
            })
            .collect();
        let lengths: Vec<String> = frame
            .nodes
            .iter()
            .map(|node| {
                format!(
                    "{},{}",
                    node.text_size_mp.unwrap_or(-1),
                    node.stroke_width_mp.unwrap_or(-1)
                )
            })
            .collect();
        out.push_str(&format!(
            "{} | {} | {}\n",
            rects.join(" "),
            limits.join(" "),
            lengths.join(" ")
        ));
    }
    out
}

/// Field-conversion inputs as (Kotlin expression, Rust value).
fn conversion_cases(rng: &mut Rng) -> Vec<(String, String)> {
    let mut cases: Vec<(String, String)> = [
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
        "１２",
        "9223372036854775807",
        "-9223372036854775808",
        "9223372036854775808",
        "-9223372036854775809",
        "00",
        "-01",
        "é",
        "😀",
    ]
    .iter()
    .map(|text| (goya_sdk::kotlin_string_literal(text), (*text).to_owned()))
    .collect();
    for (unit, count) in [
        ("a", 65536),
        ("a", 65537),
        ("é", 32768),
        ("é", 32769),
        ("😀", 16384),
        ("😀", 16385),
    ] {
        cases.push((
            format!("{}.repeat({count})", goya_sdk::kotlin_string_literal(unit)),
            unit.repeat(count),
        ));
    }
    let alphabet = ['0', '1', '9', '-', '+', ' ', '٣', 'a', '0'];
    for _ in 0..300 {
        let len = rng.below(22) as usize;
        let text: String = (0..len).map(|_| *rng.pick(&alphabet)).collect();
        cases.push((goya_sdk::kotlin_string_literal(&text), text));
    }
    cases
}

fn conversion_expectation(text: &str) -> String {
    let int = match convert_field(InteractionType::Int, text) {
        Ok(InteractionValue::Int(value)) => value.to_string(),
        _ => "null".to_owned(),
    };
    let fits = convert_field(InteractionType::Text, text).is_ok();
    format!("{int} {fits}\n")
}

#[derive(Clone)]
enum Val {
    Int(i64),
    Text(String),
    Bool(bool),
}

fn val_kotlin(class: &str, value: &Val) -> String {
    match value {
        Val::Int(v) if *v == i64::MIN => format!("{class}.Value.IntValue(Long.MIN_VALUE)"),
        Val::Int(v) => format!("{class}.Value.IntValue({v}L)"),
        Val::Text(v) => format!(
            "{class}.Value.TextValue({})",
            goya_sdk::kotlin_string_literal(v)
        ),
        Val::Bool(v) => format!("{class}.Value.BoolValue({v})"),
    }
}

fn val_d1(value: &Val) -> goya_sdk::D1Value {
    match value {
        Val::Int(v) => goya_sdk::D1Value::Int(*v),
        Val::Text(v) => goya_sdk::D1Value::Text(v.clone()),
        Val::Bool(v) => goya_sdk::D1Value::Bool(*v),
    }
}

fn val_show(value: &goya_sdk::D1Value) -> String {
    match value {
        goya_sdk::D1Value::Int(v) => format!("I:{v}"),
        goya_sdk::D1Value::Text(v) => format!("T:{v}"),
        goya_sdk::D1Value::Bool(v) => format!("B:{v}"),
    }
}

fn random_int(rng: &mut Rng) -> i64 {
    *rng.pick(&[
        0,
        1,
        -1,
        7,
        -40,
        i64::MAX,
        i64::MIN,
        i64::MAX - 3,
        i64::MIN + 2,
        1 << 40,
    ])
}

fn random_int_expr(
    rng: &mut Rng,
    depth: u32,
    reads: &mut Vec<String>,
    inputs: &mut Vec<String>,
) -> String {
    match if depth > 2 {
        rng.below(3)
    } else {
        rng.below(4)
    } {
        0 => random_int(rng).to_string(),
        1 => {
            let key = format!("k{}", rng.below(3));
            if !reads.contains(&key) {
                reads.push(key.clone());
            }
            format!("state.{key}")
        }
        2 => {
            let key = format!("i{}", rng.below(2));
            if !inputs.contains(&key) {
                inputs.push(key.clone());
            }
            format!("input.{key}")
        }
        _ => format!(
            "{} + {}",
            random_int_expr(rng, depth + 1, reads, inputs),
            random_int_expr(rng, depth + 1, reads, inputs)
        ),
    }
}

struct D1Case {
    class: String,
    source: String,
    kotlin_calls: String,
    expected: String,
}

/// One random D1 scene, an initial state, and a dispatch sequence, with the
/// expected outcome of each dispatch from the canonical executor.
fn d1_case(rng: &mut Rng, index: usize) -> Option<D1Case> {
    let class = format!("R{index}");
    let mut interactions = String::new();
    let mut buttons = String::new();
    let mut schemas = Vec::new();
    for action in 0..2 {
        let mut reads = Vec::new();
        let mut inputs = Vec::new();
        let mut assignments = Vec::new();
        let mut writes = Vec::new();
        for key in ["k0", "k1", "k2"] {
            if rng.below(2) == 0 {
                let expr = random_int_expr(rng, 0, &mut reads, &mut inputs);
                assignments.push(format!("set {key} = {expr};"));
                writes.push(key.to_owned());
            }
        }
        if rng.below(2) == 0 {
            let expr = if rng.below(2) == 0 {
                inputs.push("s0".into());
                "input.s0".to_owned()
            } else {
                format!("\"{}\"", rng.pick(&["", "abc", "x y"]))
            };
            assignments.push(format!("set t0 = {expr};"));
            writes.push("t0".into());
        }
        if writes.is_empty() {
            assignments.push("set b0 = true;".into());
            writes.push("b0".into());
        }
        let ty = |key: &str| match key.as_bytes()[0] {
            b'k' | b'i' => "int",
            b't' | b's' => "text",
            _ => "bool",
        };
        let mut state = String::new();
        for key in &reads {
            state.push_str(&format!("read {key}: {}; ", ty(key)));
        }
        for key in &writes {
            state.push_str(&format!("write {key}: {}; ", ty(key)));
        }
        let mut input_decl = String::new();
        for key in &inputs {
            input_decl.push_str(&format!("{key}: {}; ", ty(key)));
        }
        buttons.push_str(&format!(
            "@button b{action} {{ id: \"button.{action}\"; }} "
        ));
        interactions.push_str(&format!(
            "@interaction a{action} {{ id: \"action.{action}\"; trigger: button.b{action}.activate; class: D1; state {{ {state}}} input {{ {input_decl}}} {} }} ",
            assignments.join(" ")
        ));
        let mut touched: Vec<String> = reads.iter().chain(&writes).cloned().collect();
        touched.sort();
        touched.dedup();
        schemas.push((
            format!("action.{action}"),
            format!("a{action}"),
            touched,
            inputs,
        ));
    }
    let source = format!("@ontology d {{ @cell root {{ id: \"root\"; {buttons}}} {interactions}}}");
    let parsed = goya_lang::parse(&source).ok()?;
    let compiled = goya_lang::compile_d1_interactions(&parsed).ok()?;

    let mut keys: Vec<String> = schemas.iter().flat_map(|(_, _, t, _)| t.clone()).collect();
    keys.sort();
    keys.dedup();
    let mut state: BTreeMap<String, Val> = BTreeMap::new();
    for key in &keys {
        let value = match key.as_bytes()[0] {
            b'k' => Val::Int(random_int(rng)),
            b't' => Val::Text("start".into()),
            _ => Val::Bool(false),
        };
        state.insert(key.clone(), value);
    }
    let mut calls = format!(
        "    run {{\n        val rt = {class}(mapOf<String, {class}.Value>({}))\n",
        state
            .iter()
            .map(|(k, v)| format!(
                "{} to {}",
                goya_sdk::kotlin_string_literal(k),
                val_kotlin(&class, v)
            ))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut expected = String::new();
    let mut revision = 0u64;
    for _ in 0..12 {
        let (action_id, name, touched, inputs) = rng.pick(&schemas).clone();
        let mut input_values = BTreeMap::new();
        for key in &inputs {
            let value = if key.starts_with('s') {
                Val::Text(rng.pick(&["", "typed", "a b c"]).to_string())
            } else {
                Val::Int(random_int(rng))
            };
            input_values.insert(key.clone(), value);
        }
        calls.push_str(&format!(
            "        show(rt.dispatch({}, mapOf<String, {class}.Value>({})).toString(), rt.revision, rt.state.toSortedMap().entries.joinToString(\",\") {{ it.key + \"=\" + showValue(it.value) }})\n",
            goya_sdk::kotlin_string_literal(&action_id),
            input_values
                .iter()
                .map(|(k, v)| format!("{} to {}", goya_sdk::kotlin_string_literal(k), val_kotlin(&class, v)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let interaction = compiled.iter().find(|i| i.name() == name).unwrap();
        let snapshot = goya_sdk::D1Snapshot::new(
            revision,
            touched
                .iter()
                .map(|k| (k.clone(), val_d1(&state[k])))
                .collect(),
        )
        .unwrap();
        let input_snapshot = goya_sdk::InputSnapshot::new(
            input_values
                .iter()
                .map(|(k, v)| (k.clone(), val_d1(v)))
                .collect(),
        )
        .unwrap();
        let result = match interaction.execute(&snapshot, &input_snapshot) {
            Ok(outcome) => {
                for (key, value) in outcome.next_state {
                    let value = match value {
                        goya_sdk::D1Value::Int(v) => Val::Int(v),
                        goya_sdk::D1Value::Text(v) => Val::Text(v),
                        goya_sdk::D1Value::Bool(v) => Val::Bool(v),
                    };
                    state.insert(key, value);
                }
                revision += 1;
                format!("Success(revision={revision})")
            }
            Err(error) if error.to_string().to_lowercase().contains("overflow") => {
                "Failure(code=ARITHMETIC_OVERFLOW)".to_owned()
            }
            Err(error) => format!("Failure(unmapped: {error})"),
        };
        let shown = state
            .iter()
            .map(|(k, v)| format!("{k}={}", val_show(&val_d1(v))))
            .collect::<Vec<_>>()
            .join(",");
        expected.push_str(&format!("{result} {revision} {shown}\n"));
    }
    calls.push_str("    }\n");
    Some(D1Case {
        class,
        source,
        kotlin_calls: calls,
        expected,
    })
}

fn kotlin_toolchain() -> bool {
    let available = ["kotlinc", "java"].iter().all(|tool| {
        std::process::Command::new(tool)
            .arg("-version")
            .output()
            .is_ok_and(|output| output.status.success())
    });
    if !available {
        if std::env::var("ARAGON_REQUIRE_KOTLIN").as_deref() == Ok("1") {
            panic!("ARAGON_REQUIRE_KOTLIN=1 but kotlinc/java are unavailable");
        }
        eprintln!("JVM differential tests skipped: kotlinc/java unavailable");
    }
    available
}

#[test]
fn generated_kotlin_matches_the_rust_references_on_the_jvm_when_toolchain_exists() {
    use std::{fs, process::Command};
    if !kotlin_toolchain() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("aragon-scene-jvm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let mut rng = Rng(0x00a7_a90d);
    // One Kotlin function per case keeps every method under the JVM limit.
    let mut parts: Vec<String> = Vec::new();
    let mut main = String::from("package dev.goya.preview\n\nfun show(vararg parts: Any?) = println(parts.joinToString(\" \"))\n\nval sizes = arrayOf(");
    for (index, (w, h)) in SIZES.iter().enumerate() {
        if index > 0 {
            main.push_str(", ");
        }
        main.push_str(&format!("doubleArrayOf({w:?}, {h:?})"));
    }
    main.push_str(")\n\n");
    let mut expected = String::new();
    let mut sources = Vec::new();

    // 1. Layout and scroll limits against goya_lang::layout.
    let mut scenes = vec![COUNTER.to_owned()];
    for _ in 0..60 {
        let mut cells = String::new();
        random_cell(&mut rng, 0, &mut 0, &mut cells);
        scenes.push(format!("@ontology r {{ {cells}}}"));
    }
    for (index, source) in scenes.iter().enumerate() {
        let spec = format!("Spec{index}");
        let scene = compile_android_scene_with(
            source,
            &AndroidSceneOptions {
                package: "dev.goya.preview".into(),
                spec_object: spec.clone(),
            },
        )
        .unwrap_or_else(|error| panic!("{source}: {error}"));
        let path = dir.join(&scene.files[0].name);
        fs::write(&path, &scene.files[0].text).unwrap();
        sources.push(path);
        parts.push(format!(
            "    for (s in sizes) {{ val r = {spec}.resolve(s[0], s[1]); println(r.joinToString(\" \") + \" | \" + (0 until {spec}.CELL_COUNT).joinToString(\" \") {{ {spec}.scrollLimits(r, it).joinToString(\",\") }} + \" | \" + (0 until {spec}.CELL_COUNT).joinToString(\" \") {{ \"${{{spec}.textSizeMp(r, it)}},${{{spec}.strokeWidthMp(r, it)}}\" }}) }}\n"
        ));
        expected.push_str(&layout_expectation(source));
    }

    // 2. Field conversion against handoff::convert_field.
    for (kotlin, text) in conversion_cases(&mut rng) {
        parts.push(format!(
            "    run {{ val t = {kotlin}; println(Spec0.convertInt(t).toString() + \" \" + Spec0.textWithinLimit(t)) }}\n"
        ));
        expected.push_str(&conversion_expectation(&text));
    }

    // 3. D1 runtime against the canonical D1 executor.
    let mut d1 = 0;
    let mut attempts = 0;
    while d1 < 40 {
        attempts += 1;
        assert!(attempts < 400, "random D1 scenes keep failing admission");
        let Some(case) = d1_case(&mut rng, d1) else {
            continue;
        };
        let runtime = emit_android_d1_runtime_mapped(&case.source, &case.class)
            .unwrap_or_else(|error| panic!("{}: {error}", case.source));
        let path = dir.join(format!("{}.kt", case.class));
        fs::write(
            &path,
            format!("package dev.goya.preview\n\n{}", runtime.kotlin),
        )
        .unwrap();
        sources.push(path);
        parts.push(case.kotlin_calls);
        expected.push_str(&case.expected);
        d1 += 1;
    }
    for (index, body) in parts.iter().enumerate() {
        main.push_str(&format!("fun part{index}() {{\n{body}}}\n\n"));
    }
    main.push_str("fun main() {\n");
    for index in 0..parts.len() {
        main.push_str(&format!("    part{index}()\n"));
    }
    main.push_str("}\n\nfun showValue(v: Any): String = when {\n");
    main.push_str("    v.javaClass.simpleName == \"IntValue\" -> \"I:\" + v.javaClass.getMethod(\"getValue\").invoke(v)\n");
    main.push_str("    v.javaClass.simpleName == \"TextValue\" -> \"T:\" + v.javaClass.getMethod(\"getValue\").invoke(v)\n");
    main.push_str("    else -> \"B:\" + v.javaClass.getMethod(\"getValue\").invoke(v)\n}\n");
    let harness = dir.join("Harness.kt");
    fs::write(&harness, &main).unwrap();
    sources.push(harness);

    let jar = dir.join("harness.jar");
    let compile = Command::new("kotlinc")
        .args(&sources)
        .args(["-nowarn", "-include-runtime", "-d"])
        .arg(&jar)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "kotlinc failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new("java")
        .arg("-cp")
        .arg(&jar)
        .arg("dev.goya.preview.HarnessKt")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "harness failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let actual = String::from_utf8(run.stdout).unwrap();
    let mismatch = actual
        .lines()
        .zip(expected.lines())
        .enumerate()
        .find(|(_, (a, e))| a != e);
    if let Some((line, (a, e))) = mismatch {
        panic!("JVM/Rust mismatch at output line {line}:\n kotlin: {a}\n   rust: {e}");
    }
    assert_eq!(actual.lines().count(), expected.lines().count());
    let _ = fs::remove_dir_all(dir);
}
