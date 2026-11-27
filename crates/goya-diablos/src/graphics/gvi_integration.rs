//! GVI (Graphical Vector Inventory) integration for Diablos.

use crate::Fault;
use goya_gvi::{StateModel, Budget, Domain, Reachability, compile_with, CompileOptions, Bake, AlbitProfile, OperativeProfile};
use goya_gva::GvaLimits;
use goya_graphics_ir::Drawing;
use std::path::Path;

/// GVI compilation options for Diablos.
#[derive(Debug, Clone)]
pub struct GviOptions {
    /// Target domain (screen, print, headless)
    pub domain: Domain,
    /// Budget constraints
    pub budget: Budget,
    /// Reachability analysis
    pub reachability: Reachability,
    /// Bake profile for transform baking
    pub bake_profile: BakeProfile,
    /// Output format
    pub format: GviFormat,
    /// GVA limits
    pub gva_limits: GvaLimits,
}

/// Bake profile selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BakeProfile {
    /// No transform baking
    None,
    /// Transforms baked
    Transforms,
}

/// Output format for GVI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GviFormat {
    /// Binary GVI format
    Binary,
    /// JSON for debugging
    Json,
}

/// Compile a StateModel to GVI binary bytes.
pub fn compile_gvi(
    source: &Drawing,
    model: &StateModel,
    options: &GviOptions,
) -> Result<Vec<u8>, Fault> {
    let compile_options = CompileOptions {
        bake: match options.bake_profile {
            BakeProfile::None => Bake::None,
            BakeProfile::Transforms => Bake::Transforms,
        },
        max_diff_depth: 0,
        albit: AlbitProfile::Off,
        operative: OperativeProfile::Off,
    };

    compile_with(source, model, &options.budget, &options.gva_limits, compile_options)
        .map_err(|e| Fault::new(crate::fault::exit::VERIFICATION_FAILED, "GVI_COMPILATION_FAILED", e.to_string()))
}

/// Serialize GVI bytes to file.
pub fn write_gvi(bytes: &[u8], path: &Path, _format: GviFormat) -> Result<(), Fault> {
    std::fs::write(path, bytes)
        .map_err(|e| Fault::new(crate::fault::exit::VERIFICATION_FAILED, "GVI_WRITE_FAILED", e.to_string()))
}

/// Create a default StateModel for testing.
pub fn default_state_model() -> StateModel {
    StateModel {
        parent: "test".to_string(),
        dimensions: vec![],
        reachability: Reachability::Transitions {
            initial: vec![],
            rules: vec![],
        },
    }
}

/// GVI-D (Differential) integration - placeholder for future.
pub mod gvi_d {
    use super::*;
    
    /// Compile with differential geometry storage.
    pub fn compile_gvi_d(_model: &StateModel, _options: &GviOptions) -> Result<Vec<u8>, Fault> {
        Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "GVI_D_NOT_IMPLEMENTED", "GVI-D not implemented in this cycle"))
    }
}

/// GVI-A (Animated/Trajectory) integration - placeholder for future.
pub mod gvi_a {
    use super::*;
    
    /// Compile with animated trajectory representations.
    pub fn compile_gvi_a(_model: &StateModel, _options: &GviOptions) -> Result<Vec<u8>, Fault> {
        Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "GVI_A_NOT_IMPLEMENTED", "GVI-A not implemented in this cycle"))
    }
}

/// GVI-L (Colour/Style) integration - placeholder for future.
pub mod gvi_l {
    use super::*;
    
    /// Compile with compact colour/style representations.
    pub fn compile_gvi_l(_model: &StateModel, _options: &GviOptions) -> Result<Vec<u8>, Fault> {
        Err(Fault::new(crate::fault::exit::VERIFICATION_FAILED, "GVI_L_NOT_IMPLEMENTED", "GVI-L not implemented in this cycle"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goya_gva::GvaLimits;

    #[test]
    fn gvi_options_default() {
        let options = GviOptions {
            domain: Domain::Discrete(vec![]),
            budget: Budget::default(),
            reachability: Reachability::Transitions { initial: vec![], rules: vec![] },
            bake_profile: BakeProfile::None,
            format: GviFormat::Json,
            gva_limits: GvaLimits::default(),
        };
        assert_eq!(options.bake_profile, BakeProfile::None);
    }
}