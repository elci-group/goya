use goya_window_management::{
    WindowAction, WindowCapability, WindowManagementError, WindowManager,
};

/// Host supplied native adapter for platforms whose window handle is owned by
/// an embedding toolkit. This lets macOS/Windows hosts use the shared contract
/// without making the Linux Wayland crate link platform-specific SDKs.
pub struct UnsupportedWindowManager;
impl WindowManager for UnsupportedWindowManager {
    fn capabilities(&self) -> &'static [WindowCapability] {
        &[]
    }
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError> {
        Err(WindowManagementError::Unsupported(action))
    }
}
