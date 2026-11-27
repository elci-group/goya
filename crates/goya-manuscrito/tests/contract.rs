//! MIR/1 contract conformance: the Rust types must accept exactly the shape
//! the trusted TypeScript frontend emits and reject version drift, malformed
//! digests, and inverted source ranges.

use goya_manuscrito::{MirNode, MirReport, MirStatus, SemanticKind, MIR_VERSION};
use serde_json::json;

fn frontend_node(kind: &str, start: u64, end: u64, is_async: Option<bool>) -> serde_json::Value {
    let mut node = json!({
        "id": format!("src/main.ts#{start}:{kind}"),
        "parent": serde_json::Value::Null,
        "function": serde_json::Value::Null,
        "kind": kind,
        "source": "src/main.ts",
        "start": start,
        "end": end,
        "source_digest": format!("sha256:{}", "a".repeat(64)),
        "verification_status": "unverified",
        "transformation_history": ["typescript-ast-to-mir-node"],
        "source_kind": "FunctionDeclaration",
        "inferred_type": "() => Promise<string>",
        "effects": ["filesystem.readFile"],
    });
    if let Some(async_flag) = is_async {
        node["async"] = json!(async_flag);
    }
    node
}

fn frontend_report(nodes: Vec<serde_json::Value>) -> serde_json::Value {
    json!({
        "version": MIR_VERSION,
        "status": "extraction-only",
        "nodes": nodes,
    })
}

#[test]
fn frontend_shape_round_trips_losslessly() {
    let value = frontend_report(vec![
        frontend_node("function", 0, 120, Some(true)),
        frontend_node("invocation", 40, 68, None),
        frontend_node("suspension", 30, 36, None),
    ]);
    let report: MirReport = serde_json::from_value(value.clone()).expect("frontend MIR parses");
    report.validate_version().expect("frontend MIR validates");

    assert_eq!(report.status, MirStatus::ExtractionOnly);
    assert_eq!(report.nodes.len(), 3);
    assert_eq!(report.nodes[0].kind, SemanticKind::Function);
    assert_eq!(report.nodes[0].is_async, Some(true));
    assert_eq!(report.nodes[1].kind, SemanticKind::Invocation);
    assert_eq!(report.nodes[1].is_async, None);
    assert_eq!(report.nodes[2].kind, SemanticKind::Suspension);
    assert_eq!(report.nodes[2].effects, vec!["filesystem.readFile"]);

    // Round-trip is lossless: omitted `async` stays omitted, kebab-case names
    // are preserved byte-for-byte.
    assert_eq!(serde_json::to_value(&report).expect("re-serialize"), value);
}

#[test]
fn kebab_case_names_are_stable() {
    assert_eq!(
        serde_json::to_value(MirStatus::ExtractionOnly).unwrap(),
        json!("extraction-only")
    );
    for (kind, name) in [
        (SemanticKind::Function, "function"),
        (SemanticKind::Branch, "branch"),
        (SemanticKind::Iteration, "iteration"),
        (SemanticKind::Suspension, "suspension"),
        (SemanticKind::Throw, "throw"),
        (SemanticKind::Return, "return"),
        (SemanticKind::Invocation, "invocation"),
        (SemanticKind::Mutation, "mutation"),
        (SemanticKind::Binding, "binding"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), json!(name));
    }
}

#[test]
fn version_drift_is_rejected() {
    let mut value = frontend_report(vec![frontend_node("function", 0, 1, None)]);
    value["version"] = json!("goya.manuscrito.mir/2");
    let report: MirReport = serde_json::from_value(value).expect("parses");
    let error = report.validate_version().expect_err("version drift fails");
    assert!(
        error.contains("unsupported Manuscrito MIR version"),
        "error: {error}"
    );
}

#[test]
fn inverted_source_range_is_rejected() {
    let value = frontend_report(vec![frontend_node("branch", 50, 10, None)]);
    let report: MirReport = serde_json::from_value(value).expect("parses");
    let error = report.validate_version().expect_err("inverted range fails");
    assert!(error.contains("invalid source range"), "error: {error}");
}

#[test]
fn malformed_digest_is_rejected() {
    for digest in ["", "sha256:", "sha512:abcd", "sha256:nothex"] {
        let mut value = frontend_report(vec![frontend_node("binding", 0, 1, None)]);
        value["nodes"][0]["source_digest"] = json!(digest);
        let report: MirReport = serde_json::from_value(value).expect("parses");
        let error = report.validate_version().expect_err("bad digest fails");
        assert!(
            error.contains("malformed source digest"),
            "digest {digest:?}: {error}"
        );
    }
}

#[test]
fn empty_id_and_source_are_rejected() {
    for field in ["id", "source"] {
        let mut value = frontend_report(vec![frontend_node("return", 0, 1, None)]);
        value["nodes"][0][field] = json!("");
        let report: MirReport = serde_json::from_value(value).expect("parses");
        let error = report.validate_version().expect_err("empty field fails");
        assert!(error.contains("MIR node"), "field {field}: {error}");
    }
}

#[test]
fn required_node_fields_are_not_optional() {
    let mut value = frontend_report(vec![frontend_node("throw", 0, 1, None)]);
    value["nodes"][0]
        .as_object_mut()
        .expect("node object")
        .remove("source_digest");
    let error = serde_json::from_value::<MirReport>(value)
        .expect_err("missing source_digest must be rejected");
    assert!(
        error.to_string().contains("source_digest"),
        "error: {error}"
    );
}

#[test]
fn standalone_node_round_trip_keeps_optional_async_absent() {
    let node: MirNode =
        serde_json::from_value(frontend_node("mutation", 4, 9, None)).expect("node parses");
    let serialized = serde_json::to_value(&node).expect("node serializes");
    assert!(serialized.get("async").is_none());
    assert_eq!(serialized["start"], json!(4));
    assert_eq!(serialized["end"], json!(9));
}
