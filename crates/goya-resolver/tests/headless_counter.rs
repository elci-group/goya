//! Headless conformance for GY-RESOLVER/0.1.
//!
//! These tests execute admitted scenes. They do not measure startup, memory,
//! or frames, and they do not claim parity with a launched window.

use std::collections::BTreeMap;
use std::process::Command;

use goya_executable::D1Value;
use goya_lang::{InteractionValue, RectMp, StateLifetime};
use goya_resolver::{
    Event, Exhaustion, PerformanceProfile, Resolver, ResourcePolicy, SemanticContract,
};
use goya_sdk::D1Application;

const LIVE: &str = include_str!("../../sash/fixtures/interactive-live.gscene");
const GSSH1: &str = include_str!("../../goya-lang/tests/corpus/accept/gssh1-counter.gscene");
const GSSH2: &str = include_str!("../../goya-lang/tests/corpus/accept/gssh2-counter.gscene");
const DISPLAY: &str = include_str!("../../../tests/maja/fixtures/display-counter.gscene");

fn resolver() -> Resolver {
    Resolver::builder()
        .viewport(320.0, 160.0)
        .build()
        .expect("viewport")
}

fn running(source: &str) -> goya_resolver::Instance {
    let resolver = resolver();
    let application = resolver.load(source).expect("load");
    let mut instance = resolver.instantiate(&application).expect("instance");
    instance.start().expect("start");
    instance
}

#[test]
fn live_counter_matches_the_shared_d1_owner_and_projects_one_display() {
    let resolver = resolver();
    let application = resolver.load(LIVE).expect("live scene admits");
    assert_eq!(application.semantic_contract(), "GSSH/4-live");
    assert!(!application.ir_lowered());
    assert_eq!(application.dependents("count"), ["window.total"]);
    assert!(application.dependents("label").is_empty());
    let components = application.components();
    assert!(components.len() > 2);
    assert!(components
        .iter()
        .any(|component| component.semantic_id == Some("window.add")));

    let mut shared = D1Application::new(
        application.scene(),
        BTreeMap::from([("count".into(), D1Value::Int(0))]),
    )
    .expect("shared owner");
    let mut instance = resolver.instantiate(&application).expect("instance");
    assert_eq!(instance.phase().as_str(), "initialising");
    assert_eq!(instance.revision(), 0);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(0));
    assert_eq!(instance.state()[0].lifetime, StateLifetime::Session);
    instance.start().unwrap();

    let first = instance
        .dispatch(&Event::Activate {
            action: "action.add".into(),
        })
        .unwrap();
    shared
        .dispatch(
            "action.add",
            BTreeMap::from([("amount".into(), D1Value::Int(1))]),
        )
        .unwrap();
    assert_eq!(first.revision, shared.revision());
    assert_eq!(first.revision, 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(1));
    assert_eq!(shared.values()["count"], D1Value::Int(1));
    assert_eq!(
        first.announcement.as_deref(),
        Some("count increased by 1, from 0 to 1")
    );
    assert_eq!(first.affected_elements, ["window.total"]);
    assert_eq!(instance.displays()[0].text, "1");

    instance.set_field("window.amount", "0").unwrap();
    let same = instance.activate("button.add.activate").unwrap();
    assert_eq!(same.revision, 2);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(1));
    assert!(same.announcement.is_none());
    assert!(same.affected_elements.is_empty());

    let rect = instance.frame().find("amount").expect("amount").rect;
    assert_eq!(
        rect,
        RectMp {
            x: 20_000,
            y: 60_000,
            width: 120_000,
            height: 30_000,
        }
    );
    assert_eq!(instance.frame().find("amount").unwrap().rect, rect);
}

#[test]
fn a_bad_field_and_an_overflow_leave_the_authored_snapshot() {
    let mut instance = running(LIVE);
    instance.set_field("window.amount", "01").unwrap();
    let error = instance.activate("action.add").unwrap_err();
    assert_eq!(error.code, "GY-RESOLVER-FIELD");
    assert_eq!(instance.revision(), 0);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(0));

    instance
        .set_field("window.amount", "9223372036854775807")
        .unwrap();
    instance.activate("action.add").unwrap();
    let error = instance.activate("action.add").unwrap_err();
    assert_eq!(error.code, "GY-RESOLVER-ACTION");
    assert_eq!(instance.revision(), 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(i64::MAX));
    assert_eq!(instance.phase().as_str(), "running");
}

