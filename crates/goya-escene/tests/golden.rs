//! Golden-fixture conformance tests (§22): byte-exact source recovery,
//! deterministic reproduction, semantic validation, and JSON round-trip
//! through the persisted artefact form.

use goya_escene::evidence::{payload, Certainty, Evidence, EvidenceKind, Origin};
use goya_escene::model::{
    ImplementationKind, Caprichio, CaprichioStatus, ClassCoverage, Coverage, CoverageStatus,
    ExecutionModel, Status, Target, GenerationMode,
};
use goya_escene::rules::{CaprichioApplication, RuleRegistry};
use goya_escene::validate::{canonical_bytes, validate};
use goya_escene::{SourceSnapshot, Span};

/// Fixture source includes a BOM, CRLF endings, an original emoji and UTF-8
/// multi-byte content — the ESC-G01 torture cases.
const SOURCE: &[u8] = b"\xEF\xBB\xBFblock render_world {\r\n    resolve_geometry() \xF0\x9F\x8E\xAE\r\n    resolve_textures()\r\n}\r\n";

fn source() -> SourceSnapshot {
    SourceSnapshot::capture("src-fixture-1", "Doom.gscene", SOURCE.to_vec())
}

fn target() -> Target {
    Target {
        id: "arachnid".into(),
        platform: "webOS".into(),
        platform_version: Some("webOS TV 6.0".into()),
        architecture: Some("armv7".into()),
        toolchain: "goya-arachnid".into(),
        toolchain_version: "0.1.0".into(),
        execution_model: ExecutionModel::Compiled,
        configuration_digest: format!("sha256:{}", goya_escene::source::sha256_hex(b"release")),
    }
}

/// Byte offsets in SOURCE (BOM = 3 bytes, CRLF = 2):
///   block header: 3..25; resolve_geometry line: 25..54 (trailing emoji at
///   48..52); resolve_textures line: 54..78; closing brace: 78..81.
fn evidence_snapshot() -> goya_escene::EvidenceSnapshot {
    let mut snap = goya_escene::EvidenceSnapshot::default();
    snap.push(Evidence {
        id: "ev-0001".into(),
        producer: "goya-arachnid".into(),
        origin: Origin::Compiler,
        certainty: Certainty::Verified,
        kind: EvidenceKind::LoweringDecision,
        payload: payload(&[
            ("decision", serde_json::json!("native")),
            ("span_start_byte", serde_json::json!(3)),
            ("span_end_byte", serde_json::json!(25)),
        ]),
        window_start_ns: None,
        window_duration_ns: None,
        sample_count: None,
        parent_refs: vec![],
    });
    snap.push(Evidence {
        id: "ev-0002".into(),
        producer: "instrumented_allocator".into(),
        origin: Origin::Profiler,
        certainty: Certainty::Measured,
        kind: EvidenceKind::RuntimeMemory,
        payload: payload(&[
            ("resource", serde_json::json!("memory")),
            ("metric", serde_json::json!("peak_attributed_allocation")),
            ("observed_bytes", serde_json::json!(50_331_648u64)),
            ("budget_bytes", serde_json::json!(33_554_432u64)),
            ("span_start_byte", serde_json::json!(25)),
            ("span_end_byte", serde_json::json!(54)),
        ]),
        window_start_ns: Some("1000000000".into()),
        window_duration_ns: Some("10000000000".into()),
        sample_count: Some(120),
        parent_refs: vec![],
    });
    snap.push(Evidence {
        id: "ev-0003".into(),
        producer: "gvi-geometry".into(),
        origin: Origin::Caprichio,
        certainty: Certainty::Verified,
        kind: EvidenceKind::CaprichioApplication,
        payload: payload(&[
            ("caprichio_ref", serde_json::json!("cap-0001")),
            ("status", serde_json::json!("applied")),
            ("category", serde_json::json!("computational_reuse")),
        ]),
        window_start_ns: None,
        window_duration_ns: None,
        sample_count: None,
        parent_refs: vec![],
    });
    snap
}

fn applications() -> Vec<CaprichioApplication> {
    vec![CaprichioApplication {
        id: "cap-0001".into(),
        canonical_name: "geometry-reuse".into(),
        version: "0.2.0".into(),
        configuration_digest: format!("sha256:{}", goya_escene::source::sha256_hex(b"inventory")),
        status: CaprichioStatus::Applied,
        reason_codes: vec!["FINITE_STATE_SPACE".into(), "TARGET_MEMORY_BUDGET_SATISFIED".into()],
        implementation_refs: vec!["impl-0002".into()],
        span: Some((25, 54)),
    }]
}

fn caprichios() -> Vec<Caprichio> {
    vec![Caprichio {
        id: "cap-0001".into(),
        canonical_name: "geometry-reuse".into(),
        version: "0.2.0".into(),
        configuration_digest: format!("sha256:{}", goya_escene::source::sha256_hex(b"inventory")),
        status: CaprichioStatus::Applied,
        reason_codes: vec!["FINITE_STATE_SPACE".into(), "TARGET_MEMORY_BUDGET_SATISFIED".into()],
        implementation_refs: vec!["impl-0002".into()],
        evidence_refs: vec!["ev-0003".into()],
    }]
}

