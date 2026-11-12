use super::{WindowAction, WindowCapability, WindowManagementError, WindowManager};

/// Host supplied native adapter for platforms whose window handle is owned by
/// an embedding toolkit. This lets macOS/Windows hosts use the shared contract
/// without making the Linux Wayland crate link platform-specific SDKs.
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
        if !self.capabilities.contains(&match action {
            WindowAction::Close => WindowCapability::Close,
            WindowAction::Maximize => WindowCapability::Maximize,
            WindowAction::Restore => WindowCapability::Maximize,
            WindowAction::Minimize => WindowCapability::Minimize,
            WindowAction::NewTab => WindowCapability::NewTab,
        }) { return Err(WindowManagementError::Unsupported(action)); }
        (self.dispatch)(action).map_err(WindowManagementError::NativeRequestFailed)
    }
}

pub struct UnsupportedWindowManager;
impl WindowManager for UnsupportedWindowManager {
    fn capabilities(&self) -> &'static [WindowCapability] { &[] }
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
        Err(WindowManagementError::Unsupported(action))
    }
}
