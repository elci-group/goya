//! `arachnid` — Goya's webOS TV compiler.
//!
//! ```text
//! arachnid platforms                         webOS release → engine and feature table
//! arachnid models [--year YYYY]              per-series caprichio table (2018–2026)
//! arachnid lookup <target>                   resolve a model, version or user agent
//! arachnid check <scene> [--target T]        admit and gate without emitting
//! arachnid build <scene> --id ID --version V --title T --out NEW_DIR
//!                [--vendor NAME] [--icon icon.png] [--target T]
//! arachnid getkey --host TV_IP --out KEY_FILE   fetch the Developer Mode key
//! arachnid deploy <app.ipk> --host TV_IP --key KEY_FILE [--launch yes] [--dry-run yes]
//! ```
//! A target is `model:<number>`, `webos:<version>`, `reported:<version>` or
//! `ua:<agent>`. Without one, builds use the floor, webOS TV 4.0.

use arachnid::appinfo::AppInfo;
use arachnid::build::{self, BuildOptions};
use arachnid::deploy;
use arachnid::diagnostics::Diagnostic;
use arachnid::target;
use goya_caprichios::webos_engine_caprichio::{WebFeature, WebosPlatform, PLATFORMS};
use goya_caprichios::webos_tv_models::CATALOG;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  arachnid platforms\n  arachnid models [--year YYYY]\n  arachnid lookup <target>\n  arachnid check <scene> [--target T]\n  arachnid build <scene> --id ID --version V --title T --out NEW_DIR [--vendor NAME] [--icon PNG] [--target T]\n  arachnid getkey --host TV_IP --out KEY_FILE\n  arachnid deploy <app.ipk> --host TV_IP --key KEY_FILE [--launch yes] [--dry-run yes]\n\ntargets: model:<number> | webos:<version> | reported:<version> | ua:<agent>"
    );
    ExitCode::from(2)
}

fn report(diags: &[Diagnostic], file: Option<&str>) -> ExitCode {
    for d in diags {
        match file {
            Some(f) => eprintln!("{f}: {d}"),
            None => eprintln!("{d}"),
        }
    }
    ExitCode::FAILURE
}

/// `--flag value` options after the positional arguments.
fn options(args: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let Some(name) = flag.strip_prefix("--") else {
            return Err(format!("unexpected argument {flag}"));
        };
        let value = it.next().ok_or_else(|| format!("--{name} needs a value"))?;
        if out.iter().any(|(n, _): &(String, String)| n == name) {
            return Err(format!("--{name} given twice"));
        }
        out.push((name.to_string(), value.clone()));
    }
    Ok(out)
}

fn take(opts: &[(String, String)], name: &str) -> Option<String> {
    opts.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
}

fn platforms() {
    println!(
        "{:<6} {:<14} {:<5} {:<9} {:<16} features",
        "id", "marketing", "year", "reports", "chromium"
    );
    for f in &PLATFORMS {
        let features: Vec<&str> = WebFeature::ALL
            .iter()
            .filter(|w| f.platform.supports(**w))
            .map(|w| w.name())
            .collect();
        println!(
            "{:<6} {:<14} {:<5} {:<9} {:<16} {}",
            f.platform.id(),
            f.marketing,
            f.first_model_year,
            format!("{}.{}+", f.reported_major, f.reported_minor_min),
            format!("{} ({})", f.engine.chromium_major, f.engine.chrome_version),
            features.join(",")
        );
    }
}

