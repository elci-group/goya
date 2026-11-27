//! CASA — CSS-to-.gscene semantic transformation infrastructure
//!
//! Implements directive GOYA-DIS-CASA-001: a reusable, deterministic CSS transformation
//! engine for the Goya ecosystem that liberates CSS-expressed design intent from
//! browser-dependent rendering architectures, reconstructing that intent through
//! Goya's native geometry, graphics, appearance, state management and adaptive
//! presentation infrastructure.

pub mod adaptive;
pub mod animation;
pub mod appearance;
pub mod behavioural;
pub mod cascade;
pub mod cli;
pub mod context;
pub mod diagnostics;
pub mod emission;
pub mod geometry;
pub mod ingestion;
pub mod integration;
pub mod ir;
pub mod modes;
pub mod optimisation;
pub mod resources;
pub mod typography;
pub mod validation;

/// CASA directive identifier
pub const DIRECTIVE_ID: &str = "GOYA-DIS-CASA-001";

/// CASA version
pub const VERSION: &str = "1.0";

/// CASA contract identifier
pub const CONTRACT: &str = "GY-CASA/1.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_compiles() {
        // Verify the crate compiles and exports correctly
        assert_eq!(DIRECTIVE_ID, "GOYA-DIS-CASA-001");
        assert_eq!(VERSION, "1.0");
        assert_eq!(CONTRACT, "GY-CASA/1.0");
    }
}
