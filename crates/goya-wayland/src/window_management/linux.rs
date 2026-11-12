//! Linux window-manager profiles. Wayland clients can request maximize, but
//! minimize and tab creation are not standardized xdg-shell operations.
use super::{WindowAction, WindowCapability, WindowManagementError, WindowManager};

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
}

const LINUX_CAPABILITIES: &[WindowCapability] = &[
    WindowCapability::Close, WindowCapability::Maximize, WindowCapability::Minimize,
];

impl WindowManager for LinuxWindowManager {
    fn capabilities(&self) -> &'static [WindowCapability] { LINUX_CAPABILITIES }
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
        if action == WindowAction::NewTab { return Err(WindowManagementError::Unsupported(action)); }
        (self.request)(action).map_err(WindowManagementError::NativeRequestFailed)
    }
}

