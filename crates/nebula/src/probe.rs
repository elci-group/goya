//! The `nebula.probe/0.1` record.
//!
//! [`assemble`] is pure: given the session verdict, compositor build,
//! protocol result and theme summary it decides the status. [`run`] gathers
//! those inputs from the live host.

use crate::caprichios::{select_for_cosmic, Selection};
use crate::protocols::{self, ProbeError, ProtocolReport};
use crate::session::{self, CompositorBuild, SessionEnv, SessionVerdict};
use crate::theme::{
    self, ChromaMapping, CosmicGeometry, KeyIssue, Mode, ThemeError, ThemeSource, Variant,
};
use serde::Serialize;
use std::path::Path;

/// Probe outcome. Exit codes: OK 0, BLOCKED 3, REQUIRED_PROTOCOL_MISSING 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    /// COSMIC Wayland session, registry read, required globals advertised.
    Ok,
    /// Not a COSMIC Wayland session, or its display did not answer.
    Blocked,
    /// A required global is absent or below the profile minimum.
    RequiredProtocolMissing,
}

impl Status {
    pub fn exit_code(self) -> i32 {
        match self {
            Status::Ok => 0,
            Status::Blocked => 3,
            Status::RequiredProtocolMissing => 4,
        }
    }
}

/// What the record covers. Fixed: the probe never widens its own scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scope {
    pub protocol_presence_only: bool,
    pub window_opened: bool,
    pub surfaces_created: bool,
    pub single_host: bool,
    pub release_qualification: bool,
    pub compositor_matrix: &'static str,
    pub statement: &'static str,
}

pub const SCOPE: Scope = Scope {
    protocol_presence_only: true,
    window_opened: false,
    surfaces_created: false,
    single_host: true,
    release_qualification: false,
    compositor_matrix: "not run; EMP-M1-07 stays BLOCKED",
    statement: "Protocol presence only. No window was opened and no surface was created. One host. This is not a release qualification and not COSMIC compositor qualification: advertisement is not behaviour (GY-EMP-001 §4.3).",
};

/// Theme portion of the probe and of `nebula theme --json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThemeSummary {
    /// Config root, with the home directory shown as `~`.
    pub config_root: String,
    pub format: &'static str,
    pub mode: Option<Mode>,
    pub mode_error: Option<ThemeError>,
    pub variant: Option<Variant>,
    pub name: Option<String>,
    pub other_versions_present: Vec<String>,
    pub issues: Vec<KeyIssue>,
    pub chroma: Option<ChromaMapping>,
    pub geometry: Option<CosmicGeometry>,
    pub error: Option<ThemeError>,
}

fn display_root(root: &Path) -> String {
    let s = root.display().to_string();
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s,
    }
}

/// Read and map the theme. `variant` overrides the mode when given.
pub fn theme_summary(source: Option<&ThemeSource>, variant: Option<Variant>) -> ThemeSummary {
    let empty = |root: String, error: ThemeError| ThemeSummary {
        config_root: root,
        format: theme::COSMIC_THEME_FORMAT,
        mode: None,
        mode_error: None,
        variant: None,
        name: None,
        other_versions_present: Vec::new(),
        issues: Vec::new(),
        chroma: None,
        geometry: None,
        error: Some(error),
    };
    let Some(source) = source else {
        return empty("<none>".into(), ThemeError::NoConfigRoot);
    };
    let root = display_root(&source.config_root);
    let (mode, mode_error) = match theme::read_mode(source) {
        Ok(m) => (Some(m), None),
        Err(e) => (None, Some(e)),
    };
    let chosen = variant
        .or(mode.as_ref().map(|m| m.selected))
        .unwrap_or(Variant::Dark);
    match theme::load(source, chosen) {
        Err(e) => ThemeSummary {
            mode,
            mode_error,
            ..empty(root, e)
        },
        Ok(loaded) => ThemeSummary {
            config_root: root,
            format: loaded.format,
            mode,
            mode_error,
            variant: Some(loaded.variant),
            name: loaded.theme.name.clone(),
            other_versions_present: loaded.other_versions_present.clone(),
            issues: loaded.issues.clone(),
            chroma: Some(theme::map_to_chroma(&loaded)),
            geometry: Some(theme::geometry(&loaded)),
            error: None,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProbeReport {
    pub schema: &'static str,
    pub nebula_version: &'static str,
    pub generated_utc: String,
    pub status: Status,
    pub status_reasons: Vec<String>,
    pub scope: Scope,
    pub session: SessionVerdict,
    pub compositor: CompositorBuild,
    pub protocols: Option<ProtocolReport>,
    pub protocol_error: Option<String>,
    pub caprichios: Vec<Selection>,
    pub theme: ThemeSummary,
}

/// The protocol step's outcome.
#[derive(Debug, Clone)]
pub enum ProtocolOutcome {
    Report(ProtocolReport),
    Failed(ProbeError),
    NotAttempted(&'static str),
}

/// Build the record and decide the status. Pure.
pub fn assemble(
    generated_utc: String,
    session: SessionVerdict,
    compositor: CompositorBuild,
    protocols: ProtocolOutcome,
    theme: ThemeSummary,
) -> ProbeReport {
    let mut reasons: Vec<String> = session.reasons.clone();
    let (report, protocol_error) = match protocols {
        ProtocolOutcome::Report(r) => (Some(r), None),
        ProtocolOutcome::Failed(e) => (None, Some(e.to_string())),
        ProtocolOutcome::NotAttempted(why) => (None, Some(format!("not attempted: {why}"))),
    };
    if let Some(e) = &protocol_error {
        reasons.push(format!("protocol listing: {e}"));
    }
    let status = if !session.qualifies || report.is_none() {
        Status::Blocked
    } else if !report.as_ref().is_some_and(ProtocolReport::required_ok) {
        let missing = report.as_ref().map(|r| r.required_missing.join(", "));
        reasons.push(format!(
            "required globals missing: {}",
            missing.unwrap_or_default()
        ));
        Status::RequiredProtocolMissing
    } else {
        Status::Ok
    };
    ProbeReport {
        schema: crate::PROBE_SCHEMA,
        nebula_version: crate::NEBULA_VERSION,
        generated_utc,
        status,
        status_reasons: reasons,
        scope: SCOPE,
        caprichios: report.as_ref().map(select_for_cosmic).unwrap_or_default(),
        session,
        compositor,
        protocols: report,
        protocol_error,
        theme,
    }
}

/// Run the live probe.
pub fn run(source: Option<&ThemeSource>) -> ProbeReport {
    let session = session::classify(&SessionEnv::from_process());
    let compositor = session::identify_compositor();
    let protocols = if session.wayland_display_set {
        match protocols::probe() {
            Ok(r) => ProtocolOutcome::Report(r),
            Err(e) => ProtocolOutcome::Failed(e),
        }
    } else {
        ProtocolOutcome::NotAttempted("WAYLAND_DISPLAY is unset")
    };
    let theme = theme_summary(source, None);
    assemble(
        crate::time::utc_timestamp(std::time::SystemTime::now()),
        session,
        compositor,
        protocols,
        theme,
    )
}
