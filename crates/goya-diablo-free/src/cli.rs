//! Diablo Free CLI.

use crate::{Fault, COMPILER_VERSION, orchestrate::{default_invocation, execute}, target::{FREEBSD_TRIPLES, classify_target, TargetClass}};
use goya_diablos::pipeline::CommandKind;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "diablo-free", version = COMPILER_VERSION, about = "Diablo Free: FreeBSD native compiler for Goya")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Target triple (e.g., x86_64-unknown-freebsd)
    #[arg(long, global = true, default_value = "x86_64-unknown-freebsd")]
    target: String,

    /// Profile (balanced, development, performance, small, low-power, constrained)
    #[arg(long, global = true, default_value = "balanced")]
    profile: String,

    /// Enable release optimisations
    #[arg(long, global = true)]
    release: bool,

    /// Output directory
    #[arg(long, global = true)]
    out: Option<PathBuf>,

    /// Evidence directory
    #[arg(long, global = true)]
    evidence: Option<PathBuf>,

    /// Launch after build (verify only)
    #[arg(long, global = true)]
    launch: bool,

    /// Localised compilation (resolve actual hardware)
    #[arg(long, global = true)]
    localised: bool,

    /// Test/benchmark suite
    #[arg(long, global = true)]
    suite: Option<String>,

    /// Package format
    #[arg(long, global = true)]
    package_format: Option<String>,

    /// Window/graphics backend
    #[arg(long, global = true, default_value = "auto")]
    window: String,

    /// Desktop profile
    #[arg(long, global = true)]
    desktop: Option<String>,

    /// XDG config root
    #[arg(long, global = true)]
    config_root: Option<PathBuf>,

    /// Output format
    #[arg(long, global = true, value_enum, default_value = "text")]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Subcommand)]
enum Commands {
    /// Inspect a scene (admission only)
    Inspect {
        scene: PathBuf,
    },
    /// Check a scene (admission + validation)
    Check {
        scene: PathBuf,
    },
    /// Build an executable
    Build {
        scene: PathBuf,
    },
    /// Verify a build directory
    Verify {
        directory: PathBuf,
    },
    /// Run tests
    Test {
        scene: PathBuf,
    },
    /// Run benchmarks
    Benchmark {
        scene: PathBuf,
    },
    /// Package a build
    Package {
        directory: PathBuf,
    },
    /// Qualify a target
    Qualify {
        directory: PathBuf,
    },
    /// List supported targets
    Targets,
}

pub fn main() {
    let cli = Cli::parse();

    let run = match cli.command {
        Commands::Inspect { scene } => execute(default_invocation(
            CommandKind::Inspect,
            Some(scene),
            cli.target.clone(),
        )),
        Commands::Check { scene } => execute(default_invocation(
            CommandKind::Check,
            Some(scene),
            cli.target.clone(),
        )),
        Commands::Build { scene } => {
            let mut inv = default_invocation(CommandKind::Build, Some(scene), cli.target.clone());
            inv.release = cli.release;
            inv.out = cli.out;
            inv.evidence = cli.evidence;
            inv.launch = cli.launch;
            inv.localised = cli.localised;
            inv.window = cli.window;
            inv.desktop = cli.desktop;
            inv.config_root = cli.config_root;
            execute(inv)
        }
        Commands::Verify { directory } => {
            let mut inv = default_invocation(CommandKind::Verify, None, cli.target.clone());
            inv.directory = Some(directory);
            inv.launch = cli.launch;
            execute(inv)
        }
        Commands::Test { scene } => {
            let mut inv = default_invocation(CommandKind::Test, Some(scene), cli.target.clone());
            inv.suite = cli.suite;
            execute(inv)
        }
        Commands::Benchmark { scene } => {
            let mut inv = default_invocation(CommandKind::Benchmark, Some(scene), cli.target.clone());
            inv.suite = cli.suite;
            execute(inv)
        }
        Commands::Package { directory } => {
            let mut inv = default_invocation(CommandKind::Package, None, cli.target.clone());
            inv.directory = Some(directory);
            inv.package_format = cli.package_format;
            execute(inv)
        }
        Commands::Qualify { directory } => {
            let mut inv = default_invocation(CommandKind::Qualify, None, cli.target.clone());
            inv.directory = Some(directory);
            execute(inv)
        }
        Commands::Targets => {
            print_targets();
            return;
        }
    };

    match cli.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&run.json).unwrap());
        }
        OutputFormat::Text => {
            println!("{}", run.text);
            if run.code != 0 {
                eprintln!("Exit code: {}", run.code);
            }
        }
    }

    std::process::exit(run.code as i32);
}

fn print_targets() {
    println!("Supported FreeBSD targets:");
    for triple in FREEBSD_TRIPLES {
        let class = classify_target(triple);
        let status = match class {
            TargetClass::Host => " (host)",
            TargetClass::Cross => " (cross)",
            TargetClass::Blocked => " (blocked)",
            TargetClass::Unknown => " (unknown)",
        };
        println!("  {}{}", triple, status);
    }
    println!("\nProfiles: balanced (default), development, performance, small, low-power, constrained");
    println!("Window backends: auto (default), drm, wayland, x11, software");
}