#[test]
fn display_dependencies_follow_the_written_key() {
    let mut instance = running(DISPLAY);
    assert_eq!(instance.semantic_contract(), "GSSH/4-live");
    assert!(instance.ir_lowered());
    assert_eq!(
        resolver().load(DISPLAY).unwrap().dependents("count"),
        ["total"]
    );
    assert_eq!(
        resolver().load(DISPLAY).unwrap().dependents("flag"),
        ["raise"]
    );
    instance.set_field("amount", "1").unwrap();
    let added = instance.activate("action.add").unwrap();
    assert_eq!(added.affected_elements, ["total"]);
    assert_eq!(
        added.announcement.as_deref(),
        Some("Count increased by 1, from 0 to 1")
    );
    let raised = instance.activate("action.raise").unwrap();
    assert_eq!(raised.affected_elements, ["raise"]);
    assert_eq!(
        raised.announcement.as_deref(),
        Some("Raised changed from false to true")
    );
    let count = instance
        .state()
        .into_iter()
        .find(|entry| entry.key == "count")
        .unwrap();
    assert_eq!(count.lifetime, StateLifetime::Durable);
    assert_eq!(count.value, InteractionValue::Int(1));
}

#[test]
fn gssh2_header_selects_that_contract_and_lowers() {
    let application = resolver().load(GSSH2).unwrap();
    assert_eq!(application.semantic_contract(), "GSSH/2");
    assert!(application.ir_lowered());
    let mut instance = resolver().instantiate(&application).unwrap();
    instance.start().unwrap();
    instance.activate("add").unwrap();
    assert_eq!(instance.revision(), 1);
    assert_eq!(instance.state()[0].value, InteractionValue::Int(1));
}

#[test]
fn instances_do_not_share_state() {
    let resolver = resolver();
    let application = resolver.load(LIVE).unwrap();
    let mut first = resolver.instantiate(&application).unwrap();
    let second = resolver.instantiate(&application).unwrap();
    first.start().unwrap();
    first.activate("action.add").unwrap();
    assert_eq!(first.revision(), 1);
    assert_eq!(second.revision(), 0);
}

#[test]
fn duplicate_structural_names_share_one_interned_string() {
    let source =
        r#"@ontology a { @cell root { id: "r"; @cell x { id: "a"; } @cell x { id: "b"; } } }"#;
    let application = resolver().load(source).unwrap();
    let names: Vec<_> = application
        .components()
        .into_iter()
        .filter(|component| component.name == "x")
        .map(|component| component.name)
        .collect();
    assert_eq!(names.len(), 2);
    assert!(std::ptr::eq(names[0], names[1]));
}

#[test]
fn d2_is_admitted_and_refused_before_execution() {
    let source = r#"@ontology app { @button go { id: "button.go" } @interaction fetch { id: "action.fetch"; trigger: button.go.activate; class: D2; effects { network; } requests { lookup: effect network capability "network" request "static request" result int64; } state { write count: int; } input { } set count = effect.lookup; } }"#;
    let application = resolver().load(source).expect("D2 compiles");
    let refusal = application.execution_refusal().expect("refused");
    assert!(refusal.contains("action.fetch"));
    match resolver().instantiate(&application) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-D2"),
        Ok(_) => panic!("D2 instance was created"),
    }
}

#[test]
fn declaration_failures_keep_the_authored_range() {
    let mut instance = running(LIVE);
    instance.set_field("window.amount", "nope").unwrap();
    let error = instance.activate("action.add").unwrap_err();
    assert_eq!(error.code, "GY-RESOLVER-FIELD");
    let range = error.range.expect("binding range");
    let quoted = &LIVE[range.start..range.end];
    assert!(quoted.contains("from \"window.amount\""), "{quoted}");
    assert!(error.to_string().contains("(bytes "), "{error}");
    assert_eq!(instance.revision(), 0);

    match resolver().load(GSSH1) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-UNBOUND");
            let range = error.range.expect("input range");
            let quoted = &GSSH1[range.start..range.end];
            assert!(quoted.contains("amount: int"), "{quoted}");
        }
        Ok(_) => panic!("unbound input was prepared"),
    }

    let missing = r#"// profile: gssh1
@ontology app {
  @cell root {
    id: "root";
    extent: (80, 40, 0);
    @button add { id: "add"; text: "Add"; extent: (20, 8, 0); }
  }
  @interaction add {
    id: "action.add";
    trigger: button.add.activate;
    class: D1;
    state { read count: int; write count: int; }
    input { }
    set count = state.count + 1;
  }
}
"#;
    match resolver().load(missing) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-INITIAL", "{error}");
            let range = error.range.expect("state use range");
            let quoted = &missing[range.start..range.end];
            assert!(quoted.contains("read count: int"), "{quoted}");
        }
        Ok(_) => panic!("a state key without an initial was prepared"),
    }
}

