//! html5lib-tests tokenizer suite (MIT), commit
//! c5495c539ca8a79c3c5107f5f14010bcff9a35d5, vendored in
//! `tests/html5lib-tokenizer/`. Every test in every `tests` list runs once
//! per initial state, and its tokens must match exactly.
//!
//! Tests whose input contains a lone surrogate (only reachable through
//! `doubleEscaped`) are skipped and counted: Rust strings cannot hold them,
//! and a UTF-8 document cannot contain them. Parse-error codes and positions
//! are not compared.

use goya_html_tokenizer::{InitialState, Token, Tokenizer};
use serde_json::{json, Map, Value};

/// Undoes the suite's second level of `\uHHHH` escaping. `None` if the result
/// would contain a lone surrogate.
fn unescape(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut units: Vec<u16> = Vec::new();
    let mut chars = s.chars().peekable();
    let flush = |units: &mut Vec<u16>, out: &mut String| -> Option<()> {
        if !units.is_empty() {
            out.push_str(&String::from_utf16(units).ok()?);
            units.clear();
        }
        Some(())
    };
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'u') {
            chars.next();
            let hex: String = chars.by_ref().take(4).collect();
            units.push(u16::from_str_radix(&hex, 16).ok()?);
        } else {
            flush(&mut units, &mut out)?;
            out.push(c);
        }
    }
    flush(&mut units, &mut out)?;
    Some(out)
}

fn unescape_value(v: &Value) -> Option<Value> {
    Some(match v {
        Value::String(s) => Value::String(unescape(s)?),
        Value::Array(a) => Value::Array(a.iter().map(unescape_value).collect::<Option<_>>()?),
        Value::Object(o) => {
            let mut m = Map::new();
            for (k, v) in o {
                m.insert(unescape(k)?, unescape_value(v)?);
            }
            Value::Object(m)
        }
        other => other.clone(),
    })
}

fn state(name: &str) -> InitialState {
    match name {
        "Data state" => InitialState::Data,
        "PLAINTEXT state" => InitialState::Plaintext,
        "RCDATA state" => InitialState::Rcdata,
        "RAWTEXT state" => InitialState::Rawtext,
        "Script data state" => InitialState::ScriptData,
        "CDATA section state" => InitialState::CdataSection,
        other => panic!("unknown initial state {other}"),
    }
}

fn run(input: &str, initial: InitialState, last_start_tag: Option<&str>) -> Value {
    let mut t = Tokenizer::new(input);
    t.set_state(initial);
    t.set_last_start_tag(last_start_tag);
    let mut out = Vec::new();
    while let Some(s) = t.next_token() {
        out.push(match s.token {
            Token::Doctype {
                name,
                public_id,
                system_id,
                force_quirks,
            } => {
                json!(["DOCTYPE", name, public_id, system_id, !force_quirks])
            }
            Token::StartTag {
                name,
                attributes,
                self_closing,
            } => {
                let attrs: Map<String, Value> = attributes
                    .into_iter()
                    .map(|a| (a.name, Value::String(a.value)))
                    .collect();
                if self_closing {
                    json!(["StartTag", name, attrs, true])
                } else {
                    json!(["StartTag", name, attrs])
                }
            }
            Token::EndTag { name, .. } => json!(["EndTag", name]),
            Token::Comment(data) => json!(["Comment", data]),
            Token::ProcessingInstruction { target, data } => {
                json!(["ProcessingInstruction", target, data])
            }
            Token::Text(data) => json!(["Character", data]),
        });
    }
    Value::Array(out)
}

#[test]
fn html5lib_tokenizer_suite() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/html5lib-tokenizer");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "test"))
        .collect();
    files.sort();
    let (mut passed, mut skipped) = (0, 0);
    let mut failures = Vec::new();
    for path in &files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let Some(tests) = doc.get("tests").and_then(Value::as_array) else {
            continue;
        };
        for test in tests {
            let double = test.get("doubleEscaped").and_then(Value::as_bool) == Some(true);
            let raw_input = test["input"].as_str().unwrap();
            let (input, expected) = if double {
                match (unescape(raw_input), unescape_value(&test["output"])) {
                    (Some(i), Some(o)) => (i, o),
                    _ => {
                        skipped += 1;
                        continue;
                    }
                }
            } else {
                (raw_input.to_string(), test["output"].clone())
            };
            let states: Vec<&str> = match test.get("initialStates").and_then(Value::as_array) {
                Some(s) => s.iter().map(|v| v.as_str().unwrap()).collect(),
                None => vec!["Data state"],
            };
            let last = test.get("lastStartTag").and_then(Value::as_str);
            for s in states {
                let actual = run(&input, state(s), last);
                if actual == expected {
                    passed += 1;
                } else {
                    failures.push(format!(
                        "{} / {} [{s}]\n  input    {input:?}\n  expected {expected}\n  actual   {actual}",
                        path.file_name().unwrap().to_string_lossy(),
                        test["description"]
                    ));
                }
            }
        }
    }
    eprintln!(
        "html5lib tokenizer: {passed} passed, {} failed, {skipped} skipped (lone surrogates)",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} failed\n{}",
        failures.len(),
        failures
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn html5lib_error_codes() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/html5lib-tokenizer");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "test"))
        .collect();
    files.sort();
    let (mut passed, mut failures) = (0, Vec::new());
    for path in &files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let Some(tests) = doc.get("tests").and_then(Value::as_array) else {
            continue;
        };
        for test in tests {
            let raw = test["input"].as_str().unwrap();
            let double = test.get("doubleEscaped").and_then(Value::as_bool) == Some(true);
            let Some(input) = (if double {
                unescape(raw)
            } else {
                Some(raw.to_string())
            }) else {
                continue;
            };
            let mut expected: Vec<String> = test
                .get("errors")
                .and_then(Value::as_array)
                .map(|e| {
                    e.iter()
                        .map(|e| e["code"].as_str().unwrap().to_string())
                        .collect()
                })
                .unwrap_or_default();
            expected.sort();
            let states: Vec<&str> = match test.get("initialStates").and_then(Value::as_array) {
                Some(s) => s.iter().map(|v| v.as_str().unwrap()).collect(),
                None => vec!["Data state"],
            };
            for s in states {
                let mut t = Tokenizer::new(&input);
                t.set_state(state(s));
                t.set_last_start_tag(test.get("lastStartTag").and_then(Value::as_str));
                while t.next_token().is_some() {}
                let mut actual: Vec<String> =
                    t.errors().iter().map(|e| e.code.to_string()).collect();
                actual.sort();
                if actual == expected {
                    passed += 1;
                } else {
                    failures.push(format!(
                        "{:?} [{s}] expected {expected:?} actual {actual:?}",
                        input
                    ));
                }
            }
        }
    }
    eprintln!(
        "html5lib error codes: {passed} matched, {} differ",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
