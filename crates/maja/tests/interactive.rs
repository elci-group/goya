//! Experimental interactive profile: CLI behaviour, deterministic output,
//! and Node differentials of the layout and field-conversion mirrors.
//! Node is required; set `MAJA_SKIP_NODE=1` to skip those parts explicitly.

use goya_lang::handoff::{admit_v3, convert_field};
use goya_lang::{InteractionType, InteractionValue};
use std::{ffi::OsString, fs, path::PathBuf, process::Command};

const COUNTER: &str = include_str!("../../../fixtures/nevada/counter.gscene");

/// A scene that exercises nesting, clamping, defaults, fills, strokes,
/// proportional text size and wrap.
const LAYOUT: &str = r#"@ontology lay {
    @cell root {
        id: "root";
        fill: #102030;
        @cell a {
            id: "a";
            position_p: (0.1, 0.2, 0); extent_p: (0.5, 0.3, 0);
            text: "one two three four five six"; wrap: 7; text_size_p: 0.25;
            @cell a1 { id: "a1"; position_p: (0.9, 0.9, 0); extent_p: (0.5, 0.5, 0); stroke: #ff0000; stroke_width_p: 0.7; }
            @cell a2 { id: "a2"; position_p: (-0.5, 1.5, 0); }
            @button a3 { id: "a3"; extent_p: (0.333333333, 0.1, 0); text: "Go"; text_size_p: 2.5; }
        }
        @cell b { id: "b"; extent_p: (1.0, 0.07, 0); position_p: (0, 0.93, 0); stroke: #000; stroke_width_p: 0.01; }
        @input c { id: "c"; position_p: (0.7, 0.7, 0); extent_p: (0.6, 0.6, 0); }
    }
}"#;

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("maja-interactive-{}-{name}", std::process::id()))
}

fn cli(args: &[&str]) -> (i32, String, String) {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let args = std::iter::once("maja")
        .chain(args.iter().copied())
        .map(OsString::from);
    let code = maja::run(args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn node_available() -> bool {
    let ok = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        assert!(
            std::env::var_os("MAJA_SKIP_NODE").is_some(),
            "node is required; set MAJA_SKIP_NODE=1 to skip"
        );
        eprintln!("SKIPPED: node unavailable and MAJA_SKIP_NODE set");
    }
    ok
}

fn build(source: &str, name: &str) -> PathBuf {
    let src = temp(&format!("{name}.gscene"));
    fs::write(&src, source).unwrap();
    let out = temp(name);
    let _ = fs::remove_dir_all(&out);
    let (code, stdout, stderr) = cli(&[
        "build",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v2",
        "--out",
        out.to_str().unwrap(),
    ]);
    fs::remove_file(&src).unwrap();
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "built: experimental interactive subset\n");
    out
}

