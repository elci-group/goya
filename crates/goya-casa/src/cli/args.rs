//! CLI argument structures.

use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Args)]
pub struct AnalyseArgs {
    /// Input CSS file(s)
    #[arg(short, long)]
    pub input: Vec<PathBuf>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Strict mode - fail on unsupported constructs
    #[arg(long)]
    pub strict: bool,
}

#[derive(Args)]
pub struct ConvertArgs {
    /// Input CSS file
    #[arg(short, long)]
    pub input: PathBuf,

    /// Structural context (JSON)
    #[arg(short, long)]
    pub context: Option<PathBuf>,

    /// Environmental constraints (JSON)
    #[arg(long)]
    pub environment: Option<PathBuf>,

    /// Output .gscene file
    #[arg(short, long)]
    pub output: PathBuf,

    /// Optimisation profile
    #[arg(long, value_enum, default_value = "balanced")]
    pub profile: OptimisationProfile,

    /// Strict mode
    #[arg(long)]
    pub strict: bool,

    /// Conversion report output
    #[arg(long)]
    pub report: Option<PathBuf>,
}

#[derive(Args)]
pub struct ValidateArgs {
    /// Artifact to validate
    #[arg(short, long)]
    pub artifact: PathBuf,

    /// Validation level
    #[arg(long, value_enum, default_value = "full")]
    pub level: ValidationLevel,
}

#[derive(Args)]
pub struct InspectArgs {
    /// Input CSS file
    #[arg(short, long)]
    pub input: PathBuf,

    /// Inspection target
    #[arg(long, value_enum, default_value = "ir")]
    pub target: InspectionTarget,
}

#[derive(Args)]
pub struct CompareArgs {
    /// Reference output
    #[arg(long)]
    pub reference: PathBuf,

    /// Native output
    #[arg(long)]
    pub native: PathBuf,

    /// Comparison mode
    #[arg(long, value_enum, default_value = "visual")]
    pub mode: ComparisonMode,
}

#[derive(Args)]
pub struct BenchmarkArgs {
    /// Benchmark workload
    #[arg(long, value_enum)]
    pub workload: BenchmarkWorkload,

    /// Number of iterations
    #[arg(long, default_value = "10")]
    pub iterations: usize,
}

#[derive(Args)]
pub struct ExplainArgs {
    /// Input CSS file
    #[arg(short, long)]
    pub input: PathBuf,

    /// Specific property/selector to explain
    #[arg(long)]
    pub target: Option<String>,
}

#[derive(Args)]
pub struct CapabilitiesArgs {
    /// Output format
    #[arg(short, long, value_enum, default_value = "text")]
    pub format: OutputFormat,
}

#[derive(Copy, Clone, ValueEnum, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Yaml,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum OptimisationProfile {
    Fidelity,
    Compact,
    Performance,
    Constrained,
    Balanced,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum ValidationLevel {
    Basic,
    Full,
    Strict,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum InspectionTarget {
    Ir,
    Cascade,
    Dependencies,
    Geometry,
    Appearance,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum ComparisonMode {
    Visual,
    Structural,
    Semantic,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum BenchmarkWorkload {
    StaticComponentLibrary,
    ResponsiveDashboard,
    AnimatedInterface,
    ComplexGridApplication,
    TextIntensiveDocument,
    ElectronApplication,
    VectorRichIllustration,
    TelevisionInterface,
    ConstrainedMobileInterface,
    DynamicallyStyledApplication,
}
