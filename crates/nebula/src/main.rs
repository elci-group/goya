//! `nebula` command line.
//!
//! ```text
//! nebula probe [--json] [--out FILE] [--config-root DIR]
//! nebula theme [--json] [--config-root DIR] [--variant dark|light]
//! ```
//!
//! Exit codes: 0 ok, 1 theme unreadable (`theme` only), 2 usage,
//! 3 not a COSMIC Wayland session (BLOCKED; the report is still written),
//! 4 required protocol missing.

use nebula::probe::{self, ProbeReport, Status, ThemeSummary};
use nebula::protocols::{Need, Presence};
use nebula::theme::{ThemeSource, Variant};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage:
  nebula probe [--json] [--out FILE] [--config-root DIR]
  nebula theme [--json] [--config-root DIR] [--variant dark|light]

exit codes: 0 ok, 1 theme unreadable (theme), 2 usage,
            3 not a COSMIC Wayland session (BLOCKED), 4 required protocol missing";

struct Opts {
    json: bool,
    out: Option<PathBuf>,
    config_root: Option<PathBuf>,
    variant: Option<Variant>,
}

fn parse(args: &[String], allow_out: bool, allow_variant: bool) -> Result<Opts, String> {
    let mut o = Opts {
        json: false,
        out: None,
        config_root: None,
        variant: None,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match a.as_str() {
            "--json" => o.json = true,
            "--out" if allow_out => o.out = Some(value("--out")?.into()),
            "--config-root" => o.config_root = Some(value("--config-root")?.into()),
            "--variant" if allow_variant => {
                o.variant = Some(match value("--variant")?.as_str() {
                    "dark" => Variant::Dark,
                    "light" => Variant::Light,
                    v => return Err(format!("unknown variant {v:?}")),
                })
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    Ok(o)
}

fn source(o: &Opts) -> Option<ThemeSource> {
    o.config_root
        .clone()
        .map(ThemeSource::new)
        .or_else(ThemeSource::from_env)
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("nebula: {msg}\n{USAGE}");
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("probe") => match parse(&args[1..], true, false) {
            Ok(o) => cmd_probe(&o),
            Err(e) => usage_error(&e),
        },
        Some("theme") => match parse(&args[1..], false, true) {
            Ok(o) => cmd_theme(&o),
            Err(e) => usage_error(&e),
        },
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => usage_error(&format!("unknown command {other:?}")),
        None => usage_error("missing command"),
    }
}

fn cmd_probe(o: &Opts) -> ExitCode {
    let report = probe::run(source(o).as_ref());
    let json = match serde_json::to_string_pretty(&report) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("nebula: cannot serialise report: {e}");
            return ExitCode::from(1);
        }
    };
    if let Some(path) = &o.out {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("nebula: cannot create {}: {e}", parent.display());
                return ExitCode::from(1);
            }
        }
        if let Err(e) = std::fs::write(path, format!("{json}\n")) {
            eprintln!("nebula: cannot write {}: {e}", path.display());
            return ExitCode::from(1);
        }
    }
    if o.json {
        println!("{json}");
    } else {
        print_probe(&report);
    }
    ExitCode::from(report.status.exit_code() as u8)
}