#[test]
fn platform_exceptions_fail_closed() {
    let source =
        r#"@ontology app { @cell root { id: "root"; } @platform android { "media": cede; } }"#;
    match resolver().load(source) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-PLATFORM");
            assert!(error.range.is_some());
        }
        Ok(_) => panic!("platform exception was executed"),
    }
}

#[test]
fn an_ambiguous_profile_comment_is_not_guessed() {
    let source = "// profile: both\n@ontology app { @cell root { id: \"root\"; } }";
    match resolver().load(source) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-PROFILE"),
        Ok(_) => panic!("ambiguous profile was admitted"),
    }
}

#[test]
fn budgets_stop_before_the_next_action_and_a_bad_field_does_not_consume_one() {
    let mut policy = ResourcePolicy {
        max_dispatches: 1,
        ..ResourcePolicy::default()
    };
    let resolver = Resolver::builder()
        .viewport(320.0, 160.0)
        .resource_policy(policy.clone())
        .build()
        .unwrap();
    let mut instance = resolver.instantiate(&resolver.load(LIVE).unwrap()).unwrap();
    instance.start().unwrap();
    let error = instance.activate("missing").unwrap_err();
    assert_eq!(error.code, "GY-RESOLVER-ACTION");
    assert!(error.range.is_none());
    instance.set_field("window.amount", "nope").unwrap();
    assert_eq!(
        instance.activate("action.add").unwrap_err().code,
        "GY-RESOLVER-FIELD"
    );
    instance.set_field("window.amount", "1").unwrap();
    assert_eq!(instance.activate("action.add").unwrap().revision, 1);
    let error = instance.activate("action.add").unwrap_err();
    assert_eq!(error.code, "GY-RESOLVER-BUDGET");
    assert_eq!(instance.phase().as_str(), "running");
    assert_eq!(instance.revision(), 1);

    policy.on_dispatch_exhaustion = Exhaustion::Fail;
    let resolver = Resolver::builder()
        .viewport(320.0, 160.0)
        .resource_policy(policy)
        .build()
        .unwrap();
    let mut failed = resolver.instantiate(&resolver.load(LIVE).unwrap()).unwrap();
    failed.start().unwrap();
    failed.activate("action.add").unwrap();
    assert_eq!(
        failed.activate("action.add").unwrap_err().code,
        "GY-RESOLVER-BUDGET"
    );
    assert_eq!(failed.phase().as_str(), "failed");
    assert_eq!(failed.revision(), 1);
    assert!(failed.resume().is_err());
    failed.terminate().unwrap();
    assert_eq!(failed.phase().as_str(), "terminated");
}

#[test]
fn suspend_keeps_state_and_blocks_dispatch() {
    let mut instance = running(LIVE);
    instance.activate("action.add").unwrap();
    instance.suspend().unwrap();
    assert!(instance.activate("action.add").is_err());
    assert_eq!(instance.revision(), 1);
    instance.resume().unwrap();
    instance.activate("action.add").unwrap();
    assert_eq!(instance.state()[0].value, InteractionValue::Int(2));
}

#[test]
fn explicit_contract_must_agree_with_the_source_comment() {
    let resolver = Resolver::builder()
        .contract(SemanticContract::Gssh4Live)
        .build()
        .unwrap();
    match resolver.load(GSSH2) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-PROFILE"),
        Ok(_) => panic!("conflicting contract was admitted"),
    }
}

#[test]
fn source_limit_is_enforced_before_admission() {
    let policy = ResourcePolicy {
        max_source_bytes: 8,
        ..ResourcePolicy::default()
    };
    let resolver = Resolver::builder().resource_policy(policy).build().unwrap();
    match resolver.load(LIVE) {
        Err(error) => assert_eq!(error.code, "GY-RESOLVER-BUDGET"),
        Ok(_) => panic!("oversized source was admitted"),
    }
}

#[test]
fn performance_profile_is_a_label() {
    assert_eq!(PerformanceProfile::Constrained.as_str(), "constrained");
    assert_eq!(
        PerformanceProfile::parse("standard"),
        Some(PerformanceProfile::Standard)
    );
}

