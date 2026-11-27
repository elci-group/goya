//! CASA command-line interface entry point.

use clap::{Parser, Subcommand};
use goya_casa::cli::{commands, args};

#[derive(Parser)]
#[command(name = "casa", version, about = "CASA — CSS-to-.gscene semantic transformation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect source semantics and compatibility
    Analyse(args::AnalyseArgs),
    /// Generate native scene representations
    Convert(args::ConvertArgs),
    /// Validate converted artifacts
    Validate(args::ValidateArgs),
    /// Examine IR and dependency relationships
    Inspect(args::InspectArgs),
    /// Compare reference and native presentation
    Compare(args::CompareArgs),
    /// Execute defined performance workloads
    Benchmark(args::BenchmarkArgs),
    /// Explain conversion decisions
    Explain(args::ExplainArgs),
    /// Report implemented feature support
    Capabilities(args::CapabilitiesArgs),
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Analyse(args) => commands::analyse(args),
        Commands::Convert(args) => commands::convert(args),
        Commands::Validate(args) => commands::validate(args),
        Commands::Inspect(args) => commands::inspect(args),
        Commands::Compare(args) => commands::compare(args),
        Commands::Benchmark(args) => commands::benchmark(args),
        Commands::Explain(args) => commands::explain(args),
        Commands::Capabilities(args) => commands::capabilities(args),
    }
}