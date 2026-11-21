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
    assert!(scene.contains(r#"["count", "int", "0", "session"]"#));
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
    fs::write(
        &src,
        COUNTER.replace(
            "text: \"Counter\";",
            "text: \"Counter\"; text-anchor: middle;",
        ),
    )
    .unwrap();
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
            expected
                .lines()
                .any(|l| !l.ends_with(",-") && !l.ends_with(",0:0")),
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

/// Durable keys of every type next to a session key; `add` writes two
/// durable keys in one transition.
const DURABLE: &str = r#"@ontology notes {
    @state {
        count: int = 0 durable;
        label: text = "start" durable;
        flag: bool = false durable;
        ticks: int = 0 session;
    }
    @cell root {
        id: "root";
        @input amount { id: "field.amount"; }
        @input name { id: "field.name"; }
        @button add { id: "button.add"; }
        @button tick { id: "button.tick"; }
        @button raise { id: "button.raise"; }
    }
    @interaction add {
        id: "action.add";
        trigger: button.add.activate;
        class: D1;
        state { read count: int; write count: int; write label: text; }
        input { amount: int from "field.amount"; value: text from "field.name"; }
        set count = state.count + input.amount;
        set label = input.value;
    }
    @interaction tick {
        id: "action.tick";
        trigger: button.tick.activate;
        class: D1;
        state { read ticks: int; write ticks: int; }
        input { }
        set ticks = state.ticks + 1;
    }
    @interaction raise {
        id: "action.raise";
        trigger: button.raise.activate;
        class: D1;
        state { write flag: bool; }
        input { }
        set flag = true;
    }
}"#;

/// Fake `localStorage` that counts writes and can fail reads or writes.
const FAKE_STORAGE: &str = r#"const store = new Map();
const io = { failGet: false, failSet: false, sets: 0 };
globalThis.localStorage = {
  getItem(k) { if (io.failGet) throw new Error("SecurityError"); return store.has(k) ? store.get(k) : null; },
  setItem(k, v) { if (io.failSet) throw new Error("QuotaExceededError"); io.sets++; store.set(k, String(v)); },
};
const show = (m) => [...m].map(([k, v]) => `${k}=${v.type}:${String(v.value).length > 12 ? `${String(v.value).length}ch` : v.value}`).join(",");
"#;

#[test]
fn durable_state_adapter_follows_gssh2_restore_rules() {
    let dir = build(DURABLE, "durable");
    let scene = fs::read_to_string(dir.join("scene.js")).unwrap();
    assert!(scene.contains("export const ONTOLOGY = \"notes\";"));
    assert!(scene.contains(r#"["count", "int", "0", "durable"]"#));
    assert!(scene.contains(r#"["ticks", "int", "0", "session"]"#));
    let manifest = fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(manifest.contains("\"storage.js\": \""));
    let app = fs::read_to_string(dir.join("app.js")).unwrap();
    assert!(app.contains("import { restore, persister } from \"./storage.js\";"));
    // Session-only scenes do not get the adapter.
    let session = build(COUNTER, "session-only");
    assert!(!session.join("storage.js").exists());
    assert!(fs::read_to_string(session.join("app.js"))
        .unwrap()
        .contains("const restore = null, persister = null;"));
    fs::remove_dir_all(session).unwrap();
    if !node_available() {
        fs::remove_dir_all(dir).unwrap();
        return;
    }
    let driver = format!(
        r#"import {{ restore, persister, recordKey }} from "./storage.js";
import {{ createRuntime, intValue, textValue }} from "./d1.js";
import {{ INITIAL_STATE, ONTOLOGY }} from "./scene.js";
{FAKE_STORAGE}
const out = [];
const K = recordKey(ONTOLOGY);
const boot = () => {{
  const r = restore(ONTOLOGY, INITIAL_STATE);
  const rt = createRuntime(r.state);
  const p = persister(ONTOLOGY, INITIAL_STATE, r.preserved, r.seen);
  return {{ r, rt, hook: (w) => {{ const x = p(rt.snapshot().state, w); return x.ok ? true : x.conflict ? "STORAGE_CONFLICT" : false; }} }};
}};
out.push(K);
// Missing record: authored initial values.
let {{ r, rt, hook }} = boot();
out.push(`missing:${{r.ok}}:${{r.code}}:${{show(r.state)}}`);
// A session-only transition never touches storage.
out.push(`tick:${{rt.dispatch("action.tick", new Map(), hook).ok}}:${{io.sets}}:${{store.has(K)}}`);
// Two durable writes in one transition: one setItem with the whole durable snapshot.
out.push(`add:${{rt.dispatch("action.add", new Map([["amount", intValue(-5n)], ["value", textValue("γειά 🌍")]]), hook).ok}}:${{io.sets}}`);
out.push(store.get(K));
// Reload: durable keys restored, session key back to its initial value.
({{ r, rt, hook }} = boot());
out.push(`reload:${{r.ok}}:${{rt.revision}}:${{show(r.state)}}`);
// A refused write refuses the commit; state, revision and record unchanged.
io.failSet = true;
let before = store.get(K);
const refused = rt.dispatch("action.raise", new Map(), hook);
out.push(`refused:${{refused.code}}:${{rt.revision}}:${{rt.get("flag").value}}:${{store.get(K) === before}}`);
io.failSet = false;
// Corrupt records fail restore with the authored values and stay untouched.
const big = "a".repeat(65537);
for (const [name, text] of [
  ["not-json", "{{"],
  ["array", "[]"],
  ["format", '{{"format":"goya.state.v2","values":{{}}}}'],
  ["extra", '{{"format":"goya.state.v1","values":{{}},"x":1}}'],
  ["values", '{{"format":"goya.state.v1","values":[]}}'],
  ["retyped", '{{"format":"goya.state.v1","values":{{"count":["text","5"]}}}}'],
  ["unknown-type", '{{"format":"goya.state.v1","values":{{"count":["float","5"]}}}}'],
  ["leading-zero", '{{"format":"goya.state.v1","values":{{"count":["int","05"]}}}}'],
  ["number", '{{"format":"goya.state.v1","values":{{"count":["int",5]}}}}'],
  ["overflow", '{{"format":"goya.state.v1","values":{{"count":["int","9223372036854775808"]}}}}'],
  ["arity", '{{"format":"goya.state.v1","values":{{"count":["int","5","x"]}}}}'],
  ["bool-text", '{{"format":"goya.state.v1","values":{{"flag":["bool","true"]}}}}'],
  ["surrogate", '{{"format":"goya.state.v1","values":{{"label":["text","\\ud800"]}}}}'],
  ["text-limit", `{{"format":"goya.state.v1","values":{{"label":["text","${{big}}"]}}}}`],
  ["all-or-nothing", '{{"format":"goya.state.v1","values":{{"count":["int","3"],"flag":["bool",1]}}}}'],
  ["partial", '{{"format":"goya.state.v1","values":{{"count":["int","3"]}}}}'],
]) {{
  store.set(K, text);
  const x = restore(ONTOLOGY, INITIAL_STATE);
  out.push(`${{name}}:${{x.ok}}:${{x.code}}:${{show(x.state)}}:${{store.get(K) === text}}`);
}}
io.failGet = true;
const u = restore(ONTOLOGY, INITIAL_STATE);
out.push(`unavailable:${{u.ok}}:${{u.code}}`);
io.failGet = false;
// After a failed restore the first durable commit replaces the record;
// entries this scene does not declare durable are kept, without touching
// Object.prototype.
store.set(K, '{{"format":"goya.state.v1","values":{{"ticks":["int","9"],"__proto__":["int","1"],"count":["bool",true]}}}}');
({{ r, rt, hook }} = boot());
out.push(`mismatch:${{r.code}}:${{show(r.state)}}`);
out.push(`replace:${{rt.dispatch("action.raise", new Map(), hook).ok}}`);
out.push(store.get(K));
out.push(`proto:${{Object.prototype[0] === undefined && {{}}.length === undefined}}`);
({{ r }} = boot());
out.push(`after:${{r.ok}}:${{show(r.state)}}`);
// Two windows (decision D8): B's commit after A's is refused as a conflict
// and A's record survives; B recovers by reloading.
const a = boot(), b = boot();
out.push(`a:${{a.rt.dispatch("action.add", new Map([["amount", intValue(1n)], ["value", textValue("A")]]), a.hook).ok}}`);
before = store.get(K);
const conflict = b.rt.dispatch("action.add", new Map([["amount", intValue(2n)], ["value", textValue("B")]]), b.hook);
out.push(`b:${{conflict.code}}:${{b.rt.revision}}:${{b.rt.get("label").value}}:${{store.get(K) === before}}`);
out.push(`b-session:${{b.rt.dispatch("action.tick", new Map(), b.hook).ok}}`);
const b2 = boot();
out.push(`b-reload:${{show(b2.r.state)}}:${{b2.rt.dispatch("action.add", new Map([["amount", intValue(2n)], ["value", textValue("B")]]), b2.hook).ok}}`);
out.push(`a-again:${{a.rt.dispatch("action.raise", new Map(), a.hook).code}}`);
// Clearing the record elsewhere is a conflict too.
store.delete(K);
out.push(`cleared:${{b2.rt.dispatch("action.raise", new Map(), b2.hook).code}}:${{store.has(K)}}`);
process.stdout.write(out.join("\n") + "\n");
"#
    );
    let got = run_node(&dir, &driver);
    fs::remove_dir_all(dir).unwrap();
    let initial = "count=int:0,label=text:start,flag=bool:false,ticks=int:0";
    let mut expected = vec![
        "goya.state.v1/notes".to_owned(),
        format!("missing:true:null:{initial}"),
        "tick:true:0:false".into(),
        "add:true:1".into(),
        r#"{"format":"goya.state.v1","values":{"count":["int","-5"],"label":["text","γειά 🌍"],"flag":["bool",false]}}"#.into(),
        "reload:true:0:count=int:-5,label=text:γειά 🌍,flag=bool:false,ticks=int:0".into(),
        "refused:STORAGE_FAILED:0:false:true".into(),
    ];
    for (name, code) in [
        ("not-json", "RECORD_UNDECODABLE"),
        ("array", "RECORD_UNDECODABLE"),
        ("format", "RECORD_UNDECODABLE"),
        ("extra", "RECORD_UNDECODABLE"),
        ("values", "RECORD_UNDECODABLE"),
        ("retyped", "RECORD_TYPE_MISMATCH"),
        ("unknown-type", "RECORD_UNDECODABLE"),
        ("leading-zero", "RECORD_UNDECODABLE"),
        ("number", "RECORD_UNDECODABLE"),
        ("overflow", "RECORD_UNDECODABLE"),
        ("arity", "RECORD_UNDECODABLE"),
        ("bool-text", "RECORD_UNDECODABLE"),
        ("surrogate", "RECORD_UNDECODABLE"),
        ("text-limit", "RECORD_UNDECODABLE"),
        ("all-or-nothing", "RECORD_UNDECODABLE"),
    ] {
        expected.push(format!("{name}:false:{code}:{initial}:true"));
    }
    expected.extend([
        "partial:true:null:count=int:3,label=text:start,flag=bool:false,ticks=int:0:true".into(),
        "unavailable:false:STORAGE_UNAVAILABLE".into(),
        format!("mismatch:RECORD_TYPE_MISMATCH:{initial}"),
        "replace:true".into(),
        r#"{"format":"goya.state.v1","values":{"ticks":["int","9"],"__proto__":["int","1"],"count":["int","0"],"label":["text","start"],"flag":["bool",true]}}"#.into(),
        "proto:true".into(),
        "after:true:count=int:0,label=text:start,flag=bool:true,ticks=int:0".into(),
        "a:true".into(),
        "b:STORAGE_CONFLICT:0:start:true".into(),
        "b-session:true".into(),
        "b-reload:count=int:1,label=text:A,flag=bool:true,ticks=int:0:true".into(),
        "a-again:STORAGE_CONFLICT".into(),
        "cleared:STORAGE_CONFLICT:false".into(),
    ]);
    assert_eq!(got.lines().collect::<Vec<_>>(), expected);
}

#[test]
fn durable_restore_refuses_snapshots_over_the_d1_limit() {
    if !node_available() {
        return;
    }
    // 17 text keys of 64 KiB exceed the 1 MiB snapshot limit; 15 do not.
    const N: usize = 17;
    let decls: String = (0..N)
        .map(|i| format!("t{i:02}: text = \"\" durable; "))
        .collect();
    let writes: String = (0..N).map(|i| format!("write t{i:02}: text; ")).collect();
    let sets: String = (0..N).map(|i| format!("set t{i:02} = input.v; ")).collect();
    let source = format!(
        r#"@ontology big {{ @state {{ {decls}}} @cell root {{ id: "root"; @input v {{ id: "f"; }} @button go {{ id: "b"; }} }} @interaction go {{ id: "action.go"; trigger: button.go.activate; class: D1; state {{ {writes}}} input {{ v: text from "f"; }} {sets}}} }}"#
    );
    let dir = build(&source, "durable-limit");
    let driver = format!(
        r#"import {{ restore, recordKey }} from "./storage.js";
import {{ INITIAL_STATE, ONTOLOGY }} from "./scene.js";
{FAKE_STORAGE}
const full = "a".repeat(65536);
for (const n of [15, 17]) {{
  const values = {{}};
  for (let i = 0; i < n; i++) values[`t${{String(i).padStart(2, "0")}}`] = ["text", full];
  store.set(recordKey(ONTOLOGY), JSON.stringify({{ format: "goya.state.v1", values }}));
  const r = restore(ONTOLOGY, INITIAL_STATE);
  console.log(`${{n}}:${{r.ok}}:${{r.code}}:${{[...r.state.values()].filter((v) => v.value.length).length}}`);
}}
"#
    );
    let got = run_node(&dir, &driver);
    fs::remove_dir_all(dir).unwrap();
    assert_eq!(got, "15:true:null:15\n17:false:RECORD_LIMIT:0\n");
}

#[test]
fn live_profile_builds_display_bindings() {
    let source = include_str!("../../../tests/maja/fixtures/display-counter.gscene");
    let src = temp("display.gscene");
    fs::write(&src, source).unwrap();
    // v2 refuses `text-from`; v3 builds it.
    let (code, _, err) = cli(&[
        "check",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v2",
    ]);
    assert_eq!(code, 4);
    assert!(err.starts_with("GY-E1012"), "{err}");
    let out = temp("display");
    let _ = fs::remove_dir_all(&out);
    let (code, stdout, stderr) = cli(&[
        "build",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v3",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(stdout, "built: experimental interactive subset\n");
    let (code, _, err) = cli(&[
        "build",
        src.to_str().unwrap(),
        "--profile",
        "maja.web.local.v3",
        "--release",
        "--out",
        temp("display-release").to_str().unwrap(),
    ]);
    assert_eq!(code, 4);
    assert!(err.starts_with("MAJA-CLI-E0006"));
    fs::remove_file(src).unwrap();
    let scene = fs::read_to_string(out.join("scene.js")).unwrap();
    assert!(scene.starts_with("// Generated by Maja (maja.web.local.v3)."));
    assert!(scene.contains(r#"text: "Count: ", scroll: false, textFrom: "count" }"#));
    assert!(scene.contains(r#"text: "Add", scroll: false, textFrom: null }"#));
    let manifest = fs::read_to_string(out.join("manifest.json")).unwrap();
    assert!(manifest.contains("\"profile\": \"maja.web.local.v3\""));
    assert!(manifest.contains("\"handoff\": \"GSSH/3-live\""));
    assert!(out.join("storage.js").exists());
    fs::remove_dir_all(out).unwrap();
}

/// `shownText`, `displayContext`, `describeChange` and `announcement` in
/// layout.js against `goya_lang::display_binding` (decision D9).
#[test]
fn display_binding_mirror_matches_reference() {
    use goya_lang::display_binding as reference;
    use InteractionValue::{Bool, Int, Text};
    const NONE: &str = "<none>";
    if !node_available() {
        return;
    }
    let dir = build(COUNTER, "display-mirror");
    let js_str = |s: &str| {
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
    let js_val = |v: &InteractionValue| match v {
        Int(n) => format!("{{type:\"int\",value:{n}n}}"),
        Text(t) => format!("{{type:\"text\",value:{}}}", js_str(t)),
        Bool(b) => format!("{{type:\"bool\",value:{b}}}"),
    };
    let contexts: [Option<&str>; 10] = [
        Some("Count: "),
        Some("Done:\r\n"),
        Some("Total (€):\t\n"),
        Some("Ratio::"),
        Some(" : "),
        None,
        Some(""),
        Some("γειά:  "),
        Some("a\u{feff}"),
        Some(" lead: x "),
    ];
    let pairs = [
        (Int(0), Int(0)),
        (Int(-12), Int(-7)),
        (Int(i64::MAX), Int(i64::MIN)),
        (Int(i64::MIN), Int(i64::MAX)),
        (Int(5), Int(-5)),
        (Text("none".into()), Text(String::new())),
        (Text("a".into()), Text("a".into())),
        (Text(String::new()), Text("γειά 🌍".into())),
        (Text("q\"uote".into()), Text("x\ny".into())),
        (Bool(false), Bool(true)),
        (Bool(true), Bool(false)),
        (Bool(true), Bool(true)),
    ];
    let mut driver = String::from(
        "import { shownText, displayContext, describeChange, announcement } from \"./layout.js\";\nconst out = [];\nconst q = (s) => s === null ? \"null\" : Buffer.from(s, \"utf8\").toString(\"hex\");\n",
    );
    let mut expected = Vec::new();
    for c in contexts {
        let arg = c.map_or("undefined".into(), js_str);
        driver.push_str(&format!("out.push(q(displayContext({arg}, \"key\")));\n"));
        expected.push(reference::context(c, "key"));
    }
    for (old, new) in &pairs {
        driver.push_str(&format!(
            "out.push(q(shownText({})));\nout.push(q(describeChange(\"Ctx\", {}, {})));\n",
            js_val(new),
            js_val(old),
            js_val(new)
        ));
        expected.push(reference::shown_text(new));
        expected.push(reference::describe_change("Ctx", old, new).unwrap_or_else(|| NONE.into()));
    }
    let list: Vec<String> = pairs
        .iter()
        .map(|(o, n)| format!("[\"Ctx\", {}, {}]", js_val(o), js_val(n)))
        .collect();
    driver.push_str(&format!(
        "out.push(q(announcement([{}])));\nout.push(q(announcement([[\"Ctx\", {}, {}]])));\nprocess.stdout.write(out.join(\"\\n\") + \"\\n\");\n",
        list.join(", "),
        js_val(&Int(1)),
        js_val(&Int(1))
    ));
    expected.push(reference::announcement(pairs.iter().map(|(o, n)| ("Ctx", o, n))).unwrap());
    expected.push(NONE.into());
    let got = run_node(&dir, &driver);
    fs::remove_dir_all(dir).unwrap();
    // One line per value: UTF-8 hex, or "null" for no change.
    let expected: String = expected
        .iter()
        .map(|s| {
            if s == NONE {
                "null\n".to_owned()
            } else {
                s.bytes().map(|b| format!("{b:02x}")).collect::<String>() + "\n"
            }
        })
        .collect();
    assert_eq!(got, expected);
}