#[test]
fn cli_trace_reports_sites_and_layout_without_a_timer() {
    let bin = env!("CARGO_BIN_EXE_goya-resolve");
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sash/fixtures/interactive-live.gscene"
    );
    let traced = Command::new(bin)
        .args([
            "--trace",
            "--viewport",
            "320x160",
            "--dispatch",
            "action.add",
            "--set",
            "window.amount=0",
            "--dispatch",
            "action.add",
            scene,
        ])
        .output()
        .expect("trace");
    let stdout = String::from_utf8(traced.stdout).unwrap();
    assert!(
        traced.status.success(),
        "{}",
        String::from_utf8_lossy(&traced.stderr)
    );
    assert!(
        stdout.contains("trace dispatch action.add sites 1 layout 0"),
        "{stdout}"
    );
    assert!(
        stdout.contains("trace dispatch action.add sites 0 layout 0"),
        "{stdout}"
    );
    assert!(stdout.contains("revision: 2"), "{stdout}");
    assert!(!stdout.contains(" ms"), "{stdout}");
    assert!(!stdout.contains("fps"), "{stdout}");

    let presented = Command::new(bin)
        .args([
            "--trace",
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--dispatch",
            "action.add",
            "--present",
            scene,
        ])
        .output()
        .expect("present");
    let stdout = String::from_utf8(presented.stdout).unwrap();
    assert!(presented.status.success(), "{stdout}");
    let dispatch_at = stdout
        .find("trace dispatch action.add sites 1 layout 0")
        .expect("dispatch trace");
    let present_at = stdout
        .find("trace present closed nodes 4 layout 1")
        .expect("present trace");
    assert!(dispatch_at < present_at, "{stdout}");
    assert!(!stdout.contains("opened"), "{stdout}");

    let pointed = Command::new(bin)
        .args([
            "--trace",
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--pointer",
            "100000,125000",
            "--pointer",
            "200000,75000",
            scene,
        ])
        .output()
        .expect("point");
    let stdout = String::from_utf8(pointed.stdout).unwrap();
    assert!(
        pointed.status.success(),
        "{}",
        String::from_utf8_lossy(&pointed.stderr)
    );
    assert!(stdout.contains("trace point miss layout 1"), "{stdout}");
    assert!(
        stdout.contains("trace point add sites 1 layout 1"),
        "{stdout}"
    );

    let validated = Command::new(bin)
        .args(["--trace", "--validate-only", scene])
        .output()
        .expect("validate");
    let stdout = String::from_utf8(validated.stdout).unwrap();
    assert!(validated.status.success(), "{stdout}");
    assert!(
        stdout.contains("trace validate actions 1 execution ready"),
        "{stdout}"
    );
}

#[test]
fn cli_suspend_keeps_the_revision_and_blocks_dispatch() {
    let bin = env!("CARGO_BIN_EXE_goya-resolve");
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sash/fixtures/interactive-live.gscene"
    );
    let resumed = Command::new(bin)
        .args([
            "--viewport",
            "320x160",
            "--dispatch",
            "action.add",
            "--suspend",
            "--resume",
            "--dispatch",
            "action.add",
            scene,
        ])
        .output()
        .expect("resume");
    let stdout = String::from_utf8(resumed.stdout).unwrap();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert!(stdout.contains("suspend suspended"), "{stdout}");
    assert!(stdout.contains("resume running"), "{stdout}");
    assert!(stdout.contains("phase: running"), "{stdout}");
    assert!(stdout.contains("revision: 2"), "{stdout}");
    assert!(stdout.contains("state count int 2 session"), "{stdout}");
    assert!(!stdout.contains("trace "), "{stdout}");

    let blocked = Command::new(bin)
        .args([
            "--viewport",
            "320x160",
            "--dispatch",
            "action.add",
            "--suspend",
            "--dispatch",
            "action.add",
            scene,
        ])
        .output()
        .expect("blocked");
    assert_eq!(blocked.status.code(), Some(1));
    let stdout = String::from_utf8(blocked.stdout).unwrap();
    let stderr = String::from_utf8(blocked.stderr).unwrap();
    assert!(stdout.contains("suspend suspended"), "{stdout}");
    assert!(
        stdout.contains("dispatch action.add revision 1"),
        "{stdout}"
    );
    assert!(!stdout.contains("revision 2"), "{stdout}");
    assert!(stderr.contains("GY-RESOLVER-PHASE"), "{stderr}");
    assert!(
        stderr.contains("cannot dispatch while suspended"),
        "{stderr}"
    );

    let presented = Command::new(bin)
        .args([
            "--trace",
            "--target",
            "linux",
            "--viewport",
            "320x160",
            "--suspend",
            "--present",
            scene,
        ])
        .output()
        .expect("present");
    let stdout = String::from_utf8(presented.stdout).unwrap();
    assert!(
        presented.status.success(),
        "{}",
        String::from_utf8_lossy(&presented.stderr)
    );
    let suspend_at = stdout
        .find("trace suspend layout 0")
        .expect("suspend trace");
    let present_at = stdout
        .find("trace present closed nodes 4 layout 1")
        .expect("present trace");
    assert!(suspend_at < present_at, "{stdout}");
    assert!(stdout.contains("phase: suspended"), "{stdout}");
    assert!(stdout.contains("revision: 0"), "{stdout}");
    assert!(
        stdout.contains("present linux wayland closed revision 0"),
        "{stdout}"
    );
    assert!(!stdout.contains("opened"), "{stdout}");
    assert!(!stdout.contains(" ms"), "{stdout}");
    assert!(!stdout.contains("fps"), "{stdout}");

    let early = Command::new(bin)
        .args(["--viewport", "320x160", "--resume", scene])
        .output()
        .expect("early resume");
    assert_eq!(early.status.code(), Some(1));
    let stderr = String::from_utf8(early.stderr).unwrap();
    assert!(stderr.contains("cannot resume while running"), "{stderr}");

    let twice = Command::new(bin)
        .args(["--viewport", "320x160", "--suspend", "--suspend", scene])
        .output()
        .expect("double suspend");
    assert_eq!(twice.status.code(), Some(1));
    let stderr = String::from_utf8(twice.stderr).unwrap();
    assert!(
        stderr.contains("cannot suspend while suspended"),
        "{stderr}"
    );

    let validate = Command::new(bin)
        .args(["--validate-only", "--suspend", scene])
        .output()
        .expect("validate suspend");
    assert_eq!(validate.status.code(), Some(2));
    let stderr = String::from_utf8(validate.stderr).unwrap();
    assert!(
        stderr.contains("--validate-only does not suspend or resume"),
        "{stderr}"
    );
}

