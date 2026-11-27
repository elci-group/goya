//! CASA — CSS-to-.gscene semantic transformation infrastructure
//!
//! Implements directive GOYA-DIS-CASA-001: a reusable, deterministic CSS transformation
//! engine for the Goya ecosystem that liberates CSS-expressed design intent from
//! browser-dependent rendering architectures, reconstructing that intent through
//! Goya's native geometry, graphics, appearance, state management and adaptive
//! presentation infrastructure.

pub mod ingestion;
pub mod cascade;
pub mod ir;
pub mod context;
pub mod geometry;
pub mod appearance;
pub mod typography;
pub mod animation;
pub mod adaptive;
pub mod behavioural;
pub mod resources;
pub mod optimisation;
pub mod emission;
pub mod validation;
pub mod diagnostics;
pub mod modes;
pub mod cli;
pub mod integration;

/// CASA directive identifier
pub const DIRECTIVE_ID: &str = "GOYA-DIS-CASA-001";

/// CASA version
pub const VERSION: &str = "1.0";

/// CASA contract identifier
pub const CONTRACT: &str = "GY-CASA/1.0";

#[cfg(test)]
mod tests {
    #[test]
    fn crate_compiles() {
        // Verify the crate compiles and exports correctly
        assert_eq!(DIRECTIVE_ID, "GOYA-DIS-CASA-001");
        assert_eq!(VERSION, "1.0");
        assert_eq!(CONTRACT, "GY-CASA/1.0");
    }
}