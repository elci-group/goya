//! Probe status decisions and the CLI contract, without a live compositor.

use nebula::probe::{assemble, theme_summary, ProtocolOutcome, Status, SCOPE};
use nebula::protocols::{classify, Global, ProbeError};
use nebula::session::{classify as classify_session, identify_compositor_with, SessionEnv};
use std::process::Command;

fn cosmic_env() -> SessionEnv {
    SessionEnv {
        xdg_current_desktop: Some("COSMIC".into()),
        xdg_session_type: Some("wayland".into()),
        wayland_display: Some("wayland-1".into()),
        display: Some(":1".into()),
    }
}

fn required() -> Vec<Global> {
    ["wl_compositor", "wl_shm", "wl_seat", "xdg_wm_base"]
        .iter()
        .map(|i| Global::new(i, 1))
        .collect()
}

fn run(env: SessionEnv, outcome: ProtocolOutcome) -> nebula::probe::ProbeReport {
    assemble(
        "2026-11-19T00:00:00Z".into(),
        classify_session(&env),
        identify_compositor_with("/nonexistent/dpkg-query"),
        outcome,
        theme_summary(None, None),
    )
}

#[test]
fn ok_when_cosmic_and_required_present() {
    let r = run(cosmic_env(), ProtocolOutcome::Report(classify(&required())));
    assert_eq!(r.status, Status::Ok);
    assert_eq!(r.status.exit_code(), 0);
    assert_eq!(r.schema, "nebula.probe/0.1");
    assert_eq!(r.compositor.version, "unknown");
    assert!(!r.caprichios.is_empty());
}

#[test]
fn blocked_outside_cosmic_even_with_protocols() {
    let mut env = cosmic_env();
    env.xdg_current_desktop = Some("GNOME".into());
    let r = run(env, ProtocolOutcome::Report(classify(&required())));
    assert_eq!(r.status, Status::Blocked);
    assert_eq!(r.status.exit_code(), 3);
    assert!(r.protocols.is_some(), "the report is still written in full");
}

#[test]
fn blocked_when_display_does_not_answer() {
    let r = run(
        cosmic_env(),
        ProtocolOutcome::Failed(ProbeError::Connect("refused".into())),
    );
    assert_eq!(r.status, Status::Blocked);
    assert!(r.protocol_error.unwrap().contains("refused"));
    assert!(r.caprichios.is_empty());
}

#[test]
fn missing_required_exits_4() {
    let mut g = required();
    g.retain(|g| g.interface != "xdg_wm_base");
    let r = run(cosmic_env(), ProtocolOutcome::Report(classify(&g)));
    assert_eq!(r.status, Status::RequiredProtocolMissing);
    assert_eq!(r.status.exit_code(), 4);
    assert!(r.status_reasons.iter().any(|s| s.contains("xdg_wm_base")));
}

#[test]
fn scope_never_claims_more_than_presence() {
    let r = run(cosmic_env(), ProtocolOutcome::Report(classify(&required())));
    assert_eq!(r.scope, SCOPE);
    let json = serde_json::to_value(&r).unwrap();
    let scope = &json["scope"];
    assert_eq!(scope["protocol_presence_only"], true);
    assert_eq!(scope["window_opened"], false);
    assert_eq!(scope["surfaces_created"], false);
    assert_eq!(scope["single_host"], true);
    assert_eq!(scope["release_qualification"], false);
    assert_eq!(json["status"], "OK");
    let r = run(cosmic_env(), ProtocolOutcome::NotAttempted("test"));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["status"], "BLOCKED");
}

fn nebula() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nebula"))
}

#[test]
fn cli_usage_errors_exit_2() {
    for args in [
        &[][..],
        &["bogus"][..],
        &["probe", "--nope"][..],
        &["theme", "--out", "x"][..],
    ] {
        let s = nebula().args(args).output().unwrap().status;
        assert_eq!(s.code(), Some(2), "{args:?}");
    }
}

#[test]
fn cli_probe_outside_a_session_writes_blocked_report_and_exits_3() {
    let dir = std::env::temp_dir().join(format!("nebula-cli-{}", std::process::id()));
    let out = dir.join("probe.json");
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pop-2026-11-19");
    let s = nebula()
        .args(["probe", "--json", "--out"])
        .arg(&out)
        .args(["--config-root", fixtures])
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("XDG_CURRENT_DESKTOP")
        .env_remove("XDG_SESSION_TYPE")
        .env_remove("DISPLAY")
        .output()
        .unwrap();
    assert_eq!(s.status.code(), Some(3));
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(v["status"], "BLOCKED");
    assert_eq!(v["schema"], "nebula.probe/0.1");
    assert!(v["protocol_error"]
        .as_str()
        .unwrap()
        .contains("not attempted"));
    assert_eq!(v["theme"]["name"], "cosmic-dark");
    let stdout: serde_json::Value = serde_json::from_slice(&s.stdout).unwrap();
    assert_eq!(stdout, v);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cli_theme_json_against_fixture() {
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pop-2026-11-19");
    let o = nebula()
        .args([
            "theme",
            "--json",
            "--variant",
            "light",
            "--config-root",
            fixtures,
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["schema"], "nebula.theme/0.1");
    assert_eq!(v["theme"]["variant"], "light");
    assert_eq!(v["theme"]["chroma"]["polarity"], "light");
    let bad = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2-only");
    let o = nebula()
        .args(["theme", "--config-root", bad])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(1));
}
