//! Graphics integration for Diablos (GVI, GRU, GVA consumption).

pub mod gvi_integration;
pub mod backend;
pub mod proportional;

pub use gvi_integration::{compile_gvi, GviOptions};
pub use backend::{GraphicsBackend, BackendType, SoftwareBackend, DrmBackend, WaylandBackend, X11Backend, create_backend, select_backend, WindowHandle, PixelFormat, Frame, DamageRect};
pub use proportional::{resolve_geometry, ProportionalGeometry};