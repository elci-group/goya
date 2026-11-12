//! Linux window-manager profiles. Wayland clients can request maximize, but
//! minimize and tab creation are not standardized xdg-shell operations.
use goya_window_management::{WindowAction, WindowCapability, WindowManagementError, WindowManager};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxWindowManagerKind { Floating, Tiling }

pub struct LinuxWindowManager {
    kind: LinuxWindowManagerKind,
    request: Box<dyn FnMut(WindowAction) -> Result<(), String> + Send>,
}

impl LinuxWindowManager {
    /// Build a manager around a native host's action dispatcher. Tiling and
    /// floating are separate profiles because maximize/minimize semantics vary.
    pub fn new(
        kind: LinuxWindowManagerKind,
        request: impl FnMut(WindowAction) -> Result<(), String> + Send + 'static,
    ) -> Self { Self { kind, request: Box::new(request) } }

    pub fn kind(&self) -> LinuxWindowManagerKind { self.kind }

    /// Detect a supported desktop from its standard environment marker.
    /// The returned manager issues requests through the desktop's native CLI.
    pub fn detect() -> Option<Self> {
        if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
            Some(Self::new(LinuxWindowManagerKind::Tiling, run_hyprland))
        } else if std::env::var_os("SWAYSOCK").is_some() {
            Some(Self::new(LinuxWindowManagerKind::Tiling, run_sway))
        } else if std::env::var_os("I3SOCK").is_some() {
            Some(Self::new(LinuxWindowManagerKind::Tiling, run_i3))
        } else if std::env::var_os("DISPLAY").is_some() {
            Some(Self::new(LinuxWindowManagerKind::Floating, run_x11))
        } else { None }
    }
}

/// Build the detected Linux backend, if the current desktop is recognized.
pub fn native() -> Option<LinuxWindowManager> { LinuxWindowManager::detect() }

const LINUX_CAPABILITIES: &[WindowCapability] = &[WindowCapability::Maximize];

impl WindowManager for LinuxWindowManager {
    fn capabilities(&self) -> &'static [WindowCapability] { LINUX_CAPABILITIES }
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
        if matches!(action, WindowAction::Close | WindowAction::Minimize | WindowAction::NewTab) {
            return Err(WindowManagementError::Unsupported(action));
        }
        (self.request)(action).map_err(WindowManagementError::NativeRequestFailed)
    }
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if output.status.success() { Ok(()) } else {
        Err(format!("{program} exited with {}: {}", output.status,
            String::from_utf8_lossy(&output.stderr).trim()))
    }
}

fn run_hyprland(action: WindowAction) -> Result<(), String> {
    match action {
        WindowAction::Maximize => run("hyprctl", &["dispatch", "fullscreen", "1"]),
        WindowAction::Restore => run("hyprctl", &["dispatch", "fullscreen", "0"]),
        _ => Err("unsupported by the Hyprland adapter".into()),
    }
}
fn run_sway(action: WindowAction) -> Result<(), String> {
    match action {
        WindowAction::Maximize | WindowAction::Restore => run("swaymsg", &["fullscreen", "toggle"]),
        _ => Err("unsupported by the Sway adapter".into()),
    }
}
fn run_i3(action: WindowAction) -> Result<(), String> {
    match action {
        WindowAction::Maximize | WindowAction::Restore => run("i3-msg", &["fullscreen", "toggle"]),
        _ => Err("unsupported by the i3 adapter".into()),
    }
}
fn run_x11(action: WindowAction) -> Result<(), String> {
    match action {
        WindowAction::Maximize => run("wmctrl", &["-r", ":ACTIVE:", "-b", "add,maximized_vert,maximized_horz"]),
        WindowAction::Restore => run("wmctrl", &["-r", ":ACTIVE:", "-b", "remove,maximized_vert,maximized_horz"]),
        _ => Err("unsupported by the X11 floating adapter".into()),
    }
}

