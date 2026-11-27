//! Graphics rendering for Diablo Free runtime.

use crate::nir::NirGraphics;
use goya_diablos::fault::Fault;
use goya_diablos::{BackendType, GraphicsBackend, create_backend};

/// Renderer handle.
pub struct Renderer {
    backend: Box<dyn GraphicsBackend>,
}

impl Renderer {
    pub fn new(backend_type: BackendType) -> Self {
        Self {
            backend: create_backend(backend_type),
        }
    }

    pub async fn initialize(&mut self) -> Result<(), Fault> {
        self.backend.initialize().await
    }

    pub async fn present(&mut self, frame: &goya_diablos::graphics::backend::Frame) -> Result<(), Fault> {
        // Would present to window
        Ok(())
    }
}

/// Select backend based on NIR configuration.
pub fn select_backend(backend_str: &str) -> BackendType {
    match backend_str {
        "drm" => BackendType::Drm,
        "wayland" => BackendType::Wayland,
        "x11" => BackendType::X11,
        "software" => BackendType::Software,
        _ => BackendType::Auto,
    }
}

/// Create renderer from NIR graphics config.
pub fn create_renderer(
    graphics_nir: &NirGraphics,
    _window: &mut crate::window::WindowHandle,
) -> Result<Renderer, Fault> {
    let backend = select_backend(&graphics_nir.backend);
    let mut renderer = Renderer::new(backend);

    // Initialize asynchronously would happen in real implementation
    // For now, just return the renderer
    Ok(renderer)
}

/// Render a frame from GVI inventory (stub).
pub fn render_gvi(
    _renderer: &mut Renderer,
    _inventory: &goya_gvi::Inventory,
) -> Result<goya_diablos::graphics::backend::Frame, Fault> {
    // Would use goya-gvi to render
    Err(Fault::new(goya_diablos::fault::exit::VERIFICATION_FAILED, "GVI_RENDER_NOT_IMPLEMENTED", "GVI rendering not implemented"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_backend_drm() {
        assert_eq!(select_backend("drm"), BackendType::Drm);
    }

    #[test]
    fn select_backend_auto() {
        assert_eq!(select_backend("auto"), BackendType::Auto);
    }
}