//! COSMIC Wayland session detection and compositor identification.
//!
//! A session qualifies for the Nebula probe when all three hold:
//! `XDG_CURRENT_DESKTOP` names COSMIC (one colon-separated component equal
//! to `COSMIC`, case-insensitively), `WAYLAND_DISPLAY` is set and non-empty,
//! and `XDG_SESSION_TYPE` is `wayland`. Whether the Wayland socket actually
//! answers is checked later by [`crate::protocols::list_globals`].
//!
//! An X11 `DISPLAY` is reported when present. Inside a Wayland session it is
//! Xwayland, and it never counts toward COSMIC qualification.

use serde::Serialize;
use std::process::Command;

/// The environment variables Nebula reads. Captured once so detection is a
/// pure function and tests can use fixed values.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SessionEnv {
    pub xdg_current_desktop: Option<String>,
    pub xdg_session_type: Option<String>,
    pub wayland_display: Option<String>,
    pub display: Option<String>,
}

impl SessionEnv {
    /// Read the variables from the current process environment.
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var(k).ok();
        SessionEnv {
            xdg_current_desktop: get("XDG_CURRENT_DESKTOP"),
            xdg_session_type: get("XDG_SESSION_TYPE"),
            wayland_display: get("WAYLAND_DISPLAY"),
            display: get("DISPLAY"),
        }
    }
}

/// What the environment says about the session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionVerdict {
    pub env: SessionEnv,
    pub is_cosmic_desktop: bool,
    pub is_wayland_session: bool,
    pub wayland_display_set: bool,
    /// True only when all three checks above hold.
    pub qualifies: bool,
    /// One line per failed check.
    pub reasons: Vec<String>,
    /// An X11 display seen alongside the session. Never qualifying.
    pub x11_display: Option<X11Display>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct X11Display {
    pub display: String,
    /// `xwayland` inside a Wayland session, `unknown` otherwise.
    pub kind: &'static str,
    pub counts_as_cosmic_qualification: bool,
    pub note: &'static str,
}

fn non_empty(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// Classify a captured environment. Pure.
pub fn classify(env: &SessionEnv) -> SessionVerdict {
    let is_cosmic_desktop = non_empty(&env.xdg_current_desktop)
        .map(|d| {
            d.split(':')
                .any(|c| c.trim().eq_ignore_ascii_case("cosmic"))
        })
        .unwrap_or(false);
    let is_wayland_session = non_empty(&env.xdg_session_type)
        .map(|t| t.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false);
    let wayland_display_set = non_empty(&env.wayland_display).is_some();

    let mut reasons = Vec::new();
    if !is_cosmic_desktop {
        reasons.push(format!(
            "XDG_CURRENT_DESKTOP does not name COSMIC (value: {})",
            env.xdg_current_desktop.as_deref().unwrap_or("<unset>")
        ));
    }
    if !is_wayland_session {
        reasons.push(format!(
            "XDG_SESSION_TYPE is not wayland (value: {})",
            env.xdg_session_type.as_deref().unwrap_or("<unset>")
        ));
    }
    if !wayland_display_set {
        reasons.push("WAYLAND_DISPLAY is unset or empty".to_string());
    }

    let x11_display = non_empty(&env.display).map(|d| {
        let xwayland = is_wayland_session && wayland_display_set;
        X11Display {
            display: d.to_string(),
            kind: if xwayland { "xwayland" } else { "unknown" },
            counts_as_cosmic_qualification: false,
            note: if xwayland {
                "DISPLAY inside a Wayland session is Xwayland; it is not native X11 and not COSMIC qualification"
            } else {
                "DISPLAY is set outside a Wayland session; Nebula does not qualify X11"
            },
        }
    });

    SessionVerdict {
        env: env.clone(),
        is_cosmic_desktop,
        is_wayland_session,
        wayland_display_set,
        qualifies: is_cosmic_desktop && is_wayland_session && wayland_display_set,
        reasons,
        x11_display,
    }
}

/// Package whose version identifies the compositor build.
pub const COMPOSITOR_PACKAGE: &str = "cosmic-comp";

/// How the compositor build was identified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CompositorBuild {
    pub package: &'static str,
    /// The installed package version, or `"unknown"`.
    pub version: String,
    /// `dpkg-query` when the query succeeded, `unavailable` otherwise.
    pub method: &'static str,
    /// The exact command that was attempted.
    pub command: String,
    /// Why the version is unknown, when it is.
    pub detail: Option<String>,
    /// The installed package is not proof that this binary is the running
    /// compositor; that is not checked.
    pub note: &'static str,
}

