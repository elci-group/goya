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
use std::path::Path;
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
    /// Process candidate found through procfs. Its version is not inferred
    /// from the installed package version.
    pub running_process: RunningCompositor,
}

/// A process whose `/proc/<pid>/comm` is exactly `cosmic-comp`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunningCompositorProcess {
    pub pid: u32,
    pub comm: String,
    /// Target reported by `/proc/<pid>/exe`; unavailable if procfs refused it.
    pub executable: Option<String>,
    /// True when the kernel appended ` (deleted)` to the executable target.
    pub executable_deleted: bool,
    pub detail: Option<String>,
}

/// A procfs observation, deliberately separate from the installed package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunningCompositor {
    /// `candidate`, `ambiguous`, `not-found`, `incomplete`, `unavailable`, or `not-checked`.
    pub status: &'static str,
    pub method: &'static str,
    pub processes: Vec<RunningCompositorProcess>,
    /// Always unknown: procfs executable paths alone do not establish a build version.
    pub version: Option<String>,
    /// Always unknown without comparing the running binary bytes to a trusted package file.
    pub matches_installed_package: Option<bool>,
    pub uncertainty: &'static str,
    pub detail: Option<String>,
}

const RUNNING_UNCERTAINTY: &str =
    "candidate selected by process comm and executable path; running version and package identity are not verified";

fn running_not_checked() -> RunningCompositor {
    RunningCompositor {
        status: "not-checked",
        method: "not-checked",
        processes: Vec::new(),
        version: None,
        matches_installed_package: None,
        uncertainty: RUNNING_UNCERTAINTY,
        detail: None,
    }
}

/// Resolve a candidate from already-read procfs observations. This seam keeps
/// status decisions deterministic and independent of the host process table.
pub fn identify_running_compositor_from(
    processes: Vec<RunningCompositorProcess>,
) -> RunningCompositor {
    RunningCompositor {
        status: match processes.len() {
            0 => "not-found",
            1 => "candidate",
            _ => "ambiguous",
        },
        method: "procfs /proc/<pid>/comm and /proc/<pid>/exe",
        processes,
        version: None,
        matches_installed_package: None,
        uncertainty: RUNNING_UNCERTAINTY,
        detail: None,
    }
}

/// Inspect procfs for the COSMIC compositor process without assigning it the
/// version of the installed `cosmic-comp` package.
pub fn identify_running_compositor_in(proc_root: &Path) -> RunningCompositor {
    let entries = match std::fs::read_dir(proc_root) {
        Ok(entries) => entries,
        Err(error) => {
            return RunningCompositor {
                status: "unavailable",
                method: "procfs /proc/<pid>/comm and /proc/<pid>/exe",
                processes: Vec::new(),
                version: None,
                matches_installed_package: None,
                uncertainty: RUNNING_UNCERTAINTY,
                detail: Some(format!("could not read {}: {error}", proc_root.display())),
            };
        }
    };

    let mut processes = Vec::new();
    let mut scan_errors = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                scan_errors.push(format!("could not read a procfs entry: {error}"));
                continue;
            }
        };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let process_dir = entry.path();
        let comm = match std::fs::read_to_string(process_dir.join("comm")) {
            Ok(comm) if comm.trim() == COMPOSITOR_PACKAGE => comm.trim().to_string(),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                scan_errors.push(format!("could not read comm for pid {pid}: {error}"));
                continue;
            }
        };
        let (executable, executable_deleted, detail) =
            match std::fs::read_link(process_dir.join("exe")) {
                Ok(path) => {
                    let value = path.to_string_lossy().into_owned();
                    let deleted = value.ends_with(" (deleted)");
                    (Some(value), deleted, None)
                }
                Err(error) => (
                    None,
                    false,
                    Some(format!("could not read exe link: {error}")),
                ),
            };
        processes.push(RunningCompositorProcess {
            pid,
            comm,
            executable,
            executable_deleted,
            detail,
        });
    }
    let mut result = identify_running_compositor_from(processes);
    if !scan_errors.is_empty() {
        if result.processes.is_empty() {
            result.status = "incomplete";
        }
        result.detail = Some(scan_errors.join("; "));
    }
    result
}

/// Inspect the current process table through `/proc`.
pub fn identify_running_compositor() -> RunningCompositor {
    identify_running_compositor_in(Path::new("/proc"))
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
        running_process: running_not_checked(),
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
                    running_process: running_not_checked(),
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
    let mut build = identify_compositor_with("dpkg-query");
    build.running_process = identify_running_compositor();
    build
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

    #[test]
    fn running_process_candidate_does_not_inherit_installed_version() {
        let candidate = identify_running_compositor_from(vec![RunningCompositorProcess {
            pid: 42,
            comm: COMPOSITOR_PACKAGE.to_string(),
            executable: Some("/usr/libexec/cosmic-comp".into()),
            executable_deleted: false,
            detail: None,
        }]);
        assert_eq!(candidate.status, "candidate");
        assert_eq!(candidate.processes[0].pid, 42);
        assert_eq!(candidate.version, None);
        assert_eq!(candidate.matches_installed_package, None);
        assert!(candidate.uncertainty.contains("not verified"));

        let ambiguous = identify_running_compositor_from(vec![
            candidate.processes[0].clone(),
            RunningCompositorProcess {
                pid: 43,
                ..candidate.processes[0].clone()
            },
        ]);
        assert_eq!(ambiguous.status, "ambiguous");
    }

    #[cfg(unix)]
    #[test]
    fn procfs_scan_uses_comm_and_records_executable_target() {
        use std::os::unix::fs::symlink;

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("nebula-procfs-{}-{stamp}", std::process::id()));
        let cosmic = root.join("123");
        let other = root.join("456");
        std::fs::create_dir_all(&cosmic).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(cosmic.join("comm"), "cosmic-comp\n").unwrap();
        std::fs::write(other.join("comm"), "other-process\n").unwrap();
        symlink("/usr/libexec/cosmic-comp (deleted)", cosmic.join("exe")).unwrap();

        let result = identify_running_compositor_in(&root);
        assert_eq!(result.status, "candidate");
        assert_eq!(result.method, "procfs /proc/<pid>/comm and /proc/<pid>/exe");
        assert_eq!(result.processes.len(), 1);
        assert_eq!(result.processes[0].pid, 123);
        assert_eq!(
            result.processes[0].executable.as_deref(),
            Some("/usr/libexec/cosmic-comp (deleted)")
        );
        assert!(result.processes[0].executable_deleted);
        assert_eq!(result.version, None);
        assert_eq!(result.matches_installed_package, None);

        std::fs::remove_dir_all(root).unwrap();
    }
}
