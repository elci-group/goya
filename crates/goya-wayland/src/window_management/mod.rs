//! Cross-platform window management contracts. Backends expose only operations
//! that the native window system can request; final behavior remains owned by
//! the compositor/window manager.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction { Close, Maximize, Minimize, NewTab }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCapability { Close, Maximize, Minimize, NewTab }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowManagementError {
    Unsupported(WindowAction),
    NativeRequestFailed(String),
}

/// Platform implementations translate app intent into native window requests.
pub trait WindowManager {
    fn capabilities(&self) -> &'static [WindowCapability];
    fn request(&mut self, action: WindowAction) -> Result<(), WindowManagementError>;
}

#[cfg(target_os = "linux")]
pub mod platform { pub use super::linux::*; }

#[cfg(not(target_os = "linux"))]
pub mod platform { pub use super::unsupported::*; }

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod unsupported;