fn implementations() -> Vec<goya_escene::Implementation> {
    vec![
        goya_escene::Implementation {
            id: "impl-0001".into(),
            kind: goya_escene::model::ImplementationKind::Native,
            producer: "goya-arachnid".into(),
            description_code: "DESC.NATIVE_BLOCK".into(),
            source_spans: vec![Span::new(&source(), 3, 25, goya_escene::Precision::Exact).unwrap()],
            generated_symbols: vec!["render_world__native".into()],
        },
        goya_escene::Implementation {
            id: "impl-0002".into(),
            kind: goya_escene::model::ImplementationKind::Precomputed,
            producer: "gvi-geometry".into(),
            description_code: "DESC.GVI_INVENTORY".into(),
            source_spans: vec![Span::new(&source(), 25, 54, goya_escene::Precision::Exact).unwrap()],
            generated_symbols: vec!["geometry_inventory".into()],
        },
    ]
}

fn generate() -> goya_escene::Artefact {
    goya_escene::build::Builder::new(source(), target())
        .finish(
            &evidence_snapshot(),
            &applications(),
            implementations(),
            caprichios(),
            evidence_snapshot().records.clone(),
            Coverage {
                c: ClassCoverage { status: CoverageStatus::Complete, limitations: vec![], dropped_events: 0 },
                // Runtime evidence came from a bounded profile window only.
                i: ClassCoverage {
                    status: CoverageStatus::Partial,
                    limitations: vec!["single 10 s observation window".into()],
                    dropped_events: 0,
                },
                s: ClassCoverage { status: CoverageStatus::Complete, limitations: vec![], dropped_events: 0 },
            },
            &RuleRegistry::v1(),
        )
        .expect("golden fixture must validate")
}

#[test]
fn esc_g01_source_recovery_is_byte_exact() {
    let artefact = generate();
    assert_eq!(artefact.source.recover().unwrap(), SOURCE);
}

#[test]
fn esc_g02_identical_inputs_produce_byte_identical_output() {
    let a = canonical_bytes(&generate()).unwrap();
    let b = canonical_bytes(&generate()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn esc_g03_all_annotation_references_resolve() {
    let artefact = generate();
    assert!(validate(&artefact, &RuleRegistry::v1()).is_ok());
    for ann in &artefact.annotations {
        assert!(!ann.evidence_refs.is_empty());
        for r in &ann.evidence_refs {
            assert!(artefact.evidence.iter().any(|e| e.id == *r));
        }
    }
}

#[test]
fn esc_g06_caprichio_provenance_captured() {
    let artefact = generate();
    let reuse = artefact
        .annotations
        .iter()
        .find(|a| a.code.0 == "S.COMPUTATIONAL_REUSE")
        .expect("applied reuse substitution must be annotated");
    assert_eq!(reuse.emoji, "♻️");
    let cap = &artefact.caprichios[0];
    assert_eq!(cap.canonical_name, "geometry-reuse");
    assert_eq!(cap.status, CaprichioStatus::Applied);
    assert_eq!(cap.version, "0.2.0");
}

#[test]
fn esc_g07_unobserved_construct_is_disclosed_not_invented() {
    // resolve_textures (bytes 49..72) has no evidence: no annotation may claim
    // behaviour for it.
    let artefact = generate();
    for ann in &artefact.annotations {
        if let Some(span) = &ann.span {
            assert!(
                !(span.start_byte >= 54 && span.end_byte <= 78),
                "no annotation may claim evidence for the unobserved construct"
            );
        }
    }
}

#[test]
fn json_roundtrip_preserves_validation() {
    let artefact = generate();
    let json = serde_json::to_vec(&artefact).unwrap();
    let parsed: goya_escene::Artefact = serde_json::from_slice(&json).unwrap();
    assert_eq!(canonical_bytes(&parsed).unwrap(), canonical_bytes(&artefact).unwrap());
    assert!(validate(&parsed, &RuleRegistry::v1()).is_ok());
}

#[test]
fn projection_carries_all_three_classes() {
    let artefact = generate();
    assert!(artefact.projection.content.contains("⚙️[C:"));
    assert!(artefact.projection.content.contains("🐘[I:"));
    assert!(artefact.projection.content.contains("♻️[S:"));
}

#[test]
fn generation_mode_and_status_flow_through() {
    let artefact = generate();
    assert_eq!(artefact.header.status, Status::Complete);
    assert_eq!(artefact.header.generation_mode, GenerationMode::Compilation);
}

/// ESC-0 golden fixture: the committed artefact at
/// `fixtures/escene/Doom-Arachnid.escene` must remain byte-identical to
/// deterministic regeneration. Regenerate with
/// `GOYA_ESCENE_WRITE_FIXTURE=1 cargo test -p goya-escene --test golden`.
#[test]
fn committed_golden_fixture_matches_regeneration() {
    let artefact = generate();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/escene/Doom-Arachnid.escene");
    if std::env::var("GOYA_ESCENE_WRITE_FIXTURE").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, canonical_bytes(&artefact).unwrap()).unwrap();
        eprintln!("wrote fixture {}", path.display());
        return;
    }
    let committed = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("golden fixture missing at {}: {e}", path.display()));
    assert_eq!(committed, canonical_bytes(&artefact).unwrap());
}
