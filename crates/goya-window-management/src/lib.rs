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
