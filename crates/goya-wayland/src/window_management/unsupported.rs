use super::{WindowAction, WindowCapability, WindowManagementError, WindowManager};

pub struct UnsupportedWindowManager;
impl WindowManager for UnsupportedWindowManager {
    fn capabilities(&self) -> &'static [WindowCapability] { &[] }
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
        Err(WindowManagementError::Unsupported(action))
    }
}
