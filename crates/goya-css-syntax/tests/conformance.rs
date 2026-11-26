//! css-parsing-tests (Simon Sapin, CC0), commit
//! 203ce36bffd617db7f118c551e32794561fb273d, vendored in
//! `tests/css-parsing-tests/`. Every case in each file must pass.

use goya_css_syntax::*;
use serde_json::{json, Value};

fn numeric(kind: &str, n: &Numeric) -> Vec<Value> {
    vec![
        json!(kind),
        json!(n.repr),
        json!(n.value),
        json!(if n.integer { "integer" } else { "number" }),
    ]
}

fn token(t: &Token) -> Value {
    match t {
        Token::Ident(v) => json!(["ident", v]),
        Token::Function(v) => json!(["function", v]),
        Token::AtKeyword(v) => json!(["at-keyword", v]),
        Token::Hash { value, id } => {
            json!(["hash", value, if *id { "id" } else { "unrestricted" }])
        }
        Token::String { value, .. } => json!(["string", value]),
        Token::BadString => json!(["error", "bad-string"]),
        Token::Url { value, .. } => json!(["url", value]),
        Token::BadUrl => json!(["error", "bad-url"]),
        Token::Delim(c) => json!(c.to_string()),
        Token::Number(n) => Value::Array(numeric("number", n)),
        Token::Percentage(n) => Value::Array(numeric("percentage", n)),
        Token::Dimension { number, unit } => {
            let mut v = numeric("dimension", number);
            v.push(json!(unit));
            Value::Array(v)
        }
        Token::UnicodeRange { start, end } => json!(["unicode-range", start, end]),
        Token::IncludeMatch => json!("~="),
        Token::DashMatch => json!("|="),
        Token::PrefixMatch => json!("^="),
        Token::SuffixMatch => json!("$="),
        Token::SubstringMatch => json!("*="),
        Token::Column => json!("||"),
        Token::Whitespace => json!(" "),
        Token::Cdo => json!("<!--"),
        Token::Cdc => json!("-->"),
        Token::Colon => json!(":"),
        Token::Semicolon => json!(";"),
        Token::Comma => json!(","),
        Token::RightSquare => json!(["error", "]"]),
        Token::RightParen => json!(["error", ")"]),
        Token::RightCurly => json!(["error", "}"]),
        Token::LeftSquare | Token::LeftParen | Token::LeftCurly => {
            unreachable!("opening tokens always become blocks")
        }
    }
}

/// An unterminated string or URL is followed by an error marker in the
/// suite's representation.
fn eof_marker(cv: &ComponentValue) -> Option<Value> {
    match cv {
        ComponentValue::Token(
            Token::String {
                terminated: false, ..
            },
            _,
        ) => Some(json!(["error", "eof-in-string"])),
        ComponentValue::Token(
            Token::Url {
                terminated: false, ..
            },
            _,
        ) => Some(json!(["error", "eof-in-url"])),
        _ => None,
    }
}

fn list(items: &[ComponentValue]) -> Vec<Value> {
    let mut out = Vec::new();
    for cv in items {
        out.push(component(cv));
        out.extend(eof_marker(cv));
    }
    out
}

fn values(items: &[ComponentValue]) -> Value {
    Value::Array(list(items))
}

fn component(cv: &ComponentValue) -> Value {
    match cv {
        ComponentValue::Token(t, _) => token(t),
        ComponentValue::Block(b) => {
            let mut v = vec![json!(match b.open {
                '{' => "{}",
                '[' => "[]",
                _ => "()",
            })];
            v.extend(list(&b.contents));
            Value::Array(v)
        }
        ComponentValue::Function {
            name, arguments, ..
        } => {
            let mut v = vec![json!("function"), json!(name)];
            v.extend(list(arguments));
            Value::Array(v)
        }
    }
}

fn at_rule(r: &AtRule) -> Value {
    json!([
        "at-rule",
        r.name,
        values(&r.prelude),
        r.block.as_ref().map(|b| values(&b.contents))
    ])
}

fn rule(r: &Rule) -> Value {
    match r {
        Rule::Qualified(q) => json!([
            "qualified rule",
            values(&q.prelude),
            values(&q.block.contents)
        ]),
        Rule::At(a) => at_rule(a),
        Rule::Invalid { .. } => json!(["error", "invalid"]),
    }
}

fn declaration(d: &Declaration) -> Value {
    json!(["declaration", d.name, values(&d.value), d.important])
}

fn declaration_item(d: &DeclarationItem) -> Value {
    match d {
        DeclarationItem::Declaration(d) => declaration(d),
        DeclarationItem::At(a) => at_rule(a),
        DeclarationItem::Invalid { .. } => json!(["error", "invalid"]),
    }
}

fn single<T>(r: Result<T, SingleError>, f: impl Fn(&T) -> Value) -> Value {
    match r {
        Ok(v) => f(&v),
        Err(SingleError::Empty) => json!(["error", "empty"]),
        Err(SingleError::Invalid) => json!(["error", "invalid"]),
        Err(SingleError::ExtraInput) => json!(["error", "extra-input"]),
    }
}

/// JSON equality where numbers compare by value (42 equals 42.0).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            x == y || (x - y).abs() <= 1e-9 * x.abs().max(y.abs())
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        _ => a == b,
    }
}

fn run(file: &str, parse: impl Fn(&str) -> Value) {
    let path = format!(
        "{}/tests/css-parsing-tests/{file}",
        env!("CARGO_MANIFEST_DIR")
    );
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for pair in cases.chunks(2) {
        let input = pair[0].as_str().unwrap();
        let actual = parse(input);
        if !same(&actual, &pair[1]) {
            failures.push(format!(
                "input {input:?}\n  expected {}\n  actual   {}",
                pair[1], actual
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{file}: {} of {} cases failed\n{}",
        failures.len(),
        cases.len() / 2,
        failures.join("\n")
    );
}

#[test]
fn component_value_list() {
    run("component_value_list.json", |s| {
        values(&parse_component_values(s))
    });
}

#[test]
fn one_component_value() {
    run("one_component_value.json", |s| {
        single(parse_one_component_value(s), component)
    });
}

#[test]
fn declaration_list() {
    run("declaration_list.json", |s| {
        Value::Array(
            parse_declaration_list(s)
                .iter()
                .map(declaration_item)
                .collect(),
        )
    });
}

#[test]
fn one_declaration() {
    run("one_declaration.json", |s| {
        single(parse_one_declaration(s), declaration)
    });
}

#[test]
fn one_rule() {
    run("one_rule.json", |s| single(parse_one_rule(s), rule));
}

#[test]
fn rule_list() {
    run("rule_list.json", |s| {
        Value::Array(parse_rule_list(s).iter().map(rule).collect())
    });
}

#[test]
fn stylesheet() {
    run("stylesheet.json", |s| {
        Value::Array(parse_stylesheet(s).iter().map(rule).collect())
    });
}
