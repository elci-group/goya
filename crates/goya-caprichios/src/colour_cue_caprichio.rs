//! Process-local placement of a colour inventory.
//!
//! Device-local placement is refused. The palette stays in process memory.
//! This recipe does not allocate device memory and does not call a graphics API.

use std::fmt;

pub const CONTRACT: &str = "goya.graphics.colour_cue.placement.v1";
pub const RECIPE: &str = "colour-cue-placement";
pub const RECIPE_VERSION: &str = "0.0.1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    DiscreteGpu,
    IntegratedAmdIntel,
    AppleSilicon,
    AndroidTile,
    EmbeddedIndexed,
    CpuOnly,
    NativeWidgets,
}

impl Target {
    fn as_str(self) -> &'static str {
        match self {
            Self::DiscreteGpu => "DiscreteGpu",
            Self::IntegratedAmdIntel => "IntegratedAmdIntel",
            Self::AppleSilicon => "AppleSilicon",
            Self::AndroidTile => "AndroidTile",
            Self::EmbeddedIndexed => "EmbeddedIndexed",
            Self::CpuOnly => "CpuOnly",
            Self::NativeWidgets => "NativeWidgets",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCapabilities {
    pub software_present: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColourCueDeclaration {
    pub target: Target,
    pub indexed_cheaper: bool,
    pub request_device_local: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColourCueIr {
    target: Target,
    indexed_cheaper: bool,
}

/// Candidate placement before the device-local admission gate.
/// `DeviceLocal` is what `strategy_for` would choose for a discrete GPU.
/// Lowering never admits it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Strategy {
    DirectCpu,
    SoftwarePalette,
    SharedMemoryInventory,
    UnifiedMemory,
    TileDamage,
    PlatformTheme,
    DeviceLocal,
}

impl Strategy {
    fn as_str(self) -> &'static str {
        match self {
            Self::DirectCpu => "DirectCpu",
            Self::SoftwarePalette => "SoftwarePalette",
            Self::SharedMemoryInventory => "SharedMemoryInventory",
            Self::UnifiedMemory => "UnifiedMemory",
            Self::TileDamage => "TileDamage",
            Self::PlatformTheme => "PlatformTheme",
            Self::DeviceLocal => "device-local",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic(pub &'static str);

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Diagnostic {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColourCuePlan {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColourCueArtifact {
    pub contract: &'static str,
    pub recipe: &'static str,
    pub recipe_version: &'static str,
    pub canonical_ir: String,
    pub source: String,
}

/// Natural strategy for a tier, before the indexed-cost and device-local gates.
fn strategy_for(target: Target) -> Strategy {
    match target {
        Target::DiscreteGpu => Strategy::DeviceLocal,
        Target::IntegratedAmdIntel => Strategy::SharedMemoryInventory,
        Target::AppleSilicon => Strategy::UnifiedMemory,
        Target::AndroidTile => Strategy::TileDamage,
        Target::EmbeddedIndexed => Strategy::SoftwarePalette,
        Target::CpuOnly => Strategy::SoftwarePalette,
        Target::NativeWidgets => Strategy::PlatformTheme,
    }
}

fn admit(target: Target, indexed_cheaper: bool) -> Result<Strategy, Diagnostic> {
    let candidate = strategy_for(target);
    if candidate == Strategy::DeviceLocal {
        return Err(Diagnostic("Vulkan is blocked"));
    }
    if matches!(candidate, Strategy::TileDamage | Strategy::PlatformTheme) || indexed_cheaper {
        Ok(candidate)
    } else {
        Ok(Strategy::DirectCpu)
    }
}

fn canonical_ir(target: Target, strategy: Strategy, indexed_cheaper: bool) -> String {
    let mut text = format!(
        "contract={CONTRACT}\nrecipe={RECIPE}\nrecipe_version={RECIPE_VERSION}\ntarget={}\n",
        target.as_str()
    );
    if target == Target::AppleSilicon {
        text.push_str("memory=process-visible-shared\n");
    }
    text.push_str(&format!(
        "strategy={}\nindexed={indexed_cheaper}\nvulkan=blocked\n",
        strategy.as_str()
    ));
    text
}

impl ColourCueDeclaration {
    pub fn normalize(self) -> Result<ColourCueIr, Diagnostic> {
        if self.request_device_local {
            return Err(Diagnostic("Vulkan is blocked"));
        }
        Ok(ColourCueIr {
            target: self.target,
            indexed_cheaper: self.indexed_cheaper,
        })
    }
}

impl ColourCueIr {
    pub fn lower(
        &self,
        target: Target,
        host: RuntimeCapabilities,
    ) -> Result<ColourCueArtifact, Diagnostic> {
        if !host.software_present {
            return Err(Diagnostic("software presentation is required"));
        }
        if strategy_for(target) == Strategy::DeviceLocal
            || strategy_for(self.target) == Strategy::DeviceLocal
        {
            return Err(Diagnostic("Vulkan is blocked"));
        }
        if target != self.target {
            return Err(Diagnostic("colour cue target does not match host"));
        }
        let strategy = admit(target, self.indexed_cheaper)?;
        debug_assert_ne!(strategy, Strategy::DeviceLocal);
        let plan = ColourCuePlan {
            contract: CONTRACT,
            recipe: RECIPE,
            recipe_version: RECIPE_VERSION,
            canonical_ir: canonical_ir(target, strategy, self.indexed_cheaper),
        };
        Ok(ColourCueArtifact {
            contract: plan.contract,
            recipe: plan.recipe,
            recipe_version: plan.recipe_version,
            canonical_ir: plan.canonical_ir,
            source: "// Palette stays in process memory. Vulkan is not called.\n".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn software() -> RuntimeCapabilities {
        RuntimeCapabilities {
            software_present: true,
        }
    }

    fn lower(target: Target, indexed_cheaper: bool) -> Result<ColourCueArtifact, Diagnostic> {
        ColourCueDeclaration {
            target,
            indexed_cheaper,
            request_device_local: false,
        }
        .normalize()?
        .lower(target, software())
    }

    #[test]
    fn device_local_request_is_rejected() {
        let rejected = ColourCueDeclaration {
            target: Target::CpuOnly,
            indexed_cheaper: true,
            request_device_local: true,
        }
        .normalize();
        assert_eq!(rejected, Err(Diagnostic("Vulkan is blocked")));
    }

    #[test]
    fn discrete_gpu_is_rejected() {
        assert_eq!(
            lower(Target::DiscreteGpu, false),
            Err(Diagnostic("Vulkan is blocked"))
        );
        assert_eq!(
            lower(Target::DiscreteGpu, true),
            Err(Diagnostic("Vulkan is blocked"))
        );
    }

    #[test]
    fn integrated_cheaper_selects_shared_memory_inventory() {
        let artifact = lower(Target::IntegratedAmdIntel, true).unwrap();
        assert_eq!(
            artifact.canonical_ir,
            "contract=goya.graphics.colour_cue.placement.v1\n\
             recipe=colour-cue-placement\n\
             recipe_version=0.0.1\n\
             target=IntegratedAmdIntel\n\
             strategy=SharedMemoryInventory\n\
             indexed=true\n\
             vulkan=blocked\n"
        );
        assert!(!artifact.source.contains("vk"));
        assert!(artifact.source.contains("Palette stays in process memory"));
    }

    #[test]
    fn integrated_not_cheaper_selects_direct_cpu() {
        let artifact = lower(Target::IntegratedAmdIntel, false).unwrap();
        assert!(artifact.canonical_ir.contains("strategy=DirectCpu\n"));
        assert!(artifact.canonical_ir.contains("indexed=false\n"));
    }

    #[test]
    fn android_selects_tile_damage() {
        let cheaper = lower(Target::AndroidTile, true).unwrap();
        let direct = lower(Target::AndroidTile, false).unwrap();
        assert!(cheaper.canonical_ir.contains("strategy=TileDamage\n"));
        assert!(direct.canonical_ir.contains("strategy=TileDamage\n"));
    }

    #[test]
    fn apple_ir_is_process_visible_shared_and_not_metal() {
        for indexed in [true, false] {
            let artifact = lower(Target::AppleSilicon, indexed).unwrap();
            assert!(artifact
                .canonical_ir
                .contains("memory=process-visible-shared\n"));
            assert!(!artifact.canonical_ir.contains("metal"));
        }
        assert!(lower(Target::AppleSilicon, true)
            .unwrap()
            .canonical_ir
            .contains("strategy=UnifiedMemory\n"));
        assert!(lower(Target::AppleSilicon, false)
            .unwrap()
            .canonical_ir
            .contains("strategy=DirectCpu\n"));
    }

    #[test]
    fn native_widgets_select_platform_theme() {
        let cheaper = lower(Target::NativeWidgets, true).unwrap();
        let direct = lower(Target::NativeWidgets, false).unwrap();
        assert!(cheaper.canonical_ir.contains("strategy=PlatformTheme\n"));
        assert!(direct.canonical_ir.contains("strategy=PlatformTheme\n"));
    }

    #[test]
    fn missing_software_presentation_is_rejected() {
        let ir = ColourCueDeclaration {
            target: Target::CpuOnly,
            indexed_cheaper: false,
            request_device_local: false,
        }
        .normalize()
        .unwrap();
        assert_eq!(
            ir.lower(
                Target::CpuOnly,
                RuntimeCapabilities {
                    software_present: false,
                }
            ),
            Err(Diagnostic("software presentation is required"))
        );
    }

    #[test]
    fn cpu_and_embedded_follow_indexed_cheaper() {
        assert!(lower(Target::CpuOnly, true)
            .unwrap()
            .canonical_ir
            .contains("strategy=SoftwarePalette\n"));
        assert!(lower(Target::CpuOnly, false)
            .unwrap()
            .canonical_ir
            .contains("strategy=DirectCpu\n"));
        assert!(lower(Target::EmbeddedIndexed, true)
            .unwrap()
            .canonical_ir
            .contains("strategy=SoftwarePalette\n"));
        assert!(lower(Target::EmbeddedIndexed, false)
            .unwrap()
            .canonical_ir
            .contains("strategy=DirectCpu\n"));
    }
}
