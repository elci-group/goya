//! Graphics backend abstraction for Diablos.

use crate::Fault;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Graphics backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackendType {
    /// Auto-select based on capabilities
    Auto,
    /// DRM/KMS direct rendering
    Drm,
    /// Wayland compositor
    Wayland,
    /// X11 server
    X11,
    /// Software rendering (CPU)
    Software,
}

impl fmt::Display for BackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendType::Auto => write!(f, "auto"),
            BackendType::Drm => write!(f, "drm"),
            BackendType::Wayland => write!(f, "wayland"),
            BackendType::X11 => write!(f, "x11"),
            BackendType::Software => write!(f, "software"),
        }
    }
}

/// Graphics backend trait.
#[async_trait]
pub trait GraphicsBackend: Send + Sync {
    /// Backend type identifier.
    fn backend_type(&self) -> BackendType;

    /// Initialize the backend.
    async fn initialize(&mut self) -> Result<(), Fault>;

    /// Create a window/surface.
    async fn create_window(&mut self, width: u32, height: u32, title: &str) -> Result<WindowHandle, Fault>;

    /// Present a frame.
    async fn present(&mut self, handle: WindowHandle, frame: &Frame) -> Result<(), Fault>;

    /// Get supported formats.
    fn supported_formats(&self) -> Vec<PixelFormat>;

    /// Get maximum texture size.
    fn max_texture_size(&self) -> (u32, u32);

    /// Shutdown the backend.
    async fn shutdown(&mut self) -> Result<(), Fault>;
}

/// Window handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowHandle(pub u64);

/// Pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PixelFormat {
    Rgba8888,
    Bgra8888,
    Rgb565,
    Argb8888,
}

/// Frame data for presentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
    pub damage_rects: Vec<DamageRect>,
}

/// Damage rectangle for partial updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Software rendering backend (always available).
pub struct SoftwareBackend {
    initialized: bool,
}

impl SoftwareBackend {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

#[async_trait]
impl GraphicsBackend for SoftwareBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Software
    }

    async fn initialize(&mut self) -> Result<(), Fault> {
        self.initialized = true;
        Ok(())
    }

    async fn create_window(&mut self, _width: u32, _height: u32, _title: &str) -> Result<WindowHandle, Fault> {
        if !self.initialized {
            return Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "BACKEND_NOT_INITIALIZED", "Software backend not initialized"));
        }
        Ok(WindowHandle(rand::random()))
    }

    async fn present(&mut self, _handle: WindowHandle, _frame: &Frame) -> Result<(), Fault> {
        // Software rendering: would blit to framebuffer or window
        Ok(())
    }

    fn supported_formats(&self) -> Vec<PixelFormat> {
        vec![PixelFormat::Rgba8888, PixelFormat::Bgra8888, PixelFormat::Rgb565]
    }

    fn max_texture_size(&self) -> (u32, u32) {
        (8192, 8192)
    }

    async fn shutdown(&mut self) -> Result<(), Fault> {
        self.initialized = false;
        Ok(())
    }
}

/// DRM/KMS backend (Linux/BSD direct rendering).
pub struct DrmBackend {
    initialized: bool,
    device_path: Option<String>,
}

impl DrmBackend {
    pub fn new() -> Self {
        Self { initialized: false, device_path: None }
    }

    pub fn with_device(mut self, path: String) -> Self {
        self.device_path = Some(path);
        self
    }
}

#[async_trait]
impl GraphicsBackend for DrmBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Drm
    }

    async fn initialize(&mut self) -> Result<(), Fault> {
        // Would open /dev/dri/card* or /dev/dri/renderD*
        self.initialized = true;
        Ok(())
    }

    async fn create_window(&mut self, _width: u32, _height: u32, _title: &str) -> Result<WindowHandle, Fault> {
        if !self.initialized {
            return Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "BACKEND_NOT_INITIALIZED", "DRM backend not initialized"));
        }
        Ok(WindowHandle(rand::random()))
    }

    async fn present(&mut self, _handle: WindowHandle, _frame: &Frame) -> Result<(), Fault> {
        // Would use DRM atomic modesetting or legacy page flip
        Ok(())
    }

    fn supported_formats(&self) -> Vec<PixelFormat> {
        vec![PixelFormat::Rgba8888, PixelFormat::Bgra8888]
    }

    fn max_texture_size(&self) -> (u32, u32) {
        (16384, 16384)
    }

    async fn shutdown(&mut self) -> Result<(), Fault> {
        self.initialized = false;
        Ok(())
    }
}