fn print_probe(r: &ProbeReport) {
    let status = match r.status {
        Status::Ok => "OK",
        Status::Blocked => "BLOCKED",
        Status::RequiredProtocolMissing => "REQUIRED_PROTOCOL_MISSING",
    };
    println!("{} {} at {}", r.schema, status, r.generated_utc);
    println!("scope: {}", r.scope.statement);
    for reason in &r.status_reasons {
        println!("  reason: {reason}");
    }
    let s = &r.session;
    println!(
        "session: cosmic={} wayland={} WAYLAND_DISPLAY={}",
        s.is_cosmic_desktop,
        s.is_wayland_session,
        s.env.wayland_display.as_deref().unwrap_or("<unset>")
    );
    if let Some(x) = &s.x11_display {
        println!("  DISPLAY={} ({}; not qualifying)", x.display, x.kind);
    }
    println!(
        "compositor: {} {} (method {})",
        r.compositor.package, r.compositor.version, r.compositor.method
    );
    let running = &r.compositor.running_process;
    println!(
        "running compositor process: {} (method {}; version unknown; installed-package match unverified)",
        running.status, running.method
    );
    for process in &running.processes {
        println!(
            "  pid {} comm={} exe={}{}",
            process.pid,
            process.comm,
            process.executable.as_deref().unwrap_or("<unavailable>"),
            if process.executable_deleted {
                " [deleted]"
            } else {
                ""
            }
        );
    }
    if let Some(detail) = &running.detail {
        println!("  detail: {detail}");
    }
    if let Some(p) = &r.protocols {
        let c = &p.counts;
        println!(
            "protocols: required {}/{}, optional {}/{}, cosmic-private observed {} (not used), unlisted {}, total {}",
            c.required_advertised,
            c.required_total,
            c.optional_advertised,
            c.optional_total,
            c.cosmic_private_observed,
            c.unlisted_observed,
            c.globals_observed
        );
        for e in &p.entries {
            if e.need != Need::CosmicPrivateNotUsed && e.presence != Presence::Advertised {
                println!("  fallback: {} -> {}", e.interface, e.fallback);
            }
        }
    }
    for sel in &r.caprichios {
        println!("caprichio: {} {:?}: {}", sel.id, sel.status, sel.reason);
    }
    print_theme(&r.theme);
}

fn print_theme(t: &ThemeSummary) {
    if let Some(e) = &t.error {
        println!("theme: unreadable: {e}");
        return;
    }
    println!(
        "theme: {} ({:?}, format {}) from {}",
        t.name.as_deref().unwrap_or("<unnamed>"),
        t.variant.unwrap_or(Variant::Dark),
        t.format,
        t.config_root
    );
    if !t.other_versions_present.is_empty() {
        println!(
            "  other format versions present, not read: {}",
            t.other_versions_present.join(", ")
        );
    }
    for i in &t.issues {
        println!("  degraded key {}: {:?}", i.key, i.problem);
    }
    if let Some(c) = &t.chroma {
        let hex = |v: Option<goya_chroma::Rgb8>| v.map_or("-".to_string(), |c| c.to_string());
        let r = &c.roles;
        println!(
            "  chroma ({}): bg {} fg {} muted {} accent {} border {} ok {} err {} warning {}",
            c.polarity,
            hex(r.bg),
            hex(r.fg),
            hex(r.muted),
            hex(r.accent),
            hex(r.border),
            hex(r.ok),
            hex(r.err),
            hex(r.warning)
        );
        for k in &c.contrast {
            println!(
                "  contrast {} {:.2} ({})",
                k.pair,
                k.ratio,
                if k.meets_target {
                    "meets target"
                } else {
                    "below target"
                }
            );
        }
    }
    if let Some(g) = &t.geometry {
        println!(
            "  geometry: gaps {:?}, active_hint {:?}, frosted {:?}, radius_m {:?}",
            g.gaps,
            g.active_hint,
            g.is_frosted,
            g.corner_radii.as_ref().and_then(|r| r.radius_m)
        );
    }
}

fn cmd_theme(o: &Opts) -> ExitCode {
    let summary = probe::theme_summary(source(o).as_ref(), o.variant);
    if o.json {
        let value = serde_json::json!({
            "schema": nebula::THEME_SCHEMA,
            "nebula_version": nebula::NEBULA_VERSION,
            "theme": summary,
        });
        match serde_json::to_string_pretty(&value) {
            Ok(j) => println!("{j}"),
            Err(e) => {
                eprintln!("nebula: cannot serialise theme: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        print_theme(&summary);
    }
    if summary.error.is_some() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