#[test]
fn cli_dispatches_the_live_counter_and_rejects_unimplemented_flags() {
    let bin = env!("CARGO_BIN_EXE_goya-resolve");
    let scene = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../sash/fixtures/interactive-live.gscene"
    );
    let ok = Command::new(bin)
        .args(["--viewport", "320x160", "--dispatch", "action.add", scene])
        .output()
        .expect("run");
    let stdout = String::from_utf8(ok.stdout).unwrap();
    assert!(
        ok.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(stdout.contains("revision: 1"), "{stdout}");
    assert!(stdout.contains("state count int 1 session"), "{stdout}");
    assert!(
        stdout.contains("announcement: count increased by 1, from 0 to 1"),
        "{stdout}"
    );
    assert!(stdout.contains("display window.total 1"), "{stdout}");
    assert!(stdout.contains("ir: unavailable"), "{stdout}");
    assert!(stdout.contains("persistence: classified-only"), "{stdout}");
    assert!(!stdout.contains("trace "), "{stdout}");

    let rejected = Command::new(bin)
        .args(["--benchmark", scene])
        .output()
        .expect("benchmark");
    assert_eq!(rejected.status.code(), Some(2));
    let stderr = String::from_utf8(rejected.stderr).unwrap();
    assert!(
        stderr.contains("is not implemented in GY-RESOLVER/0.1"),
        "{stderr}"
    );
    let watch = Command::new(bin)
        .args(["--validate-only", "--watch", scene])
        .output()
        .expect("watch");
    assert_eq!(watch.status.code(), Some(2));
    let stderr = String::from_utf8(watch.stderr).unwrap();
    assert!(
        stderr.contains("--validate-only does not watch"),
        "{stderr}"
    );
    let validate = Command::new(bin)
        .args(["--validate-only", "--profile", "constrained", scene])
        .output()
        .expect("validate");
    let report = String::from_utf8(validate.stdout).unwrap();
    assert!(validate.status.success(), "{report}");
    assert!(report.contains("contract: GSSH/4-live"), "{report}");
    assert!(report.contains("execution: ready"), "{report}");
    assert!(
        report.contains("performance: constrained recorded"),
        "{report}"
    );
}

#[test]
fn extended_live_constructs_are_refused_not_dropped() {
    let pairs = include_str!("../../../apps/pairs.gscene");
    match resolver().load(pairs) {
        Err(error) => {
            assert_eq!(error.code, "GY-RESOLVER-LIVE", "{error}");
            let range = error.range.expect("construct range");
            assert!(range.start < range.end && range.end <= pairs.len());
        }
        Ok(_) => panic!("a scene with live constructs the resolver does not execute was loaded"),
    }
}