/// Wayland backend.
pub struct WaylandBackend {
    initialized: bool,
}

impl WaylandBackend {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

#[async_trait]
impl GraphicsBackend for WaylandBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Wayland
    }

    async fn initialize(&mut self) -> Result<(), Fault> {
        // Would connect to Wayland compositor
        self.initialized = true;
        Ok(())
    }

    async fn create_window(&mut self, _width: u32, _height: u32, _title: &str) -> Result<WindowHandle, Fault> {
        if !self.initialized {
            return Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "BACKEND_NOT_INITIALIZED", "Wayland backend not initialized"));
        }
        Ok(WindowHandle(rand::random()))
    }

    async fn present(&mut self, _handle: WindowHandle, _frame: &Frame) -> Result<(), Fault> {
        // Would use wl_surface + wp_presentation_feedback
        Ok(())
    }

    fn supported_formats(&self) -> Vec<PixelFormat> {
        vec![PixelFormat::Rgba8888, PixelFormat::Bgra8888]
    }

    fn max_texture_size(&self) -> (u32, u32) {
        (16384, 16384)
    }

    async fn shutdown(&mut self) -> Result<(), Fault> {
        self.initialized = false;
        Ok(())
    }
}

/// X11 backend.
pub struct X11Backend {
    initialized: bool,
}

impl X11Backend {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

#[async_trait]
impl GraphicsBackend for X11Backend {
    fn backend_type(&self) -> BackendType {
        BackendType::X11
    }

    async fn initialize(&mut self) -> Result<(), Fault> {
        // Would connect to X server
        self.initialized = true;
        Ok(())
    }

    async fn create_window(&mut self, _width: u32, _height: u32, _title: &str) -> Result<WindowHandle, Fault> {
        if !self.initialized {
            return Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "BACKEND_NOT_INITIALIZED", "X11 backend not initialized"));
        }
        Ok(WindowHandle(rand::random()))
    }

    async fn present(&mut self, _handle: WindowHandle, _frame: &Frame) -> Result<(), Fault> {
        // Would use XShmPutImage or similar
        Ok(())
    }

    fn supported_formats(&self) -> Vec<PixelFormat> {
        vec![PixelFormat::Rgba8888, PixelFormat::Bgra8888, PixelFormat::Rgb565]
    }

    fn max_texture_size(&self) -> (u32, u32) {
        (16384, 16384)
    }

    async fn shutdown(&mut self) -> Result<(), Fault> {
        self.initialized = false;
        Ok(())
    }
}

/// Backend factory for creating the appropriate backend.
pub fn create_backend(backend_type: BackendType) -> Box<dyn GraphicsBackend> {
    match backend_type {
        BackendType::Auto => Box::new(SoftwareBackend::new()), // Default to software
        BackendType::Drm => Box::new(DrmBackend::new()),
        BackendType::Wayland => Box::new(WaylandBackend::new()),
        BackendType::X11 => Box::new(X11Backend::new()),
        BackendType::Software => Box::new(SoftwareBackend::new()),
    }
}

/// Select best available backend based on system capabilities.
pub fn select_backend(preference: BackendType, has_gpu: bool, has_wayland: bool, has_x11: bool) -> BackendType {
    match preference {
        BackendType::Auto => {
            if has_gpu {
                if has_wayland { BackendType::Wayland }
                else if has_x11 { BackendType::X11 }
                else { BackendType::Drm }
            } else {
                BackendType::Software
            }
        }
        BackendType::Drm => {
            if has_gpu { BackendType::Drm } else { BackendType::Software }
        }
        BackendType::Wayland => {
            if has_wayland { BackendType::Wayland } else { BackendType::Software }
        }
        BackendType::X11 => {
            if has_x11 { BackendType::X11 } else { BackendType::Software }
        }
        BackendType::Software => BackendType::Software,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_selection_gpu_wayland() {
        assert_eq!(select_backend(BackendType::Auto, true, true, true), BackendType::Wayland);
    }

    #[test]
    fn backend_selection_gpu_x11_only() {
        assert_eq!(select_backend(BackendType::Auto, true, false, true), BackendType::X11);
    }

    #[test]
    fn backend_selection_no_gpu() {
        assert_eq!(select_backend(BackendType::Auto, false, true, true), BackendType::Software);
    }

    #[test]
    fn backend_selection_explicit() {
        assert_eq!(select_backend(BackendType::Drm, true, true, true), BackendType::Drm);
        assert_eq!(select_backend(BackendType::Software, true, true, true), BackendType::Software);
    }
}