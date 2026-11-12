//! Platform-neutral window actions and backend contract.
//! Applications depend on this crate rather than a native window host.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction { Close, Maximize, Restore, Minimize, NewTab }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCapability { Close, Maximize, Minimize, NewTab }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowManagementError {
    Unsupported(WindowAction),
    NativeRequestFailed(String),
}

pub trait WindowManager {
    fn capabilities(&self) -> &'static [WindowCapability];
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError>;
}

#[cfg(target_os = "linux")]
pub mod linux {
    use super::{WindowAction, WindowCapability, WindowManagementError, WindowManager};
    use std::process::Command;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LinuxWindowManagerKind { Floating, Tiling }

    pub struct LinuxWindowManager {
        kind: LinuxWindowManagerKind,
        dispatcher: fn(WindowAction) -> Result<(), String>,
        capabilities: &'static [WindowCapability],
    }

    impl LinuxWindowManager {
        pub fn detect() -> Option<Self> {
            if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
                Some(Self { kind: LinuxWindowManagerKind::Tiling, dispatcher: hyprland, capabilities: &[WindowCapability::Close, WindowCapability::Maximize] })
            } else if std::env::var_os("SWAYSOCK").is_some() {
                Some(Self { kind: LinuxWindowManagerKind::Tiling, dispatcher: sway, capabilities: &[WindowCapability::Close, WindowCapability::Maximize] })
            } else if std::env::var_os("I3SOCK").is_some() {
                Some(Self { kind: LinuxWindowManagerKind::Tiling, dispatcher: i3, capabilities: &[WindowCapability::Close, WindowCapability::Maximize] })
            } else if std::env::var_os("DISPLAY").is_some() {
                Some(Self { kind: LinuxWindowManagerKind::Floating, dispatcher: x11, capabilities: &[WindowCapability::Close, WindowCapability::Maximize] })
            } else { None }
        }
        pub fn kind(&self) -> LinuxWindowManagerKind { self.kind }
    }

    pub fn native() -> Option<LinuxWindowManager> { LinuxWindowManager::detect() }

    fn invoke(program: &str, args: &[&str]) -> Result<(), String> {
        let out = Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
        if out.status.success() { Ok(()) } else { Err(format!("{program}: {}", String::from_utf8_lossy(&out.stderr).trim())) }
    }
    fn hyprland(a: WindowAction) -> Result<(), String> { match a {
        WindowAction::Close => invoke("hyprctl", &["dispatch", "killactive"]),
        WindowAction::Maximize => invoke("hyprctl", &["dispatch", "fullscreen", "1"]),
        WindowAction::Restore => invoke("hyprctl", &["dispatch", "fullscreen", "0"]),
        _ => Err("unsupported by Hyprland adapter".into()),
    }}
    fn sway(a: WindowAction) -> Result<(), String> { match a {
        WindowAction::Close => invoke("swaymsg", &["kill"]),
        WindowAction::Maximize | WindowAction::Restore => invoke("swaymsg", &["fullscreen", "toggle"]),
        _ => Err("unsupported by Sway adapter".into()),
    }}
    fn i3(a: WindowAction) -> Result<(), String> { match a {
        WindowAction::Close => invoke("i3-msg", &["kill"]),
        WindowAction::Maximize | WindowAction::Restore => invoke("i3-msg", &["fullscreen", "toggle"]),
        _ => Err("unsupported by i3 adapter".into()),
    }}
    fn x11(a: WindowAction) -> Result<(), String> { match a {
        WindowAction::Close => invoke("wmctrl", &["-c", ":ACTIVE:"]),
        WindowAction::Maximize => invoke("wmctrl", &["-r", ":ACTIVE:", "-b", "add,maximized_vert,maximized_horz"]),
        WindowAction::Restore => invoke("wmctrl", &["-r", ":ACTIVE:", "-b", "remove,maximized_vert,maximized_horz"]),
        _ => Err("unsupported by X11 adapter".into()),
    }}

    impl WindowManager for LinuxWindowManager {
        fn capabilities(&self) -> &'static [WindowCapability] { self.capabilities }
        fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
            (self.dispatcher)(action).map_err(WindowManagementError::NativeRequestFailed)
        }
    }
}

/// Adapter for GUI frameworks that own the native window handle.
pub struct HostWindowManager {
    capabilities: &'static [WindowCapability],
    dispatch: Box<dyn FnMut(WindowAction) -> Result<(), String> + Send>,
}

impl HostWindowManager {
    pub fn new(
        capabilities: &'static [WindowCapability],
        dispatch: impl FnMut(WindowAction) -> Result<(), String> + Send + 'static,
    ) -> Self { Self { capabilities, dispatch: Box::new(dispatch) } }
}

impl WindowManager for HostWindowManager {
    fn capabilities(&self) -> &'static [WindowCapability] { self.capabilities }
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
        let capability = match action {
            WindowAction::Close => WindowCapability::Close,
            WindowAction::Maximize | WindowAction::Restore => WindowCapability::Maximize,
            WindowAction::Minimize => WindowCapability::Minimize,
            WindowAction::NewTab => WindowCapability::NewTab,
        };
        if !self.capabilities.contains(&capability) {
            return Err(WindowManagementError::Unsupported(action));
        }
        (self.dispatch)(action).map_err(WindowManagementError::NativeRequestFailed)
    }
}