fn run_node(dir: &std::path::Path, driver: &str) -> String {
    fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").unwrap();
    fs::write(dir.join("driver.js"), driver).unwrap();
    let output = Command::new("node")
        .arg(dir.join("driver.js"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn counter_builds_deterministically_with_hashed_manifest() {
    let first = build(COUNTER, "det-a");
    let second = build(COUNTER, "det-b");
    let mut names: Vec<_> = fs::read_dir(&first)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "app.css",
            "app.js",
            "d1.js",
            "index.html",
            "layout.js",
            "manifest.json",
            "scene.js"
        ]
    );
    for name in &names {
        assert_eq!(
            fs::read(first.join(name)).unwrap(),
            fs::read(second.join(name)).unwrap(),
            "{name} differs between builds"
        );
    }
    let manifest = fs::read_to_string(first.join("manifest.json")).unwrap();
    use sha2::{Digest, Sha256};
    for name in names.iter().filter(|n| *n != "manifest.json") {
        let digest: String = Sha256::digest(fs::read(first.join(name)).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert!(
            manifest.contains(&format!("\"{name}\": \"{digest}\"")),
            "{name} hash missing"
        );
    }
    assert!(manifest.contains("\"handoff\": \"GSSH/3\""));
    assert!(manifest.contains("\"action.add\": ["));
    let tmp = temp("det-a").to_string_lossy().into_owned();
    for name in &names {
        let text = fs::read_to_string(first.join(name)).unwrap();
        assert!(!text.contains(&tmp), "{name} embeds a host path");
    }
    let scene = fs::read_to_string(first.join("scene.js")).unwrap();
    assert!(scene.contains(r#"["window.add", "action.add", [["amount", "int", "window.amount"]]]"#));
    assert!(scene.contains(r#"["count", "int", "0"]"#));
    // Existing output is never replaced.
    let src = temp("again.gscene");
    fs::write(&src, COUNTER).unwrap();
    let (code, _, stderr) = cli(&[
        "build",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v2",
        "--out",
        first.to_str().unwrap(),
    ]);
    assert_eq!(code, 5, "{stderr}");
    fs::remove_file(src).unwrap();
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

#[test]
fn cli_refuses_with_source_ranges_and_rejects_release() {
    let src = temp("refuse.gscene");
    fs::write(&src, COUNTER.replace("= 0 session", "= 0 durable")).unwrap();
    let (code, out, err) = cli(&[
        "check",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v2",
    ]);
    assert_eq!(code, 4);
    assert!(out.is_empty());
    assert!(err.starts_with("MAJA-E0200: "), "{err}");
    assert!(err.contains("[bytes "), "{err}");
    fs::write(&src, COUNTER).unwrap();
    let (code, out, _) = cli(&[
        "check",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v2",
    ]);
    assert_eq!(code, 0);
    assert_eq!(out, "admitted: experimental interactive subset\n");
    let dest = temp("release");
    let (code, _, err) = cli(&[
        "build",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v2",
        "--release",
        "--out",
        dest.to_str().unwrap(),
    ]);
    assert_eq!(code, 4);
    assert!(err.starts_with("MAJA-CLI-E0006"));
    assert!(!dest.exists());
    // The v1 static profile still refuses GSSH/2 declarations.
    let (code, _, err) = cli(&[
        "check",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v1",
    ]);
    assert_eq!(code, 4);
    assert!(err.starts_with("GY-E1006"), "{err}");
    fs::remove_file(src).unwrap();
}

/// A scene with two sibling scroll viewports whose direct children
/// overflow, grandchildren clamped inside them, negative fractions and
/// content that does not overflow.
const SCROLL: &str = include_str!("../../../tests/maja/fixtures/scroll.gscene");

#[test]
fn layout_mirror_matches_reference_layout() {
    if !node_available() {
        return;
    }
    let scene_js = layout_differential(LAYOUT, "layout");
    assert!(
        scene_js.contains("text: \"one two\\u000athree\\u000afour\\u000afive\\u000asix\""),
        "wrap must come from the reference frame: {scene_js}"
    );
}

#[test]
fn layout_mirror_matches_reference_scroll_layout_and_limits() {
    if !node_available() {
        return;
    }
    let scene_js = layout_differential(SCROLL, "scroll");
    assert!(scene_js.contains("scroll: true"));
}

/// Compare `resolve` and `scrollLimits` with `goya_lang::layout` and
/// `Frame::scroll_limits` at several viewports. Returns `scene.js`.
fn layout_differential(source: &str, name: &str) -> String {
    let dir = build(source, name);
    let viewports: [(f64, f64); 8] = [
        (0.0, 0.0),
        (1.0, 1.0),
        (320.0, 568.0),
        (375.5, 812.25),
        (1280.0, 720.0),
        (1919.0, 1079.0),
        (3.0, 7.0),
        (100000.0, 1.0),
    ];
    let mut driver = String::from(
        "import { CELLS } from \"./scene.js\";\nimport { resolve, scrollLimits } from \"./layout.js\";\nconst out = [];\n",
    );
    for (w, h) in viewports {
        driver.push_str(&format!(
            "{{ const r = resolve(CELLS, {w:?}, {h:?}); const l = scrollLimits(CELLS, r); r.forEach((n, i) => out.push(`${{n.rect.x}},${{n.rect.y}},${{n.rect.width}},${{n.rect.height}},${{n.textSize}},${{n.strokeWidth}},${{l[i] === null ? \"-\" : l[i].join(\":\")}}`)); }}\n"
        ));
    }
    driver.push_str("process.stdout.write(out.join(\"\\n\") + \"\\n\");\n");
    let actual = run_node(&dir, &driver);
    let scene = &admit_v3(source).unwrap().parsed().scene.clone();
    let mut expected = String::new();
    for (w, h) in viewports {
        let frame = goya_lang::layout(scene, w, h);
        for node in &frame.nodes {
            let opt = |v: Option<i64>| v.map_or("null".to_owned(), |v| v.to_string());
            let limits = if node.scroll {
                let (x, y) = frame.scroll_limits(&node.name);
                format!("{x}:{y}")
            } else {
                "-".to_owned()
            };
            expected.push_str(&format!(
                "{},{},{},{},{},{},{limits}\n",
                node.rect.x,
                node.rect.y,
                node.rect.width,
                node.rect.height,
                opt(node.text_size_mp),
                opt(node.stroke_width_mp),
            ));
        }
    }
    assert_eq!(actual, expected);
    if name == "scroll" {
        assert!(
            expected.lines().any(|l| !l.ends_with(",-") && !l.ends_with(",0:0")),
            "scroll corpus never overflows"
        );
    }
    let scene_js = fs::read_to_string(dir.join("scene.js")).unwrap();
    fs::remove_dir_all(dir).unwrap();
    scene_js
}

#[test]
fn field_conversion_mirror_matches_convert_field() {
    if !node_available() {
        return;
    }
    let dir = build(COUNTER, "convert");
    let limit = "a".repeat(64 * 1024);
    let over = format!("{limit}a");
    let multibyte = "€".repeat(64 * 1024 / 3);
    let multibyte_over = format!("{multibyte}€");
    let cases: Vec<&str> = vec![
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
        "１",
        "9223372036854775807",
        "-9223372036854775808",
        "9223372036854775808",
        "-9223372036854775809",
        "99999999999999999999999",
        "00",
        "-01",
        "γειά 🌍",
        "\u{0}\n\t",
        &limit,
        &over,
        &multibyte,
        &multibyte_over,
    ];
    let json = |s: &str| {
        let mut out = String::from("\"");
        for unit in s.encode_utf16() {
            match unit {
                0x22 => out.push_str("\\\""),
                0x5c => out.push_str("\\\\"),
                0x20..=0x7e => out.push(unit as u8 as char),
                _ => out.push_str(&format!("\\u{unit:04x}")),
            }
        }
        out.push('"');
        out
    };
    let mut driver = String::from(
        "import { convertField } from \"./layout.js\";\nconst out = [];\nconst show = (r) => r.ok ? `ok:${r.value.type}:${Buffer.from(String(r.value.value), \"utf8\").toString(\"hex\")}` : `err:${r.error}`;\n",
    );
    let mut expected = String::new();
    for text in &cases {
        for (ty, name) in [
            (InteractionType::Int, "int"),
            (InteractionType::Text, "text"),
            (InteractionType::Bool, "bool"),
        ] {
            driver.push_str(&format!(
                "out.push(show(convertField(\"{name}\", {})));\n",
                json(text)
            ));
            let line = match convert_field(ty, text) {
                Ok(InteractionValue::Int(n)) => format!(
                    "ok:int:{}",
                    n.to_string()
                        .bytes()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                ),
                Ok(InteractionValue::Text(s)) => format!(
                    "ok:text:{}",
                    s.bytes().map(|b| format!("{b:02x}")).collect::<String>()
                ),
                Ok(InteractionValue::Bool(_)) => unreachable!(),
                Err(goya_lang::handoff::FieldConversionError::NotCanonicalInt) => {
                    "err:NOT_CANONICAL_INT".into()
                }
                Err(goya_lang::handoff::FieldConversionError::TextLimit) => "err:TEXT_LIMIT".into(),
                Err(goya_lang::handoff::FieldConversionError::Unsupported) => {
                    "err:UNSUPPORTED".into()
                }
            };
            expected.push_str(&line);
            expected.push('\n');
        }
    }
    driver.push_str("process.stdout.write(out.join(\"\\n\") + \"\\n\");\n");
    assert_eq!(run_node(&dir, &driver), expected);
    // A lone surrogate cannot reach Rust; the mirror refuses it.
    let lone = run_node(
        &dir,
        "import { convertField } from \"./layout.js\";\nprocess.stdout.write(convertField(\"text\", \"a\\ud800\").error + \"\\n\");\n",
    );
    assert_eq!(lone, "INVALID_TEXT\n");
    fs::remove_dir_all(dir).unwrap();
}
