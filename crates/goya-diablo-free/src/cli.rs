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

    /// Disable .escene artefact generation (enabled by default)
    #[arg(long, global = true)]
    no_escene: bool,

    /// Output format
    #[arg(long, global = true, value_enum, default_value = "text")]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
#[derive(PartialEq, Eq)]
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
    let escene_enabled = !cli.no_escene;
    let escene_out = cli.out.clone();
    let escene_target = cli.target.clone();
    let escene_format_text = matches!(cli.format, OutputFormat::Text);
    let escene_scene = cli_scene_path(&cli.command);

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
            inv.escene_disabled = cli.no_escene;
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

    // ESCENE generation (GOYA-ESCENE-001 §6.2): on by default for scene
    // commands. Failure never invalidates the build result (ESC-INV-012).
    if escene_enabled {
        if let Some(scene) = escene_scene {
            // §6.1: failed and blocked compilations still generate a partial
            // artefact where feasible; only admission rejection skips it.
            if run.code != goya_diablos::fault::exit::ADMISSION_FAILED {
                let inv = default_invocation(CommandKind::Build, Some(scene), escene_target);
                let out_dir = escene_out.unwrap_or_else(|| PathBuf::from("dist"));
                match goya_diablos::escene::generate_for_invocation(&inv, &out_dir) {
                    Ok(path) => {
                        if escene_format_text {
                            println!("escene: {}", path.display());
                        }
                    }
                    Err(e) => eprintln!("warning: escene generation failed (build unaffected): {e}"),
                }
            }
        }
    }

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

fn cli_scene_path(command: &Commands) -> Option<PathBuf> {
    match command {
        Commands::Inspect { scene } | Commands::Check { scene } | Commands::Build { scene } => {
            Some(scene.clone())
        }
        _ => None,
    }
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