const BUILD_NOTE: &str =
    "installed package version; not verified to be the running compositor process";

/// Interpret the result of running `dpkg-query`. Pure.
pub fn interpret_dpkg(
    command: String,
    result: std::io::Result<std::process::Output>,
) -> CompositorBuild {
    let unknown = |detail: String| CompositorBuild {
        package: COMPOSITOR_PACKAGE,
        version: "unknown".to_string(),
        method: "unavailable",
        command: command.clone(),
        detail: Some(detail),
        note: BUILD_NOTE,
    };
    match result {
        Err(e) => unknown(format!("could not run dpkg-query: {e}")),
        Ok(out) if !out.status.success() => unknown(format!(
            "dpkg-query exited with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Ok(out) => {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if v.is_empty() {
                unknown("dpkg-query returned an empty version (package not installed?)".into())
            } else {
                CompositorBuild {
                    package: COMPOSITOR_PACKAGE,
                    version: v,
                    method: "dpkg-query",
                    command,
                    detail: None,
                    note: BUILD_NOTE,
                }
            }
        }
    }
}

/// Identify the compositor build with the given `dpkg-query` program.
pub fn identify_compositor_with(program: &str) -> CompositorBuild {
    let args = ["-W", "-f=${Version}", COMPOSITOR_PACKAGE];
    let command = format!("{program} -W -f='${{Version}}' {COMPOSITOR_PACKAGE}");
    interpret_dpkg(command, Command::new(program).args(args).output())
}

/// Identify the compositor build with `dpkg-query` from `PATH`.
pub fn identify_compositor() -> CompositorBuild {
    identify_compositor_with("dpkg-query")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(desktop: &str, ty: &str, wl: &str, x: &str) -> SessionEnv {
        let o = |s: &str| (!s.is_empty()).then(|| s.to_string());
        SessionEnv {
            xdg_current_desktop: o(desktop),
            xdg_session_type: o(ty),
            wayland_display: o(wl),
            display: o(x),
        }
    }

    #[test]
    fn cosmic_wayland_qualifies_and_xwayland_does_not_count() {
        let v = classify(&env("COSMIC", "wayland", "wayland-1", ":1"));
        assert!(v.qualifies);
        assert!(v.reasons.is_empty());
        let x = v.x11_display.unwrap();
        assert_eq!(x.kind, "xwayland");
        assert!(!x.counts_as_cosmic_qualification);
    }

    #[test]
    fn desktop_match_is_case_insensitive_and_colon_separated() {
        assert!(classify(&env("pop:cosmic", "wayland", "wayland-0", "")).qualifies);
        assert!(classify(&env("Cosmic", "Wayland", "w", "")).qualifies);
        assert!(!classify(&env("GNOME", "wayland", "wayland-0", "")).qualifies);
        assert!(!classify(&env("NOTCOSMIC", "wayland", "wayland-0", "")).qualifies);
    }

    #[test]
    fn each_failed_check_is_reported() {
        let v = classify(&env("COSMIC", "x11", "", ":0"));
        assert!(!v.qualifies);
        assert_eq!(v.reasons.len(), 2);
        assert_eq!(v.x11_display.unwrap().kind, "unknown");
        let v = classify(&SessionEnv::default());
        assert_eq!(v.reasons.len(), 3);
        assert!(v.x11_display.is_none());
    }

    #[test]
    fn missing_dpkg_query_falls_back_to_unknown() {
        let b = identify_compositor_with("/nonexistent/nebula-dpkg-query");
        assert_eq!(b.version, "unknown");
        assert_eq!(b.method, "unavailable");
        assert!(b.detail.unwrap().contains("could not run"));
    }

    #[test]
    fn failing_or_empty_query_is_unknown() {
        // `false` exits non-zero with no output.
        let b = identify_compositor_with("false");
        assert_eq!(b.version, "unknown");
        // `true` succeeds with empty output.
        let b = identify_compositor_with("true");
        assert_eq!(b.version, "unknown");
        assert!(b.detail.unwrap().contains("empty"));
    }
}