fn models(year: Option<u16>) {
    println!(
        "{:<5} {:<28} {:<7} {:<8} {:<8} {:<10} caprichio",
        "year", "series", "code", "shipped", "chromium", "ceiling"
    );
    for s in CATALOG
        .iter()
        .filter(|s| year.is_none_or(|y| s.model_year == y))
    {
        println!(
            "{:<5} {:<28} {:<7} {:<8} {:<8} {:<10} {}",
            s.model_year,
            s.name,
            s.code,
            s.shipped.id(),
            s.shipped.engine().chromium_major,
            if s.upgrade_ceiling > s.shipped {
                s.upgrade_ceiling.id()
            } else {
                "-"
            },
            s.contract
        );
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first() else {
        return usage();
    };
    match command.as_str() {
        "platforms" if args.len() == 1 => {
            platforms();
            ExitCode::SUCCESS
        }
        "models" => {
            let opts = match options(&args[1..]) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{e}");
                    return usage();
                }
            };
            let year = match take(&opts, "year").map(|y| y.parse::<u16>()) {
                Some(Ok(y)) => Some(y),
                Some(Err(_)) => return usage(),
                None => None,
            };
            models(year);
            ExitCode::SUCCESS
        }
        "lookup" if args.len() == 2 => match target::resolve(Some(&args[1])) {
            Ok(t) => {
                let p: WebosPlatform = t.platform;
                println!("{}", t.description);
                println!("platform: {} ({})", p.id(), p);
                println!(
                    "engine:   Chromium {} ({})",
                    p.engine().chromium_major,
                    p.engine().chrome_version
                );
                if let Some(c) = t.series_contract {
                    println!("caprichio: {c}");
                }
                ExitCode::SUCCESS
            }
            Err(d) => report(&[d], None),
        },
        "check" | "build" if args.len() >= 2 => {
            let file = &args[1];
            let opts = match options(&args[2..]) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("{e}");
                    return usage();
                }
            };
            let source = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{file}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let tgt = match target::resolve(take(&opts, "target").as_deref()) {
                Ok(t) => t,
                Err(d) => return report(&[d], Some(file)),
            };
            if command == "check" {
                if opts.iter().any(|(n, _)| n != "target") {
                    return usage();
                }
                return match build::check(&source, &tgt) {
                    Ok(o) => {
                        println!(
                            "{file}: admitted for {} — {} nodes, {} actionable, focus graph reachable and trap-free",
                            tgt.description,
                            o.frame.nodes.len(),
                            o.actionable.len()
                        );
                        ExitCode::SUCCESS
                    }
                    Err(d) => report(&d, Some(file)),
                };
            }
            let known = ["id", "version", "title", "vendor", "icon", "target", "out"];
            if let Some((n, _)) = opts.iter().find(|(n, _)| !known.contains(&n.as_str())) {
                eprintln!("unknown option --{n}");
                return usage();
            }
            let (Some(id), Some(version), Some(title), Some(out)) = (
                take(&opts, "id"),
                take(&opts, "version"),
                take(&opts, "title"),
                take(&opts, "out"),
            ) else {
                return usage();
            };
            let icon = match take(&opts, "icon").map(std::fs::read) {
                Some(Ok(b)) => Some(b),
                Some(Err(e)) => {
                    eprintln!("icon: {e}");
                    return ExitCode::FAILURE;
                }
                None => None,
            };
            let bo = BuildOptions {
                app: AppInfo {
                    id,
                    title,
                    version,
                    vendor: take(&opts, "vendor"),
                },
                target: tgt,
                icon,
            };
            let output = match build::build(&source, &bo) {
                Ok(o) => o,
                Err(d) => return report(&d, Some(file)),
            };
            let out = PathBuf::from(out);
            if let Err(e) = build::write(&output, &out) {
                eprintln!("{}: {e}", out.display());
                return ExitCode::FAILURE;
            }
            println!(
                "{}: built {} for {}",
                file,
                out.join(&output.ipk.file_name).display(),
                bo.target.description
            );
            ExitCode::SUCCESS
        }
        "getkey" => {
            let Ok(opts) = options(&args[1..]) else {
                return usage();
            };
            let (Some(host), Some(out)) = (take(&opts, "host"), take(&opts, "out")) else {
                return usage();
            };
            match deploy::fetch_key(&host, deploy::KEY_PORT, &PathBuf::from(&out)) {
                Ok(()) => {
                    println!("saved the Developer Mode key to {out} (encrypted; ssh will ask for the passphrase shown on the TV)");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("getkey: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "deploy" if args.len() >= 2 => {
            let ipk = PathBuf::from(&args[1]);
            let Ok(opts) = options(&args[2..]) else {
                return usage();
            };
            let (Some(host), Some(key)) = (take(&opts, "host"), take(&opts, "key")) else {
                return usage();
            };
            let launch = take(&opts, "launch").as_deref() == Some("yes");
            let name = ipk.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // ares naming: <id>_<version>_<arch>.ipk
            let Some(app_id) = name
                .strip_suffix(".ipk")
                .and_then(|n| n.split('_').next())
                .filter(|i| !i.is_empty())
            else {
                eprintln!("deploy: expected <id>_<version>_all.ipk, got {name:?}");
                return ExitCode::FAILURE;
            };
            let control = std::env::temp_dir().join(format!("arachnid-ssh-{}", std::process::id()));
            let device = deploy::Device {
                host,
                key: PathBuf::from(key),
                control,
            };
            if take(&opts, "dry-run").as_deref() == Some("yes") {
                let ssh = device.ssh_args().join(" ");
                for step in deploy::plan(name, app_id, launch) {
                    println!("ssh {ssh} -- {}", step);
                }
                return ExitCode::SUCCESS;
            }
            match deploy::deploy(&device, &ipk, app_id, launch) {
                Ok(()) => {
                    println!(
                        "installed {app_id} on {}{}",
                        device.host,
                        if launch { " and launched it" } else { "" }
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("deploy: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        _ => usage(),
    }
}
