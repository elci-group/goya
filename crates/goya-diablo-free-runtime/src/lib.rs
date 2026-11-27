//! Diablo Free Runtime — FreeBSD native execution runtime for Goya applications.

#![forbid(unsafe_code)]

pub mod compiled_scene;
pub mod graphics;
pub mod nir;
pub mod run_mode;
pub mod security;
pub mod window;

use crate::nir::FreeBsdNir;
use crate::run_mode::RunMode;
use crate::security::CapsicumSandbox;
use goya_diablos::fault::Fault;

/// Diablo Free runtime version.
pub const RUNTIME_VERSION: &str = "0.0.1";

/// Runtime profile.
pub const PROFILE: &str = "diablo.free.interactive/0.0.1";

/// Load and validate NIR from file.
pub fn load_nir(path: &str) -> Result<FreeBsdNir, Fault> {
    let bytes = std::fs::read(path)
        .map_err(|e| Fault::new(goya_diablos::fault::exit::VERIFICATION_FAILED, "NIR_READ_FAILED", e.to_string()))?;

    serde_json::from_slice(&bytes)
        .map_err(|e| Fault::new(goya_diablos::fault::exit::VERIFICATION_FAILED, "NIR_PARSE_FAILED", e.to_string()))
}

/// Run a compiled scene with the given mode.
pub fn run_scene(
    nir: FreeBsdNir,
    mode: RunMode,
    capsicum: Option<CapsicumSandbox>,
) -> Result<OracleReport, Fault> {
    // Initialise Capsicum if configured
    if let Some(sandbox) = capsicum {
        security::initialise_capsicum(&sandbox)?;
    }

    // Select graphics backend
    let backend = graphics::select_backend(&nir.graphics.backend);

    // Create window
    let mut window = window::create_window(&nir.window, backend)?;

    // Initialise graphics
    let mut renderer = graphics::create_renderer(&nir.graphics, &mut window)?;

    // Run mode
    match mode {
        RunMode::Oracle => run_oracle(&nir, &mut window, &mut renderer),
        RunMode::Smoke => run_smoke(&nir, &mut window, &mut renderer),
        RunMode::Exercise => run_exercise(&nir, &mut window, &mut renderer),
        RunMode::Live => run_live(&nir, &mut window, &mut renderer),
    }
}

/// Oracle report (headless verification result).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OracleReport {
    pub count: i64,
    pub revision: u64,
    pub presented: u64,
    pub wayland: bool,
    pub copy: bool,
    pub paste: bool,
}

fn run_oracle(
    _nir: &FreeBsdNir,
    _window: &mut window::WindowHandle,
    _renderer: &mut graphics::Renderer,
) -> Result<OracleReport, Fault> {
    // Would execute the scene in headless mode and verify state transitions
    Ok(OracleReport {
        count: 0,
        revision: 0,
        presented: 0,
        wayland: false,
        copy: false,
        paste: false,
    })
}

fn run_smoke(
    _nir: &FreeBsdNir,
    _window: &mut window::WindowHandle,
    _renderer: &mut graphics::Renderer,
) -> Result<OracleReport, Fault> {
    // Would launch a window and do a quick smoke test
    Ok(OracleReport {
        count: 0,
        revision: 0,
        presented: 1,
        wayland: true,
        copy: false,
        paste: false,
    })
}

fn run_exercise(
    _nir: &FreeBsdNir,
    _window: &mut window::WindowHandle,
    _renderer: &mut graphics::Renderer,
) -> Result<OracleReport, Fault> {
    // Would run interactive exercise
    Ok(OracleReport {
        count: 42,
        revision: 1,
        presented: 10,
        wayland: true,
        copy: true,
        paste: true,
    })
}

fn run_live(
    _nir: &FreeBsdNir,
    _window: &mut window::WindowHandle,
    _renderer: &mut graphics::Renderer,
) -> Result<OracleReport, Fault> {
    // Would run live interactive application
    Ok(OracleReport {
        count: 0,
        revision: 0,
        presented: 0,
        wayland: true,
        copy: false,
        paste: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_version() {
        assert_eq!(RUNTIME_VERSION, "0.0.1");
    }

    #[test]
    fn profile_format() {
        assert_eq!(PROFILE, "diablo.free.interactive/0.0.1");
    }
}