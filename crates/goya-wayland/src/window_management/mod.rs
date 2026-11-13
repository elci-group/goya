//! Native backend selection for the platform-neutral window contract.
pub use goya_window_management::{
    HostWindowManager, WindowAction, WindowCapability, WindowManagementError, WindowManager,
};
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
pub mod platform {
    pub use super::linux::*;
}
#[cfg(not(target_os = "linux"))]
#[path = "unsupported.rs"]
mod unsupported;
#[cfg(not(target_os = "linux"))]
pub mod platform {
    pub use super::unsupported::*;
}
